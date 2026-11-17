use anyhow::Result;
use colored::*;
use std::io::{self, Write};
use std::sync::Arc;

use crate::config::Config;
use crate::provider::LlmProvider;
use crate::session::{ChatMessage, Session, SessionStore};
use crate::tools::ToolRegistry;

pub struct Agent {
    provider: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    config: Config,
    auto_confirm: bool,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        config: Config,
        auto_confirm_override: bool,
    ) -> Self {
        let auto_confirm = auto_confirm_override || config.auto_confirm;
        Self {
            provider,
            tools,
            config,
            auto_confirm,
        }
    }

    pub fn default_system_prompt() -> String {
        r#"Anda adalah CRootAgent, asisten pengembang cerdas berbasis CLI ultra-ringan yang tangguh, praktis, dan berorientasi hasil.
Aturan kerja Anda:
1. Jawab langsung ke inti teknis tanpa basa-basi atau kalimat klise pembuka.
2. Gunakan perkakas (tools) yang tersedia untuk membaca kode, membuat file, mengedit file, dan menjalankan perintah terminal.
3. Lakukan perubahan secara bedah (surgical), jangan merusak kode yang sudah bekerja.
4. Jangan pernah mengarang isi file atau hasil eksekusi terminal. Selalu gunakan perkakas yang disediakan untuk memverifikasi.
5. Bila tugas membutuhkan beberapa langkah, lakukan langkah demi langkah secara teratur."#
            .to_string()
    }

    pub async fn step(&self, session: &mut Session, user_input: &str) -> Result<String> {
        // If session is empty, inject system prompt
        if session.messages.is_empty() {
            let sys = self
                .config
                .system_prompt
                .clone()
                .unwrap_or_else(Self::default_system_prompt);
            session.messages.push(ChatMessage::system(sys));
        }

        // Push user message
        session.messages.push(ChatMessage::user(user_input));
        SessionStore::save(session)?;

        let mut iterations = 0;
        let max_iter = self.config.max_iterations;
        let mut final_response = String::new();

        println!(
            "{}",
            format!(
                "⚡ [CRootAgent] Menghubungi provider: {} ({})",
                self.provider.name().cyan(),
                self.provider.model().yellow()
            )
            .dimmed()
        );

        while iterations < max_iter {
            iterations += 1;

            let tool_defs = self.tools.definitions();
            let response = match self.provider.chat(&session.messages, &tool_defs).await {
                Ok(resp) => resp,
                Err(err) => {
                    eprintln!("{}", format!("❌ Provider Error: {:#}", err).red().bold());
                    return Err(err);
                }
            };

            // Process text content if present
            if let Some(content) = &response.content {
                if !content.trim().is_empty() {
                    println!("\n{}", content);
                    final_response = content.clone();
                }
            }

            // Save assistant message to session history
            let assistant_msg = ChatMessage::assistant(
                response.content.clone(),
                response.tool_calls.clone(),
                Some(response.model_name.clone()),
            );
            session.messages.push(assistant_msg);
            SessionStore::save(session)?;

            // If no tool calls, conversation turn is done
            let tool_calls = match response.tool_calls {
                Some(calls) if !calls.is_empty() => calls,
                _ => break,
            };

            // Execute requested tools
            for call in tool_calls {
                println!(
                    "\n{} {}",
                    "🔧 Memanggil Perkakas:".bold().magenta(),
                    call.name.bold().yellow()
                );
                println!(
                    "{}",
                    format!("   Argumen: {}", call.arguments).dimmed()
                );

                let is_destructive = self.tools.is_destructive(&call.name);

                // Human-In-The-Loop confirmation check
                let mut allow = true;
                if is_destructive && !self.auto_confirm {
                    print!(
                        "{}",
                        format!(
                            "⚠️  [HITL Safety] Izinkan eksekusi perkakas '{}'? [y/N]: ",
                            call.name
                        )
                        .bold()
                        .yellow()
                    );
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    let trimmed = input.trim().to_lowercase();
                    if trimmed != "y" && trimmed != "yes" {
                        allow = false;
                    }
                }

                let tool_output = if allow {
                    let parsed_args: serde_json::Value =
                        serde_json::from_str(&call.arguments).unwrap_or_else(|_| {
                            serde_json::json!({ "raw": call.arguments })
                        });

                    match self.tools.execute(&call.name, &parsed_args).await {
                        Ok(res) => {
                            let preview = if res.len() > 300 {
                                format!("{} ... ({} byte)", &res[..300], res.len())
                            } else {
                                res.clone()
                            };
                            println!(
                                "{}",
                                format!("   Hasil:\n{}", preview).dimmed()
                            );
                            res
                        }
                        Err(err) => {
                            let err_msg = format!("Error eksekusi {}: {:#}", call.name, err);
                            eprintln!("{}", format!("   {}", err_msg).red());
                            err_msg
                        }
                    }
                } else {
                    let reject_msg = "Eksekusi perkakas dibatalkan oleh pengguna (HITL safety confirmation ditolak).".to_string();
                    println!("{}", format!("   [Dibatalkan pengguna]").yellow());
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
            println!(
                "{}",
                format!("⚠️ Batas maksimum iterasi ({}) tercapai.", max_iter).yellow()
            );
        }

        Ok(final_response)
    }
}
