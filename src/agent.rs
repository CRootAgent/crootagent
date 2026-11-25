use anyhow::Result;
use colored::*;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use crate::config::Config;
use crate::db::Database;
use crate::diff::DiffViewer;
use crate::document::DocumentEngine;
use crate::provider::{FailoverInfo, LlmProvider};
use crate::session::{ChatMessage, Session, SessionStore};
use crate::tools::ToolRegistry;

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub reply: String,
    pub model_used: String,
    pub failover: Option<FailoverInfo>,
}

pub struct Agent {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: ToolRegistry,
    pub config: Config,
    pub auto_confirm: bool,
    pub db: Option<Arc<std::sync::Mutex<Database>>>,
    pub quiet: bool,
}

impl Clone for Agent {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            tools: self.tools.clone(),
            config: self.config.clone(),
            auto_confirm: self.auto_confirm,
            db: self.db.clone(),
            quiet: self.quiet,
        }
    }
}

impl Agent {
    pub fn update_provider(&mut self, provider_name: &str, model_override: Option<&str>) {
        if let Ok(mut router) = crate::router::LlmRouter::new(provider_name, model_override, &self.config) {
            router.quiet = self.quiet;
            self.provider = Arc::new(router);
        }
    }

    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        config: Config,
        auto_confirm_override: bool,
    ) -> Self {
        Self::with_options(provider, tools, config, auto_confirm_override, false)
    }

    pub fn with_options(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        config: Config,
        auto_confirm_override: bool,
        quiet: bool,
    ) -> Self {
        let auto_confirm = auto_confirm_override || config.auto_confirm;
        let db = Database::open_default().ok().map(|d| Arc::new(std::sync::Mutex::new(d)));
        Self {
            provider,
            tools,
            config,
            auto_confirm,
            db,
            quiet,
        }
    }

    pub fn default_system_prompt() -> String {
        r#"Anda adalah LONTAR CLI, AI Agent modern berbasis Rust yang memadukan keunggulan analisis dokumen (NotebookLM) dan eksekusi coding terminal (OpenCode/Kiro).

Aturan kerja utama:
1. Tanggapi langsung ke inti pertanyaan atau instruksi pengguna secara presisi dan jelas. Jika pengguna menyapa (seperti 'hi' atau 'halo'), balas dengan sapaan ramah singkat dan tanyakan apa yang bisa dibantu.
2. Gunakan pemformatan teks yang rapi, ringkas, dan mudah dibaca di terminal. Utamakan poin-poin (bullet points) ringkas daripada tabel ASCII lebar yang mudah terpotong pada layar sempit.
3. Ketika menganalisis dokumen atau sumber konteks tertera, berikan sitasi yang jelas merujuk pada dokumen tersebut.
4. Gunakan perkakas (tools) yang tersedia untuk membaca file, membuat file, mengedit file, dan menjalankan perintah terminal bila diizinkan.
5. Lakukan perubahan kode secara bedah (surgical edit), jangan merusak kode yang sudah bekerja.
6. Bila menulis rencana implementasi, pastikan langkah-langkahnya dapat diverifikasi secara konkret."#
            .to_string()
    }

    /// Pre-process prompt to handle inline `@filename` syntax (OpenCode style)
    pub fn process_inline_mentions(input: &str) -> String {
        let mut processed = input.to_string();
        let words: Vec<&str> = input.split_whitespace().collect();

        for word in words {
            if word.starts_with('@') && word.len() > 1 {
                let file_ref = &word[1..];
                let path = Path::new(file_ref);
                if path.exists() && path.is_file() {
                    if let Ok(content) = fs::read_to_string(path) {
                        let injection = format!(
                            "\n\n[Konten File @{}]:\n```\n{}\n```\n",
                            file_ref, content
                        );
                        processed.push_str(&injection);
                    }
                }
            }
        }

        processed
    }

    pub async fn step(&self, session: &mut Session, user_input: &str) -> Result<String> {
        Ok(self.step_internal(session, user_input, true, "build").await?.reply)
    }

    pub async fn step_with_mode(&self, session: &mut Session, user_input: &str, mode: &str) -> Result<String> {
        Ok(self.step_with_mode_detailed(session, user_input, mode).await?.reply)
    }

    pub async fn step_with_mode_detailed(&self, session: &mut Session, user_input: &str, mode: &str) -> Result<StepOutcome> {
        let mode_lower = mode.to_lowercase();
        let prompt_adjusted = if mode_lower == "study" {
            format!("[MODE: STUDY - Konteks dokumen NotebookLM aktif]:\n{}", user_input)
        } else if mode_lower == "plan" || mode_lower == "chat" {
            let trimmed = user_input.trim();
            let is_short_greeting = trimmed.eq_ignore_ascii_case("hi")
                || trimmed.eq_ignore_ascii_case("hello")
                || trimmed.eq_ignore_ascii_case("halo")
                || trimmed.eq_ignore_ascii_case("hey")
                || trimmed.eq_ignore_ascii_case("test")
                || trimmed.eq_ignore_ascii_case("selamat pagi")
                || trimmed.eq_ignore_ascii_case("selamat siang")
                || trimmed.eq_ignore_ascii_case("selamat malam");
            if is_short_greeting {
                user_input.to_string()
            } else {
                format!("[MODE: PLAN - Diskusikan, beri masukan, atau rencanakan arsitektur tanpa mengubah file]:\n{}", user_input)
            }
        } else {
            user_input.to_string()
        };

        let allow_tools = mode_lower == "build";
        self.step_internal(session, &prompt_adjusted, allow_tools, mode).await
    }

    async fn step_internal(&self, session: &mut Session, user_input: &str, allow_tools: bool, mode: &str) -> Result<StepOutcome> {
        // If session is empty, inject system prompt
        if session.messages.is_empty() {
            let sys = self
                .config
                .system_prompt
                .clone()
                .unwrap_or_else(Self::default_system_prompt);
            session.messages.push(ChatMessage::system(sys));
        }

        // Process @filename mentions
        let processed_input = Self::process_inline_mentions(user_input);

        // Retrieve grounded document context from SQLite if available (NotebookLM integration)
        let final_prompt = if let Some(db_lock) = &self.db {
            if let Ok(db) = db_lock.lock() {
                if let Ok(context_str) = DocumentEngine::retrieve_context(&db, &session.id, user_input, 4) {
                    if !context_str.trim().is_empty() {
                        format!("{}\n\n{}", context_str, processed_input)
                    } else {
                        processed_input
                    }
                } else {
                    processed_input
                }
            } else {
                processed_input
            }
        } else {
            processed_input
        };

        // Push user message with active mode
        let mut user_msg = ChatMessage::user(final_prompt);
        user_msg.mode = Some(mode.to_string());
        session.messages.push(user_msg);
        SessionStore::save(session)?;

        let mut iterations = 0;
        let max_iter = self.config.max_iterations;
        let mut final_response = String::new();
        let mut last_model_used = self.provider.model().to_string();
        let mut last_failover: Option<FailoverInfo> = None;

        if !self.quiet {
            println!(
                "{}",
                format!(
                    "⚡ [Lontar] Menghubungi provider: {} ({})",
                    self.provider.name().cyan(),
                    self.provider.model().yellow()
                )
                .dimmed()
            );
        }

        while iterations < max_iter {
            iterations += 1;

            let empty_defs = Vec::new();
            let tool_defs = if allow_tools {
                self.tools.definitions()
            } else {
                empty_defs
            };
            let response = match self.provider.chat(&session.messages, &tool_defs).await {
                Ok(resp) => resp,
                Err(err) => {
                    if !self.quiet {
                        eprintln!("{}", format!("❌ Provider Error: {:#}", err).red().bold());
                    }
                    return Err(err);
                }
            };

            if response.failover.is_some() {
                last_failover = response.failover.clone();
            }
            last_model_used = response.model_name.clone();

            // Process text content if present
            if let Some(content) = &response.content {
                if !content.trim().is_empty() {
                    if !self.quiet {
                        println!("\n{}", content);
                    }
                    final_response = content.clone();
                }
            }

            // Save assistant message to session history with mode
            let mut assistant_msg = ChatMessage::assistant(
                response.content.clone(),
                response.tool_calls.clone(),
                Some(response.model_name.clone()),
            );
            assistant_msg.mode = Some(mode.to_string());
            session.messages.push(assistant_msg);
            SessionStore::save(session)?;

            // If no tool calls, conversation turn is done
            let tool_calls = match response.tool_calls {
                Some(calls) if !calls.is_empty() => calls,
                _ => break,
            };

            // Execute requested tools
            for call in tool_calls {
                if !self.quiet {
                    println!(
                        "\n{} {}",
                        "🔧 Memanggil Perkakas:".bold().magenta(),
                        call.name.bold().yellow()
                    );
                }

                let is_destructive = self.tools.is_destructive(&call.name);
                let parsed_args: serde_json::Value =
                    serde_json::from_str(&call.arguments).unwrap_or_else(|_| {
                        serde_json::json!({ "raw": call.arguments })
                    });

                // Human-In-The-Loop confirmation check with Diff Preview for file modifications
                let mut allow = true;
                if is_destructive && !self.auto_confirm {
                    if call.name == "write_file" {
                        if let (Some(path_str), Some(new_content)) = (
                            parsed_args.get("path").and_then(|v| v.as_str()),
                            parsed_args.get("content").and_then(|v| v.as_str()),
                        ) {
                            let path = Path::new(path_str);
                            let old_content = fs::read_to_string(path).unwrap_or_default();
                            allow = DiffViewer::prompt_diff_confirmation(path, &old_content, new_content, false);
                        } else {
                            allow = Self::prompt_simple_confirmation(&call.name)?;
                        }
                    } else if call.name == "edit_file" {
                        if let (Some(path_str), Some(target), Some(replacement)) = (
                            parsed_args.get("path").and_then(|v| v.as_str()),
                            parsed_args.get("target").and_then(|v| v.as_str()),
                            parsed_args.get("replacement").and_then(|v| v.as_str()),
                        ) {
                            let path = Path::new(path_str);
                            let old_content = fs::read_to_string(path).unwrap_or_default();
                            let new_content = old_content.replace(target, replacement);
                            allow = DiffViewer::prompt_diff_confirmation(path, &old_content, &new_content, false);
                        } else {
                            allow = Self::prompt_simple_confirmation(&call.name)?;
                        }
                    } else {
                        allow = Self::prompt_simple_confirmation(&call.name)?;
                    }
                }

                let tool_output = if allow {
                    match self.tools.execute(&call.name, &parsed_args).await {
                        Ok(res) => {
                            let preview = if res.len() > 300 {
                                format!("{} ... ({} byte)", &res[..300], res.len())
                            } else {
                                res.clone()
                            };
                            if !self.quiet {
                                println!(
                                    "{}",
                                    format!("   Hasil:\n{}", preview).dimmed()
                                );
                            }
                            res
                        }
                        Err(err) => {
                            let err_msg = format!("Error eksekusi {}: {:#}", call.name, err);
                            if !self.quiet {
                                eprintln!("{}", format!("   {}", err_msg).red());
                            }
                            err_msg
                        }
                    }
                } else {
                    let reject_msg = "Eksekusi perkakas dibatalkan oleh pengguna (HITL safety confirmation ditolak).".to_string();
                    if !self.quiet {
                        println!("{}", format!("   [Dibatalkan pengguna]").yellow());
                    }
                    reject_msg
                };

                // Record tool response
                session
                    .messages
                    .push(ChatMessage::tool_response(call.id, tool_output));
                SessionStore::save(session)?;
            }
        }

        if iterations >= max_iter {
            if !self.quiet {
                println!(
                    "{}",
                    format!("⚠️ Batas maksimum iterasi ({}) tercapai.", max_iter).yellow()
                );
            }
        }

        Ok(StepOutcome {
            reply: final_response,
            model_used: last_model_used,
            failover: last_failover,
        })
    }

    fn prompt_simple_confirmation(tool_name: &str) -> Result<bool> {
        print!(
            "{}",
            format!(
                "⚠️  [HITL Safety] Izinkan eksekusi perkakas '{}'? [y/N]: ",
                tool_name
            )
            .bold()
            .yellow()
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        Ok(trimmed == "y" || trimmed == "yes")
    }
}
