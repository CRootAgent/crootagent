use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        KeyboardEnhancementFlags, MouseEventKind, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use crate::agent::Agent;
use crate::config::Config;
use crate::db::{Database, DbSession};
use crate::document::DocumentEngine;
use crate::provider::FailoverInfo;
use crate::router::LlmRouter;
use crate::session::{Session, SessionHeader, SessionStore};
use crate::tools::ToolRegistry;

// ── OpenCode Minimalist Color Palette ───────────────────────────────────────
const BG_CARD: Color = Color::Rgb(24, 24, 27);         // zinc-900 surface
const BG_MODAL: Color = Color::Rgb(18, 18, 20);        // zinc-950 modal surface
const TEXT_PRIMARY: Color = Color::Rgb(244, 244, 245);  // zinc-100
const TEXT_MUTED: Color = Color::Rgb(161, 161, 170);    // zinc-400
const TEXT_DIM: Color = Color::Rgb(113, 113, 122);      // zinc-500
const TEXT_DARK: Color = Color::Rgb(82, 82, 91);        // zinc-600
const COLOR_BUILD: Color = Color::Rgb(56, 189, 248);   // Biru (sky-400)
const COLOR_PLAN: Color = Color::Rgb(254, 215, 170);    // Krem / Beige (orange-200)
const COLOR_STUDY: Color = Color::Rgb(74, 222, 128);   // Hijau (emerald-400)
const ACCENT_ORANGE: Color = Color::Rgb(251, 146, 60);  // orange-400 (Tip dot)
const ACCENT_PURPLE: Color = Color::Rgb(167, 139, 250); // violet-400 (Category labels)
const HIGHLIGHT_PEACH: Color = Color::Rgb(254, 178, 120); // peach selected highlight
const BORDER_SUBTLE: Color = Color::Rgb(39, 39, 42);    // zinc-800
const BORDER_MODAL: Color = Color::Rgb(63, 63, 70);     // zinc-700
const ERROR_RED: Color = Color::Rgb(248, 113, 113);     // red-400

const PLACEHOLDERS: &[&str] = &[
    "Fix a TODO in the codebase",
    "Analisis dokumen grounded dan buat ringkasan",
    "Refactor fungsi agar lebih efisien dan modular",
    "Buat unit test untuk modul database",
    "Synthesize PRD dari ide fitur baru",
    "Jelaskan arsitektur LLM router di lontar",
];

const TIPS: &[&str] = &[
    "Multi-line input: Shift+Enter, Alt+Enter, Ctrl+J, or end line with \\",
    "Press Tab to switch between Build (Biru), Plan (Krem), and Study (Hijau)",
    "Press Ctrl+X then m (or Ctrl+P) to select AI model",
    "Press Ctrl+X then l (or Ctrl+P) to switch saved sessions",
    "Press Ctrl+N to start a fresh new session",
    "Press Ctrl+T to inspect grounded sources and tasks",
    "Use \"@file\" to ground codebase or documents into prompt",
];

pub fn mode_color_by_name(mode: &str) -> Color {
    match mode.to_lowercase().as_str() {
        "build" => COLOR_BUILD,
        "plan" | "chat" => COLOR_PLAN,
        "study" => COLOR_STUDY,
        _ => COLOR_BUILD,
    }
}

pub fn mode_name_normalized(mode: &str) -> &'static str {
    match mode.to_lowercase().as_str() {
        "plan" | "chat" => "Plan",
        "study" => "Study",
        _ => "Build",
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\x1b[?1007l");
        let _ = stdout.flush();
        let _ = execute!(
            stdout,
            PopKeyboardEnhancementFlags,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum AppMode {
    Build,
    Plan,
    Study,
}

impl AppMode {
    pub fn name(&self) -> &'static str {
        match self {
            AppMode::Build => "Build",
            AppMode::Plan => "Plan",
            AppMode::Study => "Study",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            AppMode::Build => COLOR_BUILD,
            AppMode::Plan => COLOR_PLAN,
            AppMode::Study => COLOR_STUDY,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            AppMode::Build => AppMode::Plan,
            AppMode::Plan => AppMode::Study,
            AppMode::Study => AppMode::Build,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatMessageItem {
    pub sender: String,
    pub text: String,
    pub mode_name: String,
    pub mode_color: Color,
    pub model_info: String,
}

#[derive(Clone, Debug)]
pub struct ModelChoice {
    pub group: &'static str,
    pub name: &'static str,
    pub provider_id: &'static str,
    pub model_id: &'static str,
    pub tag: &'static str,
}

#[derive(Clone, Debug)]
pub struct ModelChoiceItem {
    pub group: String,
    pub name: String,
    pub provider_id: String,
    pub model_id: String,
    pub tag: String,
}

pub const MODEL_CHOICES: &[ModelChoice] = &[
    // Lontar Free AI (No API Key Required - Out of the Box)
    ModelChoice { group: "Lontar Free AI", name: "Lontar Fast AI (No Key Needed)", provider_id: "lontar-free", model_id: "openai-fast", tag: "Free" },
    ModelChoice { group: "Lontar Free AI", name: "Lontar Reasoning AI (No Key Needed)", provider_id: "lontar-free", model_id: "deepseek", tag: "Free" },

    // Recent / Popular Free Models
    ModelChoice { group: "Recent", name: "DeepSeek V4.1 Flash (Latest V4)", provider_id: "deepseek", model_id: "deepseek-flash", tag: "Free" },
    ModelChoice { group: "Recent", name: "Gemini 3.5 Flash-Lite", provider_id: "gemini", model_id: "gemini-3.5-flash-lite", tag: "Free" },
    ModelChoice { group: "Recent", name: "Z.AI GLM-4.5-Flash", provider_id: "z.ai", model_id: "glm-4.5-flash", tag: "Free" },
    ModelChoice { group: "Recent", name: "Llama 3.3 70B Versatile", provider_id: "groq", model_id: "llama-3.3-70b-versatile", tag: "Free" },
    ModelChoice { group: "Recent", name: "NVIDIA Nemotron 3.5 Lightning", provider_id: "openrouter", model_id: "nvidia/nemotron-3.5-lightning:free", tag: "Free" },

    // Google Gemini (Accurate 2026 Active Free Tier on Google AI Studio)
    ModelChoice { group: "Google Gemini", name: "Gemini 3.5 Flash-Lite (Active Free Tier)", provider_id: "gemini", model_id: "gemini-3.5-flash-lite", tag: "Free" },
    ModelChoice { group: "Google Gemini", name: "Gemini 3.5 Flash (Next-Gen Multimodal)", provider_id: "gemini", model_id: "gemini-3.5-flash", tag: "Free" },
    ModelChoice { group: "Google Gemini", name: "Gemini 3.8 Flash (Advanced Reasoning)", provider_id: "gemini", model_id: "gemini-3.8-flash", tag: "Free" },
    ModelChoice { group: "Google Gemini", name: "Gemini Flash Latest (Dynamic Active)", provider_id: "gemini", model_id: "gemini-flash-latest", tag: "Free" },

    // Z.AI / Zhipu (Free GLM Models)
    ModelChoice { group: "Z.AI (Zhipu)", name: "GLM-4.5-Flash (Free Zero-Cost)", provider_id: "z.ai", model_id: "glm-4.5-flash", tag: "Free" },
    ModelChoice { group: "Z.AI (Zhipu)", name: "GLM-4.7-Flash (Free Fast Reasoning)", provider_id: "z.ai", model_id: "glm-4.7-flash", tag: "Free" },

    // Groq Cloud (Ultra-fast LPU Inference)
    ModelChoice { group: "Groq Cloud", name: "Llama 3.3 70B Versatile (300+ tok/s)", provider_id: "groq", model_id: "llama-3.3-70b-versatile", tag: "Free" },
    ModelChoice { group: "Groq Cloud", name: "Llama 3.1 8B Instant (Ultra-fast)", provider_id: "groq", model_id: "llama-3.1-8b-instant", tag: "Free" },

    // DeepSeek (Official V4, R1 & V3)
    ModelChoice { group: "DeepSeek", name: "DeepSeek V4.1 Flash (Official V4 Default)", provider_id: "deepseek", model_id: "deepseek-flash", tag: "Free" },
    ModelChoice { group: "DeepSeek", name: "DeepSeek V4 Pro (Advanced Agent V4)", provider_id: "deepseek", model_id: "deepseek-v4-pro", tag: "Paid" },
    ModelChoice { group: "DeepSeek", name: "DeepSeek R1 (Reasoner / Deep Thinking)", provider_id: "deepseek", model_id: "deepseek-reasoner", tag: "Free" },
    ModelChoice { group: "DeepSeek", name: "DeepSeek V3 (Legacy Chat)", provider_id: "deepseek", model_id: "deepseek-chat", tag: "Free" },

    // OpenRouter Free Hub (Awesome Free LLM APIs)
    ModelChoice { group: "OpenRouter Free", name: "Nemotron 3.5 Lightning (:free - 1M Ctx)", provider_id: "openrouter", model_id: "nvidia/nemotron-3.5-lightning:free", tag: "Free" },
    ModelChoice { group: "OpenRouter Free", name: "Llama 3.3 70B Instruct (:free)", provider_id: "openrouter", model_id: "meta-llama/llama-3.3-70b-instruct:free", tag: "Free" },
    ModelChoice { group: "OpenRouter Free", name: "DeepSeek R1 Reasoner (:free)", provider_id: "openrouter", model_id: "deepseek/deepseek-r1:free", tag: "Free" },
    ModelChoice { group: "OpenRouter Free", name: "Google Gemma 4 26B (:free)", provider_id: "openrouter", model_id: "google/gemma-4-26b-a4b-it:free", tag: "Free" },

    // Hardware Accelerators & Others
    ModelChoice { group: "SambaNova", name: "Llama 3.3 70B (Fast RDU)", provider_id: "sambanova", model_id: "Meta-Llama-3.3-70B-Instruct", tag: "Free" },
    ModelChoice { group: "GitHub Models", name: "GPT-4o Mini (Free with PAT)", provider_id: "github", model_id: "gpt-4o-mini", tag: "Free" },
    ModelChoice { group: "Mistral AI", name: "Mistral Small Latest", provider_id: "mistral", model_id: "mistral-small-latest", tag: "Free" },
    ModelChoice { group: "Ollama (Offline)", name: "Llama 3.2 (Local Machine)", provider_id: "ollama", model_id: "llama3.2", tag: "Free" },

    // Commercial
    ModelChoice { group: "Commercial", name: "GPT-4o Mini", provider_id: "openai", model_id: "gpt-4o-mini", tag: "Paid" },
    ModelChoice { group: "Commercial", name: "Claude 3.5 Sonnet", provider_id: "anthropic", model_id: "claude-3-5-sonnet-20241022", tag: "Paid" },
];

pub struct PaletteItem {
    pub group: &'static str,
    pub name: &'static str,
    pub shortcut: &'static str,
    pub action: &'static str,
}

pub const PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem { group: "Suggested", name: "New session", shortcut: "ctrl+n", action: "new_session" },
    PaletteItem { group: "Suggested", name: "Switch model", shortcut: "ctrl+x m", action: "model_ui" },
    PaletteItem { group: "Suggested", name: "Switch session", shortcut: "ctrl+x l", action: "session_ui" },
    PaletteItem { group: "Suggested", name: "Switch mode (Build/Plan/Study)", shortcut: "tab", action: "mode" },
    PaletteItem { group: "AI Providers", name: "Tutorial: Cara dapat API Key Gratis", shortcut: "/guide", action: "guide" },
    PaletteItem { group: "AI Providers", name: "Tambah Custom Provider API", shortcut: "/custom", action: "custom_provider" },
    PaletteItem { group: "AI Providers", name: "Auto-detect Model dari API", shortcut: "/models fetch", action: "fetch_models" },
    PaletteItem { group: "NotebookLM", name: "Ingest document", shortcut: "/context add", action: "context_add" },
    PaletteItem { group: "NotebookLM", name: "Inspect sources & tasks", shortcut: "ctrl+t", action: "sources" },
    PaletteItem { group: "NotebookLM", name: "Generate Study Guide", shortcut: "/study-guide", action: "study_guide" },
    PaletteItem { group: "NotebookLM", name: "Generate PRD", shortcut: "/prd", action: "prd" },
    PaletteItem { group: "NotebookLM", name: "Generate Tasklist", shortcut: "/tasklist", action: "tasklist" },
    PaletteItem { group: "System", name: "Git diff status", shortcut: "/diff", action: "diff" },
    PaletteItem { group: "System", name: "Clear conversation", shortcut: "ctrl+l", action: "clear" },
    PaletteItem { group: "System", name: "Exit Lontar", shortcut: "ctrl+c", action: "exit" },
];

#[derive(Clone, Debug)]
pub struct SlashCommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub example: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommandDef] = &[
    SlashCommandDef {
        name: "/context add",
        description: "Ingest file dokumen (PDF/MD/code) ke context NotebookLM",
        example: "/context add <path_file> [pertanyaan]",
    },
    SlashCommandDef {
        name: "/context list",
        description: "Lihat daftar dokumen yang aktif di sesi ini",
        example: "/context list",
    },
    SlashCommandDef {
        name: "/model",
        description: "Ganti model atau provider aktif (e.g. /model deepseek-flash)",
        example: "/model <nama_model/provider>",
    },
    SlashCommandDef {
        name: "/models fetch",
        description: "Sinkronkan daftar model aktif (tambah baru & hapus usang) dari API",
        example: "/models fetch [provider]",
    },
    SlashCommandDef {
        name: "/custom",
        description: "Daftarkan API key provider baru (DeepSeek, Groq, Z.AI, dll)",
        example: "/custom <preset> <api_key>",
    },
    SlashCommandDef {
        name: "/guide",
        description: "Tutorial dan panduan dapat API key gratis",
        example: "/guide [provider]",
    },
    SlashCommandDef {
        name: "/study-guide",
        description: "Generate Study Guide terstruktur dari dokumen grounded",
        example: "/study-guide [topik]",
    },
    SlashCommandDef {
        name: "/prd",
        description: "Generate Product Requirement Document (PRD.md)",
        example: "/prd [fitur]",
    },
    SlashCommandDef {
        name: "/tasklist",
        description: "Generate checklist roadmap tugas (TASKLIST.md)",
        example: "/tasklist [roadmap]",
    },
    SlashCommandDef {
        name: "/diff",
        description: "Tampilkan Git diff visual perubahan kode",
        example: "/diff",
    },
    SlashCommandDef {
        name: "/session",
        description: "Kelola & beralih sesi percakapan",
        example: "/session",
    },
    SlashCommandDef {
        name: "/config set",
        description: "Ubah pengaturan konfigurasi Lontar",
        example: "/config set <key> <val>",
    },
    SlashCommandDef {
        name: "/clear",
        description: "Bersihkan layar percakapan saat ini",
        example: "/clear",
    },
    SlashCommandDef {
        name: "/help",
        description: "Bantuan dan daftar shortcut perintah",
        example: "/help",
    },
    SlashCommandDef {
        name: "/exit",
        description: "Keluar dari Lontar CLI",
        example: "/exit",
    },
];

pub struct TuiApp {
    pub session: Session,
    pub input: String,
    pub cursor_position: usize,
    pub slash_selected_index: usize,
    pub messages: Vec<ChatMessageItem>,
    pub grounded_docs: Vec<(String, String, i64)>, // (path, type, chunks)
    pub tasklist: Vec<(bool, String)>,
    pub active_provider: String,
    pub active_model: String,
    pub mode: AppMode,
    pub scroll_offset_from_bottom: usize,
    pub max_scroll: usize,
    pub should_quit: bool,
    pub agent: Agent,
    // Modals
    pub show_commands_modal: bool,
    pub command_search: String,
    pub selected_command_index: usize,
    pub palette_scroll_offset: usize,

    pub show_models_modal: bool,
    pub model_search: String,
    pub selected_model_index: usize,
    pub model_scroll_offset: usize,

    pub show_sessions_modal: bool,
    pub session_search: String,
    pub selected_session_index: usize,
    pub session_scroll_offset: usize,
    pub sessions_list: Vec<SessionHeader>,

    pub show_sources_modal: bool,
    pub pending_ctrl_x: bool,

    // Connect Provider Modal (OpenCode DialogIntegration Parity)
    pub show_connect_modal: bool,
    pub connect_provider_id: String,
    pub connect_model_id: String,
    pub connect_api_key_input: String,
    pub connect_cursor_position: usize,

    // Async Loading & Interrupt (OpenCode Parity)
    pub is_loading: bool,
    pub interrupt_armed: bool,
    pub armed_time: Option<Instant>,
    pub spinner_frame: usize,

    // Error & Timing
    pub api_error: Option<String>,
    pub start_time: Instant,
}

impl TuiApp {
    pub fn new(session: Session, config: Config, cli_yes: bool) -> Result<Self> {
        let mut provider_name = Config::canonical_provider_name(&session.provider);
        let mut model_name = session.model.clone();

        // If active provider isn't configured (missing key for non-local/non-custom), fallback to lontar-free
        if !config.has_configured_provider(&provider_name) {
            provider_name = "lontar-free".to_string();
            model_name = config.lontar_free_model.clone();
        }

        let mut router = LlmRouter::new(&provider_name, Some(&model_name), &config)?;
        router.quiet = true;
        let tools = ToolRegistry::new();
        let agent = Agent::with_options(Arc::new(router), tools, config.clone(), cli_yes, true);

        // Load existing messages - strictly ignore internal system prompts
        let mut messages = Vec::new();
        for msg in &session.messages {
            if msg.role == "system" {
                continue;
            }
            let sender = match msg.role.as_str() {
                "user" => "User",
                "assistant" => "Lontar",
                "tool" => "Tool",
                _ => continue,
            };
            if let Some(content) = &msg.content {
                if content.starts_with("=== GROUNDED CONTEXT") && sender == "User" {
                    continue;
                }
                let mode_str = msg.mode.as_deref().unwrap_or("build");
                let mode_color = mode_color_by_name(mode_str);
                let mode_name = mode_name_normalized(mode_str);
                let model_info = msg.model_attribution.clone().unwrap_or_else(|| {
                    format!("{} {}", session.model, session.provider)
                });
                messages.push(ChatMessageItem {
                    sender: sender.to_string(),
                    text: content.clone(),
                    mode_name: mode_name.to_string(),
                    mode_color,
                    model_info,
                });
            }
        }

        // Load documents from SQLite
        let mut grounded_docs = Vec::new();
        if let Ok(db) = Database::open_default() {
            if let Ok(docs) = db.list_documents(&session.id) {
                for d in docs {
                    grounded_docs.push((d.file_path, d.file_type, d.chunk_count));
                }
            }
        }

        // Actionable tasklist items (Kiro style)
        let tasklist = vec![
            (true, "Inisialisasi Arsitektur Lontar CLI Rust".to_string()),
            (true, "Setup SQLite CRUD Database (Sessions, Docs, Logs)".to_string()),
            (true, "Konfigurasi Universal Free LLM Router & 429 Failover".to_string()),
            (true, "Implementasi Grounding Dokumen NotebookLM".to_string()),
            (true, "Perpaduan TUI Interaktif OpenCode Stream & Kiro Tasks".to_string()),
            (false, "Eksplorasi Tugas & Coding Baru Pengguna".to_string()),
        ];

        let has_key = agent.config.has_configured_provider(&provider_name);
        let api_error = if has_key {
            None
        } else {
            Some(format!(
                "✖ API key '{}' belum diset. Tekan Enter untuk hubungkan, atau pilih model lain.",
                provider_name
            ))
        };

        let sessions_list = SessionStore::list().unwrap_or_default();

        Ok(Self {
            session,
            input: String::new(),
            cursor_position: 0,
            slash_selected_index: 0,
            messages,
            grounded_docs,
            tasklist,
            active_provider: provider_name,
            active_model: model_name,
            mode: AppMode::Build,
            scroll_offset_from_bottom: 0,
            max_scroll: 0,
            should_quit: false,
            agent,
            show_commands_modal: false,
            command_search: String::new(),
            selected_command_index: 0,
            palette_scroll_offset: 0,
            show_models_modal: false,
            model_search: String::new(),
            selected_model_index: 0,
            model_scroll_offset: 0,
            show_sessions_modal: false,
            session_search: String::new(),
            selected_session_index: 0,
            session_scroll_offset: 0,
            sessions_list,
            show_sources_modal: false,
            pending_ctrl_x: false,
            show_connect_modal: false,
            connect_provider_id: String::new(),
            connect_model_id: String::new(),
            connect_api_key_input: String::new(),
            connect_cursor_position: 0,
            is_loading: false,
            interrupt_armed: false,
            armed_time: None,
            spinner_frame: 0,
            api_error,
            start_time: Instant::now(),
        })
    }

    /// Persist user-chosen provider and model permanently to `config.toml` and active session.
    pub fn persist_active_provider(&mut self, provider: &str, model: Option<&str>) {
        let canonical = Config::canonical_provider_name(provider);
        let resolved_model = model
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .unwrap_or_else(|| self.agent.config.get_model(&canonical));

        let _ = self
            .agent
            .config
            .activate_provider_and_save(&canonical, Some(&resolved_model));
        self.active_provider = canonical.clone();
        self.active_model = resolved_model.clone();
        self.agent.update_provider(&canonical, Some(&resolved_model));
        self.session.provider = canonical;
        self.session.model = resolved_model;
        let _ = SessionStore::save(&self.session);
        self.api_error = None;
    }

    /// Reflect automatic model/provider failover in UI state, sync any newly discovered models,
    /// and notify the user cleanly.
    pub fn apply_failover(&mut self, failover: Option<FailoverInfo>) -> String {
        self.apply_failover_with_save(failover, true)
    }

    pub fn apply_failover_with_save(&mut self, failover: Option<FailoverInfo>, persist: bool) -> String {
        if let Some(info) = failover {
            self.active_provider = info.to_provider.clone();
            self.active_model = info.to_model.clone();
            self.agent.update_provider(&info.to_provider, Some(&info.to_model));
            self.session.provider = info.to_provider.clone();
            self.session.model = info.to_model.clone();
            if persist {
                let _ = SessionStore::save(&self.session);
            }

            let mut synced_count = 0usize;
            if let Some(ref discovered) = info.discovered_models {
                let canonical = Config::canonical_provider_name(&info.to_provider);
                let baseline_static: Vec<&str> = MODEL_CHOICES
                    .iter()
                    .filter(|m| Config::canonical_provider_name(m.provider_id) == canonical)
                    .map(|m| m.model_id)
                    .collect();

                if persist {
                    let _ = self.agent.config.sync_provider_models(&canonical, discovered, &baseline_static);
                    let _ = self.agent.config.activate_provider_and_save(&canonical, Some(&info.to_model));
                } else {
                    self.agent.config.sync_provider_models_in_memory(&canonical, discovered, &baseline_static);
                    self.agent.config.activate_provider_in_memory(&canonical, Some(&info.to_model));
                }
                synced_count = discovered.len();
            } else if info.from_provider.eq_ignore_ascii_case(&info.to_provider)
                && info.reason.contains("tidak tersedia")
            {
                if persist {
                    let _ = self.agent.config.activate_provider_and_save(&info.to_provider, Some(&info.to_model));
                } else {
                    self.agent.config.activate_provider_in_memory(&info.to_provider, Some(&info.to_model));
                }
            }

            let sync_note = if synced_count > 0 {
                format!(" Daftar {} model di menu Select Model otomatis diperbarui.", synced_count)
            } else {
                String::new()
            };
            self.messages.push(ChatMessageItem {
                sender: "System".to_string(),
                text: format!(
                    "↻ Auto-switch: '{}' ({}) {}, otomatis beralih ke '{}' ({}).{}",
                    info.from_model, info.from_provider, info.reason, info.to_model, info.to_provider, sync_note
                ),
                mode_name: self.mode.name().to_string(),
                mode_color: self.mode.color(),
                model_info: format!("{} {}", info.to_model, info.to_provider),
            });
        }
        format!("{} {}", self.active_model, self.active_provider)
    }

    /// Synchronize fetched models for `provider`: add live chat models, remove undetected static ones,
    /// and update the active model if the previous default is no longer available on the API.
    pub fn apply_fetched_models_with_save(
        &mut self,
        provider: &str,
        raw_models: &[String],
        persist: bool,
    ) -> String {
        if raw_models.is_empty() {
            return format!("Tidak ditemukan model aktif dari endpoint provider '{}'.", provider);
        }

        let canonical = Config::canonical_provider_name(provider);
        let prev_active_model = self.agent.config.get_model(&canonical);

        let filtered_chat: Vec<String> = raw_models
            .iter()
            .filter(|m| crate::provider::is_likely_chat_model(m))
            .cloned()
            .collect();
        let skipped_non_chat = raw_models.len().saturating_sub(filtered_chat.len());
        let chat_models = if filtered_chat.is_empty() {
            raw_models.to_vec()
        } else {
            filtered_chat
        };

        let baseline_static: Vec<&str> = MODEL_CHOICES
            .iter()
            .filter(|m| Config::canonical_provider_name(m.provider_id) == canonical)
            .map(|m| m.model_id)
            .collect();

        let outcome = if persist {
            self.agent
                .config
                .sync_provider_models(&canonical, &chat_models, &baseline_static)
                .unwrap_or_else(|_| {
                    self.agent
                        .config
                        .sync_provider_models_in_memory(&canonical, &chat_models, &baseline_static)
                })
        } else {
            self.agent
                .config
                .sync_provider_models_in_memory(&canonical, &chat_models, &baseline_static)
        };

        if let Some(ref new_default) = outcome.switched_default_to {
            if Config::canonical_provider_name(&self.active_provider) == canonical {
                self.active_model = new_default.clone();
                self.session.model = new_default.clone();
                self.agent.update_provider(&canonical, Some(new_default));
                if persist {
                    let _ = SessionStore::save(&self.session);
                }
            }
        }

        let mut lines = Vec::new();
        let skip_note = if skipped_non_chat > 0 {
            format!(" ({} non-chat seperti TTS/Whisper/Guard dilewati)", skipped_non_chat)
        } else {
            String::new()
        };
        lines.push(format!(
            "✔ Sync selesai! {} model chat aktif dari '{}' disimpan{}:",
            outcome.total_live, canonical, skip_note
        ));

        for m in chat_models.iter().take(10) {
            lines.push(format!("• `{}`", m));
        }
        if chat_models.len() > 10 {
            lines.push(format!("...dan {} model lainnya.", chat_models.len() - 10));
        }

        if !outcome.added.is_empty() {
            lines.push(format!(
                "+ Ditambahkan ({}): {}",
                outcome.added.len(),
                outcome.added.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !outcome.removed.is_empty() {
            lines.push(format!(
                "- Dihapus karena tidak tersedia ({}): {}",
                outcome.removed.len(),
                outcome.removed.join(", ")
            ));
        }
        if let Some(new_default) = outcome.switched_default_to {
            lines.push(format!(
                "↻ Model default '{}' diperbarui otomatis: `{}` → `{}`",
                canonical, prev_active_model, new_default
            ));
        }
        lines.push("💡 Daftar model di Select Model (`ctrl+x m`) & auto-failover sudah otomatis terupdate.".to_string());
        lines.join("\n")
    }

    pub fn scroll_up(&mut self, delta: usize) {
        self.scroll_offset_from_bottom = (self.scroll_offset_from_bottom + delta).min(self.max_scroll);
    }

    pub fn scroll_down(&mut self, delta: usize) {
        self.scroll_offset_from_bottom = self.scroll_offset_from_bottom.saturating_sub(delta);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset_from_bottom = 0;
    }

    pub fn refresh_docs(&mut self) {
        if let Ok(db) = Database::open_default() {
            if let Ok(docs) = db.list_documents(&self.session.id) {
                self.grounded_docs.clear();
                for d in docs {
                    self.grounded_docs.push((d.file_path, d.file_type, d.chunk_count));
                }
            }
        }
    }

    pub fn refresh_sessions(&mut self) {
        self.sessions_list = SessionStore::list().unwrap_or_default();
    }

    pub fn start_new_session(&mut self) -> Result<()> {
        let new_sess = SessionStore::create(None, self.active_provider.clone(), self.active_model.clone())?;
        if let Ok(db) = Database::open_default() {
            let _ = db.insert_session(&DbSession {
                id: new_sess.id.clone(),
                name: new_sess.name.clone(),
                created_at: new_sess.created_at.to_rfc3339(),
                updated_at: new_sess.updated_at.to_rfc3339(),
                provider: new_sess.provider.clone(),
                model: new_sess.model.clone(),
                total_tokens: 0,
            });
        }
        self.session = new_sess;
        self.messages.clear();
        self.input.clear();
        self.cursor_position = 0;
        self.slash_selected_index = 0;
        self.scroll_to_bottom();
        self.api_error = None;
        self.show_commands_modal = false;
        self.show_models_modal = false;
        self.show_sessions_modal = false;
        self.show_sources_modal = false;
        self.show_connect_modal = false;
        self.pending_ctrl_x = false;
        self.is_loading = false;
        self.interrupt_armed = false;
        self.armed_time = None;
        self.refresh_sessions();
        Ok(())
    }

    pub fn load_selected_session(&mut self, session_id: &str) -> Result<()> {
        let loaded = SessionStore::load(session_id)?;
        self.session = loaded.clone();
        self.messages.clear();

        for msg in &loaded.messages {
            if msg.role == "system" {
                continue;
            }
            let sender = match msg.role.as_str() {
                "user" => "User",
                "assistant" => "Lontar",
                "tool" => "Tool",
                _ => continue,
            };
            if let Some(content) = &msg.content {
                if content.starts_with("=== GROUNDED CONTEXT") && sender == "User" {
                    continue;
                }
                let mode_str = msg.mode.as_deref().unwrap_or("build");
                let mode_color = mode_color_by_name(mode_str);
                let mode_name = mode_name_normalized(mode_str);
                let model_info = msg.model_attribution.clone().unwrap_or_else(|| {
                    format!("{} {}", loaded.model, loaded.provider)
                });
                self.messages.push(ChatMessageItem {
                    sender: sender.to_string(),
                    text: content.clone(),
                    mode_name: mode_name.to_string(),
                    mode_color,
                    model_info,
                });
            }
        }

        self.active_provider = loaded.provider.clone();
        self.active_model = loaded.model.clone();
        self.agent.update_provider(&loaded.provider, Some(&loaded.model));
        self.refresh_docs();
        self.input.clear();
        self.cursor_position = 0;
        self.scroll_to_bottom();
        self.api_error = None;
        self.show_sessions_modal = false;
        self.show_connect_modal = false;
        self.is_loading = false;
        self.interrupt_armed = false;
        self.armed_time = None;
        Ok(())
    }

    pub fn delete_selected_session(&mut self, session_id: &str) -> Result<()> {
        let _ = SessionStore::delete(session_id);
        if let Ok(db) = Database::open_default() {
            let _ = db.delete_session(session_id);
        }
        self.refresh_sessions();
        if self.session.id == session_id {
            self.start_new_session()?;
        }
        Ok(())
    }

    pub fn cwd_name() -> String {
        std::env::current_dir()
            .map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string_lossy().to_string()))
            .unwrap_or_else(|_| "workspace".to_string())
    }

    pub fn git_branch() -> Option<String> {
        if let Ok(head) = fs::read_to_string(".git/HEAD") {
            let trimmed = head.trim();
            if trimmed.starts_with("ref: refs/heads/") {
                return Some(trimmed.trim_start_matches("ref: refs/heads/").to_string());
            } else if !trimmed.is_empty() {
                return Some("detached".to_string());
            }
        }
        None
    }

    pub fn get_filtered_palette(&self) -> Vec<&'static PaletteItem> {
        let q = self.command_search.to_lowercase();
        if q.is_empty() {
            PALETTE_ITEMS.iter().collect()
        } else {
            PALETTE_ITEMS
                .iter()
                .filter(|item| {
                    item.name.to_lowercase().contains(&q)
                        || item.shortcut.to_lowercase().contains(&q)
                        || item.group.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn open_models_modal(&mut self) {
        self.show_models_modal = true;
        self.model_search.clear();
        let items = self.get_filtered_models();
        let active_idx = items
            .iter()
            .position(|item| item.tag == "● Active")
            .unwrap_or(0);
        self.selected_model_index = active_idx;
        self.model_scroll_offset = active_idx.saturating_sub(3);
    }

    pub fn get_filtered_models(&self) -> Vec<ModelChoiceItem> {
        let q = self.model_search.to_lowercase();
        let mut list = Vec::new();

        // 1. Live-synced providers (shown first so newly fetched models are immediately accessible)
        let mut synced_providers: Vec<String> = self.agent.config.synced_models.keys().cloned().collect();
        synced_providers.sort();
        for prov_key in &synced_providers {
            if let Some(live_models) = self.agent.config.synced_models.get(prov_key) {
                let group_label = format!("{} (Synced Live)", prov_key.to_uppercase());
                for mdl in live_models {
                    list.push(ModelChoiceItem {
                        group: group_label.clone(),
                        name: mdl.clone(),
                        provider_id: prov_key.clone(),
                        model_id: mdl.clone(),
                        tag: "Live".to_string(),
                    });
                }
            }
        }

        // 2. Builtin models (skip stale static entries for any provider that has been synced from live API)
        for m in MODEL_CHOICES {
            let canon = Config::canonical_provider_name(m.provider_id);
            if let Some(live_models) = self.agent.config.get_synced_models(&canon) {
                if m.group == "Recent" && live_models.iter().any(|lm| lm.eq_ignore_ascii_case(m.model_id)) {
                    list.push(ModelChoiceItem {
                        group: m.group.to_string(),
                        name: m.name.to_string(),
                        provider_id: m.provider_id.to_string(),
                        model_id: m.model_id.to_string(),
                        tag: m.tag.to_string(),
                    });
                }
                continue;
            }
            list.push(ModelChoiceItem {
                group: m.group.to_string(),
                name: m.name.to_string(),
                provider_id: m.provider_id.to_string(),
                model_id: m.model_id.to_string(),
                tag: m.tag.to_string(),
            });
        }

        // 3. Custom configured providers (skip built-in names and already-synced providers to avoid duplicates)
        let mut custom_keys: Vec<String> = self.agent.config.custom_providers.keys().cloned().collect();
        custom_keys.sort();
        for key in custom_keys {
            if Config::is_builtin_provider(&key) || self.agent.config.synced_models.contains_key(&key) {
                continue;
            }
            if let Some(custom) = self.agent.config.custom_providers.get(&key) {
                list.push(ModelChoiceItem {
                    group: "Custom Providers".to_string(),
                    name: format!("{} ({})", custom.name, custom.model),
                    provider_id: custom.name.clone(),
                    model_id: custom.model.clone(),
                    tag: "Custom".to_string(),
                });
            }
        }

        // Ensure active (provider, model) is always present and clearly marked "● Active"
        let active_canon = Config::canonical_provider_name(&self.active_provider);
        let mut found_active = false;
        for item in &mut list {
            let item_canon = Config::canonical_provider_name(&item.provider_id);
            if item_canon.eq_ignore_ascii_case(&active_canon)
                && item.model_id.eq_ignore_ascii_case(&self.active_model)
            {
                item.tag = "● Active".to_string();
                found_active = true;
            }
        }
        if !found_active && !self.active_model.trim().is_empty() {
            list.insert(
                0,
                ModelChoiceItem {
                    group: format!("{} (Active)", active_canon.to_uppercase()),
                    name: self.active_model.clone(),
                    provider_id: active_canon,
                    model_id: self.active_model.clone(),
                    tag: "● Active".to_string(),
                },
            );
        }

        if q.is_empty() {
            list
        } else {
            list.into_iter()
                .filter(|m| {
                    m.name.to_lowercase().contains(&q)
                        || m.model_id.to_lowercase().contains(&q)
                        || m.provider_id.to_lowercase().contains(&q)
                        || m.group.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn get_filtered_sessions(&self) -> Vec<&SessionHeader> {
        let q = self.session_search.to_lowercase();
        if q.is_empty() {
            self.sessions_list.iter().collect()
        } else {
            self.sessions_list
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&q)
                        || s.id.to_lowercase().contains(&q)
                        || s.provider.to_lowercase().contains(&q)
                        || s.model.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    pub fn current_placeholder(&self) -> &'static str {
        let idx = (self.start_time.elapsed().as_secs() / 4) as usize % PLACEHOLDERS.len();
        PLACEHOLDERS[idx]
    }

    pub fn current_tip(&self) -> &'static str {
        let idx = (self.start_time.elapsed().as_secs() / 5) as usize % TIPS.len();
        TIPS[idx]
    }

    pub fn get_matching_slash_commands(&self) -> Vec<&'static SlashCommandDef> {
        let input_trimmed = self.input.trim_start();
        if !input_trimmed.starts_with('/') {
            return Vec::new();
        }

        let query = input_trimmed.to_lowercase();
        let query_tokens: Vec<&str> = query.split_whitespace().collect();

        let matches: Vec<&'static SlashCommandDef> = SLASH_COMMANDS
            .iter()
            .filter(|cmd| {
                let cmd_lower = cmd.name.to_lowercase();
                let cmd_tokens: Vec<&str> = cmd_lower.split_whitespace().collect();

                // If user typed more tokens than the command itself, they are typing arguments
                if query_tokens.len() > cmd_tokens.len() {
                    return false;
                }

                if query == "/" {
                    true
                } else if cmd_lower.starts_with(&query) {
                    true
                } else if query_tokens.len() <= cmd_tokens.len() {
                    query_tokens.iter().enumerate().all(|(i, qtok)| {
                        if i == query_tokens.len() - 1 {
                            cmd_tokens[i].starts_with(qtok)
                        } else {
                            cmd_tokens[i] == *qtok
                        }
                    })
                } else {
                    false
                }
            })
            .collect();

        matches
    }
}

pub enum BgTaskOutput {
    Chat { reply: String, session: Session, failover: Option<FailoverInfo> },
    StudyGuide { reply: String, session: Session, out_path: &'static str, failover: Option<FailoverInfo> },
    Prd { reply: String, session: Session, out_path: &'static str, failover: Option<FailoverInfo> },
    Tasklist { reply: String, session: Session, out_path: &'static str, failover: Option<FailoverInfo> },
    FetchedModels { provider: String, models: Vec<String> },
    Error(String),
}

fn spawn_agent_task(
    tx: tokio::sync::mpsc::Sender<BgTaskOutput>,
    agent: Agent,
    mut sess: Session,
    prompt: String,
    mode: String,
    artifact_path: Option<&'static str>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        match agent.step_with_mode_detailed(&mut sess, &prompt, &mode).await {
            Ok(outcome) => {
                if let Some(path) = artifact_path {
                    let _ = fs::write(path, &outcome.reply);
                }
                let msg = match artifact_path {
                    Some("STUDY_GUIDE.md") => BgTaskOutput::StudyGuide {
                        reply: outcome.reply,
                        session: sess,
                        out_path: "STUDY_GUIDE.md",
                        failover: outcome.failover,
                    },
                    Some("PRD.md") => BgTaskOutput::Prd {
                        reply: outcome.reply,
                        session: sess,
                        out_path: "PRD.md",
                        failover: outcome.failover,
                    },
                    Some("TASKLIST.md") => BgTaskOutput::Tasklist {
                        reply: outcome.reply,
                        session: sess,
                        out_path: "TASKLIST.md",
                        failover: outcome.failover,
                    },
                    _ => BgTaskOutput::Chat {
                        reply: outcome.reply,
                        session: sess,
                        failover: outcome.failover,
                    },
                };
                let _ = tx.send(msg).await;
            }
            Err(e) => {
                let _ = tx.send(BgTaskOutput::Error(e.to_string())).await;
            }
        }
    })
}

fn spawn_fetch_models_task(
    tx: tokio::sync::mpsc::Sender<BgTaskOutput>,
    app: &mut TuiApp,
    target_provider: &str,
) -> tokio::task::JoinHandle<()> {
    let prov = Config::canonical_provider_name(target_provider);
    let base_url = app.agent.config.get_base_url(&prov);
    let api_key = app.agent.config.get_api_key(&prov);
    app.messages.push(ChatMessageItem {
        sender: "System".to_string(),
        text: format!("🔍 Mendeteksi & menyinkronkan daftar model dari endpoint '{}'...", prov),
        mode_name: app.mode.name().to_string(),
        mode_color: app.mode.color(),
        model_info: format!("{} {}", app.active_model, app.active_provider),
    });
    app.scroll_to_bottom();
    app.is_loading = true;
    tokio::spawn(async move {
        match crate::provider::fetch_remote_models(&prov, base_url.as_deref(), api_key.as_deref()).await {
            Ok(models) => {
                let _ = tx.send(BgTaskOutput::FetchedModels { provider: prov, models }).await;
            }
            Err(e) => {
                let _ = tx.send(BgTaskOutput::Error(format!("Gagal fetch model dari '{}': {:#}", prov, e))).await;
            }
        }
    })
}

const SCANNER_WIDTH: usize = 8;
const SCANNER_HOLD_END: usize = 9;
const SCANNER_HOLD_START: usize = 30;
const SCANNER_TRAIL_LENGTH: usize = 6;

pub fn get_scanner_state(frame_index: usize, width: usize) -> (usize, bool, usize, bool) {
    let forward_frames = width;
    let hold_end_frames = SCANNER_HOLD_END;
    let backward_frames = width.saturating_sub(1);
    let hold_start_frames = SCANNER_HOLD_START;
    let total_frames = forward_frames + hold_end_frames + backward_frames + hold_start_frames;

    let f = frame_index % total_frames;

    if f < forward_frames {
        (f, false, 0, true)
    } else if f < forward_frames + hold_end_frames {
        (width - 1, true, f - forward_frames, true)
    } else if f < forward_frames + hold_end_frames + backward_frames {
        let b_idx = f - forward_frames - hold_end_frames;
        (width - 2 - b_idx, false, 0, false)
    } else {
        (0, true, f - forward_frames - hold_end_frames - backward_frames, false)
    }
}

pub fn calculate_scanner_index(frame_index: usize, char_index: usize, width: usize, trail_length: usize) -> i32 {
    let (active_pos, is_holding, hold_progress, is_moving_forward) = get_scanner_state(frame_index, width);

    let directional_distance = if is_moving_forward {
        active_pos as isize - char_index as isize
    } else {
        char_index as isize - active_pos as isize
    };

    if is_holding {
        return if directional_distance >= 0 {
            (directional_distance + hold_progress as isize) as i32
        } else {
            -1
        };
    }

    if directional_distance > 0 && (directional_distance as usize) < trail_length {
        return directional_distance as i32;
    }

    if directional_distance == 0 {
        return 0;
    }

    -1
}

pub fn render_scanner_spans(frame_index: usize, mode_color: Color) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(SCANNER_WIDTH);
    for char_index in 0..SCANNER_WIDTH {
        let idx = calculate_scanner_index(frame_index, char_index, SCANNER_WIDTH, SCANNER_TRAIL_LENGTH);
        if idx >= 0 && (idx as usize) < SCANNER_TRAIL_LENGTH {
            if idx == 0 {
                spans.push(Span::styled("■", Style::default().fg(mode_color).add_modifier(Modifier::BOLD)));
            } else if idx == 1 {
                spans.push(Span::styled("■", Style::default().fg(mode_color)));
            } else {
                spans.push(Span::styled("■", Style::default().fg(Color::Rgb(100, 116, 139))));
            }
        } else {
            spans.push(Span::styled("⬝", Style::default().fg(Color::Rgb(71, 85, 105))));
        }
    }
    spans
}

pub fn render_interrupt_status_spans(armed: bool) -> Vec<Span<'static>> {
    if armed {
        vec![
            Span::styled("  esc ", Style::default().fg(Color::Rgb(251, 191, 36)).add_modifier(Modifier::BOLD)),
            Span::styled("again to interrupt", Style::default().fg(Color::Rgb(251, 191, 36))),
        ]
    } else {
        vec![
            Span::styled("  esc ", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled("interrupt", Style::default().fg(TEXT_MUTED)),
        ]
    }
}

pub fn format_api_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    // Identify provider name if mentioned in raw error
    let provider_prefix = if lower.contains("gemini") || lower.contains("google") {
        "Gemini"
    } else if lower.contains("groq") {
        "Groq"
    } else if lower.contains("deepseek") {
        "DeepSeek"
    } else if lower.contains("openrouter") {
        "OpenRouter"
    } else if lower.contains("sambanova") {
        "SambaNova"
    } else if lower.contains("mistral") {
        "Mistral"
    } else if lower.contains("github") {
        "GitHub Models"
    } else if lower.contains("anthropic") || lower.contains("claude") {
        "Anthropic"
    } else if lower.contains("openai") {
        "OpenAI"
    } else if lower.contains("ollama") {
        "Ollama"
    } else {
        "Provider"
    };

    // 1. Check for Unauthorized / Invalid Key
    if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("api_key_invalid")
        || lower.contains("invalid api key")
        || lower.contains("api key not valid")
        || lower.contains("invalid_api_key")
        || lower.contains("authentication failed")
    {
        return format!("✖ {}: API key tidak valid atau salah (HTTP 401)", provider_prefix);
    }

    // 2. Check for Quota / Rate Limit
    if lower.contains("429")
        || lower.contains("resource_exhausted")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("quota exceeded")
        || lower.contains("too many requests")
        || lower.contains("free usage limit reached")
    {
        return format!("✖ {}: Kuota habis atau batas rate limit tercapai (HTTP 429)", provider_prefix);
    }

    // 3. Check for Model Not Found / Deprecated
    if lower.contains("404")
        || lower.contains("not_found")
        || lower.contains("model not found")
        || lower.contains("model_not_found")
        || lower.contains("does not exist")
        || lower.contains("is not available")
    {
        return format!("✖ {}: Model tidak ditemukan / tidak tersedia di tier ini (HTTP 404)", provider_prefix);
    }

    // 4. Check for Forbidden / Region Block
    if lower.contains("403")
        || lower.contains("forbidden")
        || lower.contains("permission_denied")
        || lower.contains("country, region, or territory")
        || lower.contains("not supported in your country")
    {
        return format!("✖ {}: Wilayah/negara tidak didukung atau akses ditolak (HTTP 403)", provider_prefix);
    }

    // 5. Server Errors (500, 502, 503, 504)
    if lower.contains("500") || lower.contains("502") || lower.contains("503") || lower.contains("504") || lower.contains("overloaded") {
        return format!("✖ {}: Server sedang mengalami gangguan / overload (HTTP 5xx)", provider_prefix);
    }

    // 6. Network / DNS / Timeout
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("connection refused") || lower.contains("failed to lookup address") {
        return format!("✖ {}: Gagal terhubung ke jaringan provider (Timeout / Jaringan)", provider_prefix);
    }

    // 7. Try parsing JSON error message if present: {"error": {"message": "..."}}
    if let Some(start) = raw.find('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw[start..]) {
            if let Some(msg) = val.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                return format!("✖ {}: {}", provider_prefix, msg);
            }
        }
    }

    // Cleaned fallback message
    let first_line = raw.lines().next().unwrap_or(raw).trim();
    if first_line.len() > 80 {
        format!("✖ {}: {}...", provider_prefix, &first_line[..75])
    } else {
        format!("✖ {}: {}", provider_prefix, first_line)
    }
}

pub fn resolve_char_with_shift(code: KeyCode, modifiers: KeyModifiers) -> Option<char> {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match code {
        KeyCode::Char(c) => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                if c.is_ascii_lowercase() {
                    Some(c.to_ascii_uppercase())
                } else {
                    let mapped = match c {
                        '/' => '?',
                        '1' => '!',
                        '2' => '@',
                        '3' => '#',
                        '4' => '$',
                        '5' => '%',
                        '6' => '^',
                        '7' => '&',
                        '8' => '*',
                        '9' => '(',
                        '0' => ')',
                        '-' => '_',
                        '=' => '+',
                        '[' => '{',
                        ']' => '}',
                        '\\' => '|',
                        ';' => ':',
                        '\'' => '"',
                        ',' => '<',
                        '.' => '>',
                        '`' => '~',
                        other => other,
                    };
                    Some(mapped)
                }
            } else {
                Some(c)
            }
        }
        _ => None,
    }
}

pub fn get_provider_guide(filter: Option<&str>) -> String {
    let f = filter.unwrap_or("").to_lowercase();
    if f == "deepseek" {
        return r#"### 🐋 Panduan DeepSeek API (Resmi V4)
1. Buka: https://platform.deepseek.com/api_keys
2. Login dan klik **Create API Key**.
3. Salin API key (`sk-...`).
4. Cara pasang di Lontar:
   • Cara 1: Tekan `ctrl+x m`, pilih DeepSeek V4.1 Flash, lalu paste API key.
   • Cara 2: Ketik `/custom deepseek sk-...`
   • Cara 3: Ketik `/config set deepseek_api_key sk-...`
*Model Resmi Aktif (V4):*
• `deepseek-flash` (DeepSeek-V4.1-Flash - Cepat & Default Resmi)
• `deepseek-v4-pro` (DeepSeek-V4-Pro - Level kemampuan tertinggi untuk developer)
• `deepseek-reasoner` (DeepSeek-R1 - Penalaran mendalam step-by-step)
• `deepseek-chat` (Legacy V3 - Tetap didukung)
💡 Ganti model instan: `/model deepseek-flash` atau `/model deepseek-v4-pro`"#.to_string();
    }

    if f == "gemini" || f == "google" {
        return r#"### 📘 Panduan Google Gemini API (Google AI Studio)
1. Buka: https://aistudio.google.com/apikey
2. Login akun Google dan klik **Create API Key**.
3. Salin API Key (format: `AIzaSy...`).
4. Di Lontar, jalankan:
   `/config set gemini_api_key AIzaSy...`
   atau tekan `ctrl+x m` lalu pilih Gemini.
*Model Tier Gratis Aktif 2026:*
• `gemini-3.5-flash-lite` (Aktif Free Tier, latensi ultra rendah)
• `gemini-3.5-flash` (Next-gen reasoning)
• `gemini-3.8-flash` (Advanced reasoning)
• `gemini-flash-latest` (Otomatis diarahkan ke model flash terbaru)"#.to_string();
    }

    if f == "groq" {
        return r#"### ⚡ Panduan Groq Cloud (Kecepatan 300+ token/detik)
1. Buka: https://console.groq.com/keys
2. Sign up / Login, lalu klik **Create API Key**.
3. Salin key (`gsk_...`).
4. Di Lontar, jalankan:
   `/config set groq_api_key gsk_...`
   atau `/custom groq gsk_...`
*1 API key berlaku untuk semua model Groq:*
• `llama-3.3-70b-versatile` (30 RPM / 6000 TPM)
• `llama-3.1-8b-instant` (Ultra-cepat)"#.to_string();
    }

    if f == "openrouter" {
        return r#"### 🌐 Panduan OpenRouter (Akses Berbagai Model Gratis)
1. Buka: https://openrouter.ai/settings/keys
2. Klik **Create Key** (tidak butuh kartu kredit).
3. Di Lontar, jalankan:
   `/config set openrouter_api_key sk-or-v1-...`
   atau `/custom openrouter sk-or-v1-...`
*Model Gratis (akhiran `:free`):*
• `nvidia/nemotron-3.5-lightning:free` (1M konteks)
• `deepseek/deepseek-r1:free` (Reasoning model open-weight)
• `meta-llama/llama-3.3-70b-instruct:free`
• `google/gemma-4-26b-a4b-it:free`"#.to_string();
    }

    if f == "sambanova" || f == "samba" {
        return r#"### ⚡ Panduan SambaNova Cloud (Inference RDU Super Cepat)
1. Buka: https://cloud.sambanova.ai/apis
2. Buat API key gratis.
3. Pasang di Lontar:
   `/custom sambanova <api_key>`
   atau `/config set sambanova_api_key <api_key>`
*Model Aktif:*
• `Meta-Llama-3.3-70B-Instruct`
• `Qwen2.5-72B-Instruct`"#.to_string();
    }

    if f == "z.ai" || f == "zai" || f == "zhipu" {
        return r#"### 🇨🇳 Panduan Z.AI / Zhipu GLM
1. Buka: https://open.bigmodel.cn atau https://z.ai
2. Dapatkan API Key.
3. Pasang di Lontar:
   `/custom z.ai <api_key>`
*Model Gratis:*
• `glm-4.5-flash` (Gratis tanpa biaya kuota)"#.to_string();
    }

    if f == "nvidia" || f == "nim" {
        return r#"### 🟢 Panduan NVIDIA NIM (Free 1000 Credits)
1. Kunjungi: https://build.nvidia.com
2. Buat akun NVIDIA Developer dan generate API key (`nvapi-...`).
3. Sambungkan ke Lontar via:
   `/custom nvidia <api_key>`"#.to_string();
    }

    if f == "cerebras" {
        return r#"### 🚀 Panduan Cerebras Cloud (1 Juta Token/Hari Gratis)
1. Buka: https://cloud.cerebras.ai
2. Buat API key (`csk-...`).
3. Sambungkan ke Lontar via:
   `/custom cerebras <api_key>`"#.to_string();
    }

    // Default Full Guide
    r#"### 📚 Panduan Mendapatkan API Key AI Gratis (Free LLM Tiers)

💡 **Konsep Penting:**
1 API Key adalah tiket masuk untuk **seluruh provider**, bukan per-model!
Contoh: 1 API Key DeepSeek otomatis bisa dipakai untuk `deepseek-chat` maupun `deepseek-reasoner` (R1).

Lontar mendukung berbagai provider AI gratis berkualitas tinggi:

1. **DeepSeek Direct**
   • URL: https://platform.deepseek.com/api_keys
   • Model: `deepseek-chat` (V3), `deepseek-reasoner` (R1)
   • Setup: `/custom deepseek <KEY>` atau tekan `ctrl+x m` lalu pilih DeepSeek.

2. **Google Gemini (Google AI Studio)**
   • URL: https://aistudio.google.com/apikey
   • Model 2026 Aktif: `gemini-3.5-flash-lite`, `gemini-3.5-flash`, `gemini-3.8-flash`
   • Setup: `/config set gemini_api_key <KEY>`

3. **Groq Cloud (Inference Tercepat 300+ tok/s)**
   • URL: https://console.groq.com/keys
   • Model: `llama-3.3-70b-versatile`, `llama-3.1-8b-instant`
   • Setup: `/custom groq <KEY>` atau `/config set groq_api_key <KEY>`

4. **OpenRouter (Koleksi Model Gratis Terlengkap)**
   • URL: https://openrouter.ai/settings/keys
   • Model: `nvidia/nemotron-3.5-lightning:free`, `deepseek/deepseek-r1:free`
   • Setup: `/custom openrouter <KEY>`

5. **SambaNova Cloud (Akselerator LPU)**
   • URL: https://cloud.sambanova.ai/apis
   • Model: `Meta-Llama-3.3-70B-Instruct`
   • Setup: `/custom sambanova <KEY>`

6. **Z.AI / Zhipu (GLM-4.5-Flash Free)**
   • URL: https://open.bigmodel.cn
   • Setup: `/custom z.ai <KEY>`

7. **Koleksi API Gratis Lainnya**:
   • https://github.com/mnfst/awesome-free-llm-apis
   • https://github.com/OuterSpacee/free-ai-apis

💡 **Tips Praktis:**
• Auto-detect model aktif langsung dari server: `/models fetch`
• Ganti model provider aktif: `/config set <provider>_model <nama_model>`
• Pilih model visual: tekan `ctrl+x m`"#.to_string()
}

pub async fn run_tui(session: Session, config: Config, cli_yes: bool) -> Result<()> {
    let mut app = TuiApp::new(session, config, cli_yes)?;

    enable_raw_mode()?;
    let _guard = TerminalGuard;

    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0),
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    );
    let _ = stdout.write_all(b"\x1b[?1007h");
    let _ = stdout.flush();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (task_tx, mut task_rx) = tokio::sync::mpsc::channel::<BgTaskOutput>(1);
    let mut active_task: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        app.spinner_frame = app.spinner_frame.wrapping_add(1);

        // Auto-disarm interrupt after 3 seconds without second Esc press
        if let Some(t) = app.armed_time {
            if t.elapsed().as_secs() >= 3 {
                app.interrupt_armed = false;
                app.armed_time = None;
            }
        }

        // Check if background task completed
        if let Ok(output) = task_rx.try_recv() {
            active_task = None;
            app.is_loading = false;
            app.interrupt_armed = false;
            app.armed_time = None;

            match output {
                BgTaskOutput::Chat { reply, session, failover } => {
                    app.session = session;
                    let current_mode_name = app.mode.name().to_string();
                    let current_mode_color = app.mode.color();
                    let current_model_info = app.apply_failover(failover);
                    app.messages.push(ChatMessageItem {
                        sender: "Lontar".to_string(),
                        text: reply,
                        mode_name: current_mode_name,
                        mode_color: current_mode_color,
                        model_info: current_model_info,
                    });
                    app.refresh_docs();
                    app.scroll_to_bottom();
                }
                BgTaskOutput::StudyGuide { reply, session, out_path, failover } => {
                    app.session = session;
                    let model_info = app.apply_failover(failover);
                    app.messages.push(ChatMessageItem {
                        sender: "Lontar".to_string(),
                        text: format!("✔ Tersimpan ke `{}`:\n\n{}", out_path, reply),
                        mode_name: "Study".to_string(),
                        mode_color: COLOR_STUDY,
                        model_info,
                    });
                    app.refresh_docs();
                    app.scroll_to_bottom();
                }
                BgTaskOutput::Prd { reply, session, out_path, failover } => {
                    app.session = session;
                    let model_info = app.apply_failover(failover);
                    app.messages.push(ChatMessageItem {
                        sender: "Lontar".to_string(),
                        text: format!("✔ Tersimpan ke `{}`:\n\n{}", out_path, reply),
                        mode_name: "Plan".to_string(),
                        mode_color: COLOR_PLAN,
                        model_info,
                    });
                    app.refresh_docs();
                    app.scroll_to_bottom();
                }
                BgTaskOutput::Tasklist { reply, session, out_path, failover } => {
                    app.session = session;
                    let model_info = app.apply_failover(failover);
                    app.messages.push(ChatMessageItem {
                        sender: "Lontar".to_string(),
                        text: format!("✔ Tersimpan ke `{}`:\n\n{}", out_path, reply),
                        mode_name: "Plan".to_string(),
                        mode_color: COLOR_PLAN,
                        model_info,
                    });
                    app.refresh_docs();
                    app.scroll_to_bottom();
                }
                BgTaskOutput::FetchedModels { provider, models } => {
                    let text = app.apply_fetched_models_with_save(&provider, &models, true);
                    app.messages.push(ChatMessageItem {
                        sender: "System".to_string(),
                        text,
                        mode_name: app.mode.name().to_string(),
                        mode_color: app.mode.color(),
                        model_info: format!("{} {}", app.active_model, app.active_provider),
                    });
                    app.scroll_to_bottom();
                }
                BgTaskOutput::Error(err_msg) => {
                    let formatted = format_api_error(&err_msg);
                    app.api_error = Some(formatted.clone());
                    app.messages.push(ChatMessageItem {
                        sender: "Error".to_string(),
                        text: formatted,
                        mode_name: app.mode.name().to_string(),
                        mode_color: app.mode.color(),
                        model_info: format!("{} {}", app.active_model, app.active_provider),
                    });
                    app.scroll_to_bottom();
                }
            }
        }

        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(40))? {
            let ev = event::read()?;
            match ev {
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            if app.show_commands_modal {
                                app.selected_command_index = app.selected_command_index.saturating_sub(1);
                                if app.selected_command_index < app.palette_scroll_offset {
                                    app.palette_scroll_offset = app.selected_command_index;
                                }
                            } else if app.show_models_modal {
                                app.selected_model_index = app.selected_model_index.saturating_sub(1);
                                if app.selected_model_index < app.model_scroll_offset {
                                    app.model_scroll_offset = app.selected_model_index;
                                }
                            } else if app.show_sessions_modal {
                                app.selected_session_index = app.selected_session_index.saturating_sub(1);
                                if app.selected_session_index < app.session_scroll_offset {
                                    app.session_scroll_offset = app.selected_session_index;
                                }
                            } else {
                                app.scroll_up(3);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if app.show_commands_modal {
                                let filtered = app.get_filtered_palette();
                                if !filtered.is_empty() {
                                    let max_idx = filtered.len().saturating_sub(1);
                                    app.selected_command_index = (app.selected_command_index + 1).min(max_idx);
                                    if app.selected_command_index >= app.palette_scroll_offset + 7 {
                                        app.palette_scroll_offset = app.selected_command_index + 1 - 7;
                                    }
                                }
                            } else if app.show_models_modal {
                                let filtered = app.get_filtered_models();
                                if !filtered.is_empty() {
                                    let max_idx = filtered.len().saturating_sub(1);
                                    app.selected_model_index = (app.selected_model_index + 1).min(max_idx);
                                    if app.selected_model_index >= app.model_scroll_offset + 8 {
                                        app.model_scroll_offset = app.selected_model_index + 1 - 8;
                                    }
                                }
                            } else if app.show_sessions_modal {
                                let filtered = app.get_filtered_sessions();
                                if !filtered.is_empty() {
                                    let max_idx = filtered.len().saturating_sub(1);
                                    app.selected_session_index = (app.selected_session_index + 1).min(max_idx);
                                    if app.selected_session_index >= app.session_scroll_offset + 8 {
                                        app.session_scroll_offset = app.selected_session_index + 1 - 8;
                                    }
                                }
                            } else {
                                app.scroll_down(3);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                // If loading, Esc arms and triggers interrupt (OpenCode parity)
                if app.is_loading {
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) => {
                            if !app.interrupt_armed {
                                app.interrupt_armed = true;
                                app.armed_time = Some(Instant::now());
                            } else {
                                if let Some(h) = active_task.take() {
                                    h.abort();
                                }
                                app.is_loading = false;
                                app.interrupt_armed = false;
                                app.armed_time = None;
                                app.messages.push(ChatMessageItem {
                                    sender: "Lontar".to_string(),
                                    text: "· interrupted".to_string(),
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                                app.scroll_to_bottom();
                            }
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                            if let Some(h) = active_task.take() {
                                h.abort();
                            }
                            app.is_loading = false;
                            app.interrupt_armed = false;
                            app.armed_time = None;
                            app.messages.push(ChatMessageItem {
                                sender: "Lontar".to_string(),
                                text: "· interrupted".to_string(),
                                mode_name: app.mode.name().to_string(),
                                mode_color: app.mode.color(),
                                model_info: format!("{} {}", app.active_model, app.active_provider),
                            });
                            app.scroll_to_bottom();
                        }
                        _ => {}
                    }
                    continue;
                }

                // Pending Ctrl+X key chords (e.g. ctrl+x m -> select model, ctrl+x l -> sessions)
                if app.pending_ctrl_x {
                    app.pending_ctrl_x = false;
                    match key.code {
                        KeyCode::Char('m') => {
                            app.open_models_modal();
                            continue;
                        }
                        KeyCode::Char('l') => {
                            app.show_sessions_modal = true;
                            app.session_search.clear();
                            app.selected_session_index = 0;
                            app.session_scroll_offset = 0;
                            app.refresh_sessions();
                            continue;
                        }
                        KeyCode::Char('n') => {
                            let _ = app.start_new_session();
                            continue;
                        }
                        _ => {}
                    }
                }

                // Modal 1: Command Palette
                if app.show_commands_modal {
                    let filtered = app.get_filtered_palette();
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) => {
                            app.show_commands_modal = false;
                            app.command_search.clear();
                        }
                        (_, KeyCode::Up) => {
                            app.selected_command_index = app.selected_command_index.saturating_sub(1);
                            if app.selected_command_index < app.palette_scroll_offset {
                                app.palette_scroll_offset = app.selected_command_index;
                            }
                        }
                        (_, KeyCode::Down) => {
                            if !filtered.is_empty() {
                                let max_idx = filtered.len().saturating_sub(1);
                                app.selected_command_index = (app.selected_command_index + 1).min(max_idx);
                                if app.selected_command_index >= app.palette_scroll_offset + 7 {
                                    app.palette_scroll_offset = app.selected_command_index + 1 - 7;
                                }
                            }
                        }
                        (_, KeyCode::Enter) => {
                            if !filtered.is_empty() && app.selected_command_index < filtered.len() {
                                let item = filtered[app.selected_command_index];
                                app.show_commands_modal = false;
                                app.command_search.clear();

                                match item.action {
                                    "new_session" => {
                                        let _ = app.start_new_session();
                                    }
                                    "model_ui" => {
                                        app.open_models_modal();
                                    }
                                    "session_ui" => {
                                        app.show_sessions_modal = true;
                                        app.session_search.clear();
                                        app.selected_session_index = 0;
                                        app.session_scroll_offset = 0;
                                        app.refresh_sessions();
                                    }
                                    "mode" => {
                                        app.mode = app.mode.next();
                                    }
                                    "sources" => {
                                        app.show_sources_modal = true;
                                    }
                                    "clear" => {
                                        app.messages.clear();
                                        app.scroll_to_bottom();
                                    }
                                    "exit" => {
                                        app.should_quit = true;
                                    }
                                    "diff" => {
                                        app.input = "/diff".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "context_add" => {
                                        app.input = "/context add ".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "study_guide" => {
                                        app.input = "/study-guide ".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "prd" => {
                                        app.input = "/prd ".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "tasklist" => {
                                        app.input = "/tasklist ".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "guide" => {
                                        let guide_text = get_provider_guide(None);
                                        app.messages.push(ChatMessageItem {
                                            sender: "System".to_string(),
                                            text: guide_text,
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                        app.scroll_to_bottom();
                                    }
                                    "custom_provider" => {
                                        app.input = "/custom ".to_string();
                                        app.cursor_position = app.input.len();
                                    }
                                    "fetch_models" => {
                                        let prov = app.active_provider.clone();
                                        active_task = Some(spawn_fetch_models_task(task_tx.clone(), &mut app, &prov));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        (_, KeyCode::Backspace) => {
                            app.command_search.pop();
                            app.selected_command_index = 0;
                            app.palette_scroll_offset = 0;
                        }
                        (_, KeyCode::Char(c)) => {
                            if let Some(ch) = resolve_char_with_shift(KeyCode::Char(c), key.modifiers) {
                                app.command_search.push(ch);
                                app.selected_command_index = 0;
                                app.palette_scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Modal 2: Select Model Modal (Image 4 Dedicated UI)
                if app.show_models_modal {
                    let filtered = app.get_filtered_models();
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) => {
                            app.show_models_modal = false;
                            app.model_search.clear();
                        }
                        (_, KeyCode::Up) => {
                            app.selected_model_index = app.selected_model_index.saturating_sub(1);
                            if app.selected_model_index < app.model_scroll_offset {
                                app.model_scroll_offset = app.selected_model_index;
                            }
                        }
                        (_, KeyCode::Down) => {
                            if !filtered.is_empty() {
                                let max_idx = filtered.len().saturating_sub(1);
                                app.selected_model_index = (app.selected_model_index + 1).min(max_idx);
                                if app.selected_model_index >= app.model_scroll_offset + 8 {
                                    app.model_scroll_offset = app.selected_model_index + 1 - 8;
                                }
                            }
                        }
                        (_, KeyCode::Enter) => {
                            if !filtered.is_empty() && app.selected_model_index < filtered.len() {
                                let choice = filtered[app.selected_model_index].clone();
                                let has_key = app.agent.config.has_configured_provider(&choice.provider_id);

                                if has_key {
                                    app.persist_active_provider(&choice.provider_id, Some(&choice.model_id));
                                    app.show_models_modal = false;
                                    app.model_search.clear();
                                } else {
                                    // Open In-TUI Connect Modal! (Exact OpenCode DialogIntegration Parity)
                                    app.show_models_modal = false;
                                    app.model_search.clear();
                                    app.show_connect_modal = true;
                                    app.connect_provider_id = choice.provider_id;
                                    app.connect_model_id = choice.model_id;
                                    app.connect_api_key_input.clear();
                                    app.connect_cursor_position = 0;
                                }
                            }
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                            let target_prov = if !filtered.is_empty() && app.selected_model_index < filtered.len() {
                                filtered[app.selected_model_index].provider_id.clone()
                            } else {
                                app.active_provider.clone()
                            };
                            app.show_models_modal = false;
                            app.model_search.clear();
                            active_task = Some(spawn_fetch_models_task(task_tx.clone(), &mut app, &target_prov));
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                            if !filtered.is_empty() && app.selected_model_index < filtered.len() {
                                let choice = &filtered[app.selected_model_index];
                                app.show_models_modal = false;
                                app.input = format!("/config set {}_api_key ", choice.provider_id);
                                app.cursor_position = app.input.len();
                            }
                        }
                        (_, KeyCode::Backspace) => {
                            app.model_search.pop();
                            app.selected_model_index = 0;
                            app.model_scroll_offset = 0;
                        }
                        (_, KeyCode::Char(c)) => {
                            if let Some(ch) = resolve_char_with_shift(KeyCode::Char(c), key.modifiers) {
                                app.model_search.push(ch);
                                app.selected_model_index = 0;
                                app.model_scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Modal 3: Sessions Modal (Image 5 Dedicated UI)
                if app.show_sessions_modal {
                    let filtered = app.get_filtered_sessions();
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) => {
                            app.show_sessions_modal = false;
                            app.session_search.clear();
                        }
                        (_, KeyCode::Up) => {
                            app.selected_session_index = app.selected_session_index.saturating_sub(1);
                            if app.selected_session_index < app.session_scroll_offset {
                                app.session_scroll_offset = app.selected_session_index;
                            }
                        }
                        (_, KeyCode::Down) => {
                            if !filtered.is_empty() {
                                let max_idx = filtered.len().saturating_sub(1);
                                app.selected_session_index = (app.selected_session_index + 1).min(max_idx);
                                if app.selected_session_index >= app.session_scroll_offset + 8 {
                                    app.session_scroll_offset = app.selected_session_index + 1 - 8;
                                }
                            }
                        }
                        (_, KeyCode::Enter) => {
                            if !filtered.is_empty() && app.selected_session_index < filtered.len() {
                                let sess_header = filtered[app.selected_session_index];
                                let sess_id = sess_header.id.clone();
                                let _ = app.load_selected_session(&sess_id);
                            }
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                            let _ = app.start_new_session();
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                            if !filtered.is_empty() && app.selected_session_index < filtered.len() {
                                let sess_header = filtered[app.selected_session_index];
                                let sess_id = sess_header.id.clone();
                                let _ = app.delete_selected_session(&sess_id);
                                app.selected_session_index = 0;
                                app.session_scroll_offset = 0;
                            }
                        }
                        (_, KeyCode::Backspace) => {
                            app.session_search.pop();
                            app.selected_session_index = 0;
                            app.session_scroll_offset = 0;
                        }
                        (_, KeyCode::Char(c)) => {
                            if let Some(ch) = resolve_char_with_shift(KeyCode::Char(c), key.modifiers) {
                                app.session_search.push(ch);
                                app.selected_session_index = 0;
                                app.session_scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Modal 4: Sources & Tasks Modal (Ctrl+T)
                if app.show_sources_modal {
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) | (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                            app.show_sources_modal = false;
                        }
                        (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                            app.should_quit = true;
                        }
                        _ => {}
                    }
                    continue;
                }

                // Modal 5: Connect Provider Modal (OpenCode DialogIntegration parity)
                if app.show_connect_modal {
                    match (key.modifiers, key.code) {
                        (_, KeyCode::Esc) => {
                            app.show_connect_modal = false;
                            app.connect_api_key_input.clear();
                        }
                        (_, KeyCode::Enter) => {
                            let key_val = app.connect_api_key_input.trim().to_string();
                            if !key_val.is_empty() {
                                let canonical_prov = Config::canonical_provider_name(&app.connect_provider_id);
                                let target_model = if !app.connect_model_id.trim().is_empty() {
                                    app.connect_model_id.clone()
                                } else {
                                    app.agent.config.get_model(&canonical_prov)
                                };

                                if canonical_prov == "z.ai" {
                                    let base_url = app
                                        .agent
                                        .config
                                        .get_base_url("z.ai")
                                        .unwrap_or_else(|| "https://api.z.ai/api/paas/v4".to_string());
                                    let _ = app.agent.config.set_custom_provider(
                                        "z.ai",
                                        &base_url,
                                        Some(&key_val),
                                        &target_model,
                                    );
                                } else if !Config::is_builtin_provider(&canonical_prov) {
                                    let base_url = app
                                        .agent
                                        .config
                                        .get_base_url(&canonical_prov)
                                        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
                                    let _ = app.agent.config.set_custom_provider(
                                        &canonical_prov,
                                        &base_url,
                                        Some(&key_val),
                                        &target_model,
                                    );
                                } else {
                                    let key_name = if canonical_prov == "github" {
                                        "github_token".to_string()
                                    } else {
                                        format!("{}_api_key", canonical_prov)
                                    };
                                    let _ = app.agent.config.set_value(&key_name, &key_val);
                                    app.agent.config.custom_providers.remove(&canonical_prov);
                                }

                                app.persist_active_provider(&canonical_prov, Some(&target_model));
                                app.show_connect_modal = false;
                                app.connect_api_key_input.clear();
                                app.connect_cursor_position = 0;
                                app.messages.push(ChatMessageItem {
                                    sender: "System".to_string(),
                                    text: format!(
                                        "✔ Berhasil menghubungkan provider '{}' dengan model '{}' (tersimpan permanen ke config.toml)!\n💡 1 API key ini otomatis berlaku untuk semua model dari provider '{}'.",
                                        canonical_prov, app.active_model, canonical_prov
                                    ),
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                            }
                        }
                        (_, KeyCode::Backspace) => {
                            if app.connect_cursor_position > 0 {
                                app.connect_api_key_input.remove(app.connect_cursor_position - 1);
                                app.connect_cursor_position -= 1;
                            }
                        }
                        (_, KeyCode::Char(c)) => {
                            if let Some(ch) = resolve_char_with_shift(KeyCode::Char(c), key.modifiers) {
                                app.connect_api_key_input.insert(app.connect_cursor_position, ch);
                                app.connect_cursor_position += 1;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Normal Mode Key Handling
                match (key.modifiers, key.code) {
                    // Only Ctrl+C quits! Esc does NOT quit!
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                        app.should_quit = true;
                    }
                    (_, KeyCode::Esc) => {
                        if !app.input.is_empty() {
                            app.input.clear();
                            app.cursor_position = 0;
                            app.slash_selected_index = 0;
                        }
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
                        app.show_commands_modal = true;
                        app.command_search.clear();
                        app.selected_command_index = 0;
                        app.palette_scroll_offset = 0;
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('x')) => {
                        app.pending_ctrl_x = true;
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
                        let _ = app.start_new_session();
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
                        app.show_sources_modal = !app.show_sources_modal;
                    }
                    (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                        app.messages.clear();
                        app.scroll_to_bottom();
                    }
                    // Tab switches MODE or autocompletes matching slash command
                    (_, KeyCode::Tab) => {
                        let matches = app.get_matching_slash_commands();
                        if !matches.is_empty() {
                            let sel = app.slash_selected_index.min(matches.len().saturating_sub(1));
                            let chosen = matches[sel];
                            app.input = format!("{} ", chosen.name);
                            app.cursor_position = app.input.len();
                            app.slash_selected_index = 0;
                        } else {
                            app.mode = app.mode.next();
                        }
                    }
                    // Multi-line input support:
                    // 1. Shift+Enter
                    // 2. Alt/Option+Enter
                    // 3. Ctrl+Enter
                    // 4. Ctrl+J (universal ASCII 10 Linefeed)
                    // 5. Explicit '\n' character
                    (KeyModifiers::SHIFT, KeyCode::Enter)
                    | (KeyModifiers::ALT, KeyCode::Enter)
                    | (KeyModifiers::CONTROL, KeyCode::Enter)
                    | (KeyModifiers::CONTROL, KeyCode::Char('j'))
                    | (_, KeyCode::Char('\n')) => {
                        app.input.insert(app.cursor_position, '\n');
                        app.cursor_position += 1;
                    }
                    (_, KeyCode::Up) => {
                        let matches = app.get_matching_slash_commands();
                        if !matches.is_empty() {
                            if app.slash_selected_index > 0 {
                                app.slash_selected_index -= 1;
                            } else {
                                app.slash_selected_index = matches.len() - 1;
                            }
                        } else if app.input.is_empty() {
                            app.scroll_up(2);
                        }
                    }
                    (_, KeyCode::Down) => {
                        let matches = app.get_matching_slash_commands();
                        if !matches.is_empty() {
                            app.slash_selected_index = (app.slash_selected_index + 1) % matches.len();
                        } else if app.input.is_empty() {
                            app.scroll_down(2);
                        }
                    }
                    (_, KeyCode::PageUp) => {
                        app.scroll_up(8);
                    }
                    (_, KeyCode::PageDown) => {
                        app.scroll_down(8);
                    }
                    (_, KeyCode::Left) => {
                        if app.cursor_position > 0 {
                            app.cursor_position -= 1;
                        }
                    }
                    (_, KeyCode::Right) => {
                        if app.cursor_position < app.input.len() {
                            app.cursor_position += 1;
                        }
                    }
                    (_, KeyCode::Home) => {
                        app.cursor_position = 0;
                    }
                    (_, KeyCode::End) => {
                        app.cursor_position = app.input.len();
                    }
                    (_, KeyCode::Backspace) => {
                        if app.cursor_position > 0 {
                            app.input.remove(app.cursor_position - 1);
                            app.cursor_position -= 1;
                            app.slash_selected_index = 0;
                        }
                    }
                    // Enter submits prompt (with shell line-continuation '\' check)
                    (_, KeyCode::Enter) => {
                        if app.input.ends_with('\\') {
                            app.input.pop();
                            if app.cursor_position > app.input.len() {
                                app.cursor_position = app.input.len();
                            }
                            app.input.insert(app.cursor_position, '\n');
                            app.cursor_position += 1;
                            continue;
                        }

                        let matches = app.get_matching_slash_commands();
                        let trimmed_input = app.input.trim().to_string();

                        // If user pressed Enter on a partially typed slash command, autocomplete it first
                        if !matches.is_empty() && trimmed_input.starts_with('/') {
                            let sel = app.slash_selected_index.min(matches.len().saturating_sub(1));
                            let chosen = matches[sel];
                            if trimmed_input != chosen.name && !trimmed_input.starts_with(&format!("{} ", chosen.name)) {
                                app.input = format!("{} ", chosen.name);
                                app.cursor_position = app.input.len();
                                app.slash_selected_index = 0;
                                continue;
                            }
                        }

                        let trimmed = trimmed_input;
                        if !trimmed.is_empty() {
                            app.input.clear();
                            app.cursor_position = 0;
                            app.slash_selected_index = 0;
                            app.scroll_to_bottom();
                            app.api_error = None;

                            if trimmed == "/exit" || trimmed == "/quit" {
                                app.should_quit = true;
                            } else if trimmed == "/clear" {
                                app.messages.clear();
                            } else if trimmed == "/" || trimmed == "/help" {
                                app.show_commands_modal = true;
                                app.command_search.clear();
                                app.selected_command_index = 0;
                                app.palette_scroll_offset = 0;
                            } else if trimmed == "/new" {
                                let _ = app.start_new_session();
                            } else if trimmed == "/model" {
                                app.open_models_modal();
                            } else if trimmed == "/session" {
                                app.show_sessions_modal = true;
                                app.session_search.clear();
                                app.selected_session_index = 0;
                                app.session_scroll_offset = 0;
                                app.refresh_sessions();
                            } else if trimmed == "/models fetch"
                                || trimmed == "/model fetch"
                                || trimmed.starts_with("/models fetch ")
                                || trimmed.starts_with("/model fetch ")
                            {
                                let arg = trimmed
                                    .trim_start_matches("/models fetch")
                                    .trim_start_matches("/model fetch")
                                    .trim();
                                let prov = if arg.is_empty() {
                                    app.active_provider.clone()
                                } else {
                                    Config::canonical_provider_name(arg)
                                };
                                active_task = Some(spawn_fetch_models_task(task_tx.clone(), &mut app, &prov));
                            } else if trimmed == "/guide" || trimmed.starts_with("/guide ") {
                                let prov_opt = if trimmed.starts_with("/guide ") {
                                    Some(trimmed.trim_start_matches("/guide ").trim())
                                } else {
                                    None
                                };
                                let guide_text = get_provider_guide(prov_opt);
                                app.messages.push(ChatMessageItem {
                                    sender: "System".to_string(),
                                    text: guide_text,
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                                app.scroll_to_bottom();
                            } else if trimmed == "/custom" || trimmed.starts_with("/custom ") {
                                let rest = trimmed.trim_start_matches("/custom").trim();
                                let parts: Vec<&str> = rest.split_whitespace().collect();
                                if parts.is_empty() {
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: "Cara mendaftarkan Custom Provider API:\n• Dengan Preset: `/custom <preset> <api_key>` (contoh: `/custom deepseek sk-...` atau `/custom z.ai 1ac...`)\n• Manual Lengkap: `/custom <nama> <base_url> [api_key] [model]`\n\nPreset didukung: `deepseek`, `groq`, `openrouter`, `sambanova`, `gemini`, `z.ai`, `cerebras`, `together`, `nvidia`, `mistral`".to_string(),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                } else if parts.len() == 2 && !parts[1].starts_with("http://") && !parts[1].starts_with("https://") {
                                    // User entered: `/custom <preset> <key>`
                                    let prov_name = parts[0].to_lowercase();
                                    let api_key = parts[1];
                                    let (canonical_name, base_url, model) = match prov_name.as_str() {
                                        "deepseek" => {
                                            ("deepseek", "https://api.deepseek.com", "deepseek-flash")
                                        }
                                        "gemini" | "google" => {
                                            ("gemini", "https://generativelanguage.googleapis.com", "gemini-3.5-flash-lite")
                                        }
                                        "groq" => {
                                            ("groq", "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile")
                                        }
                                        "openrouter" => {
                                            ("openrouter", "https://openrouter.ai/api/v1", "nvidia/nemotron-3.5-lightning:free")
                                        }
                                        "sambanova" | "samba" => {
                                            ("sambanova", "https://api.sambanova.ai/v1", "Meta-Llama-3.3-70B-Instruct")
                                        }
                                        "z.ai" | "zai" | "zhipu" | "bigmodel" => {
                                            ("z.ai", "https://api.z.ai/api/paas/v4", "glm-4.5-flash")
                                        }
                                        "cerebras" => {
                                            ("cerebras", "https://api.cerebras.ai/v1", "llama3.3-70b")
                                        }
                                        "together" => {
                                            ("together", "https://api.together.xyz/v1", "meta-llama/Llama-3.3-70B-Instruct-Turbo")
                                        }
                                        "nvidia" | "nim" => {
                                            ("nvidia", "https://integrate.api.nvidia.com/v1", "meta/llama-3.3-70b-instruct")
                                        }
                                        "mistral" => {
                                            ("mistral", "https://api.mistral.ai/v1", "mistral-small-latest")
                                        }
                                        other => {
                                            app.messages.push(ChatMessageItem {
                                                sender: "System".to_string(),
                                                text: format!("Provider '{}' belum memiliki preset. Harap sertakan Base URL lengkap:\n`/custom {} https://api.domain.com/v1 {} default`", other, other, api_key),
                                                mode_name: app.mode.name().to_string(),
                                                mode_color: app.mode.color(),
                                                model_info: format!("{} {}", app.active_model, app.active_provider),
                                            });
                                            app.scroll_to_bottom();
                                            continue;
                                        }
                                    };

                                    if Config::is_builtin_provider(canonical_name) {
                                        let key_name = if canonical_name == "github" {
                                            "github_token".to_string()
                                        } else {
                                            format!("{}_api_key", canonical_name)
                                        };
                                        let _ = app.agent.config.set_value(&key_name, api_key);
                                        app.agent.config.custom_providers.remove(canonical_name);
                                    } else {
                                        let _ = app.agent.config.set_custom_provider(canonical_name, base_url, Some(api_key), model);
                                    }
                                    app.persist_active_provider(canonical_name, Some(model));
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: format!("✔ Provider '{}' berhasil diaktifkan & disimpan permanen!\n• Model: {}\n• Base URL: {}\n💡 1 API Key ini berlaku untuk semua model dari provider '{}'.", canonical_name, model, base_url, canonical_name),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                } else if parts.len() >= 2 {
                                    let name = Config::canonical_provider_name(parts[0]);
                                    let base_url = parts[1].to_string();
                                    let api_key = if parts.len() > 2 && parts[2] != "none" { Some(parts[2].to_string()) } else { None };
                                    let model = if parts.len() > 3 { parts[3].to_string() } else { "default".to_string() };

                                    let _ = app.agent.config.set_custom_provider(&name, &base_url, api_key.as_deref(), &model);
                                    app.persist_active_provider(&name, Some(&model));
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: format!("✔ Provider kustom '{}' berhasil didaftarkan & disimpan permanen!\n• Base URL: {}\n• Model: {}\n• Status API Key: {}", name, base_url, model, if api_key.is_some() { "Tersimpan" } else { "None" }),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                } else {
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: "Format: `/custom <nama> <base_url> [api_key] [model]` atau `/custom <preset> <api_key>`".to_string(),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                }
                                app.scroll_to_bottom();
                            } else if trimmed.starts_with("/model ") {
                                let arg = trimmed.trim_start_matches("/model ").trim();
                                let parts: Vec<&str> = arg.split_whitespace().collect();
                                if parts.len() == 2 {
                                    let prov = Config::canonical_provider_name(parts[0]);
                                    let mdl = parts[1];
                                    if Config::is_builtin_provider(&prov) || app.agent.config.custom_providers.contains_key(&prov) {
                                        app.persist_active_provider(&prov, Some(mdl));
                                        app.messages.push(ChatMessageItem {
                                            sender: "System".to_string(),
                                            text: format!("✔ Provider aktif: {} | Model aktif diubah & disimpan permanen ke: `{}`", prov, mdl),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    } else {
                                        app.messages.push(ChatMessageItem {
                                            sender: "Error".to_string(),
                                            text: format!("Provider '{}' belum terdaftar. Gunakan `/custom` atau `/model` untuk melihat daftar provider.", prov),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    }
                                } else if parts.len() == 1 {
                                    let val = parts[0];
                                    let val_lower = val.to_lowercase();
                                    let canonical = Config::canonical_provider_name(val);
                                    let is_known_provider = Config::is_builtin_provider(&canonical)
                                        || app.agent.config.custom_providers.contains_key(&canonical);

                                    if is_known_provider {
                                        if app.agent.config.has_configured_provider(&canonical) {
                                            app.persist_active_provider(&canonical, None);
                                        } else {
                                            app.active_provider = canonical.clone();
                                            app.active_model = app.agent.config.get_model(&canonical);
                                            app.agent.update_provider(&canonical, None);
                                            app.api_error = Some(format!(
                                                "✖ API key '{}' belum diset. Jalankan `/config set {}_api_key <KEY>`.",
                                                canonical, canonical
                                            ));
                                        }
                                        app.messages.push(ChatMessageItem {
                                            sender: "System".to_string(),
                                            text: format!("✔ Provider aktif: {} ({})", app.active_provider, app.active_model),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    } else {
                                        let mut prov = app.active_provider.clone();
                                        if val_lower.starts_with("deepseek-") {
                                            prov = "deepseek".to_string();
                                        } else if val_lower.starts_with("gemini-") {
                                            prov = "gemini".to_string();
                                        } else if val_lower.starts_with("glm-") {
                                            prov = "z.ai".to_string();
                                        }
                                        app.persist_active_provider(&prov, Some(val));
                                        app.messages.push(ChatMessageItem {
                                            sender: "System".to_string(),
                                            text: format!("✔ Model aktif untuk provider '{}' diubah & disimpan permanen ke: `{}`", prov, val),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    }
                                }
                            } else if trimmed.starts_with("/config set ") {
                                let rest = trimmed.trim_start_matches("/config set ").trim();
                                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                                if parts.len() == 2 {
                                    let key = parts[0];
                                    let val = parts[1];
                                    if let Err(e) = app.agent.config.set_value(key, val) {
                                        app.messages.push(ChatMessageItem {
                                            sender: "Error".to_string(),
                                            text: format!("Gagal set config: {:#}", e),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    } else {
                                        let mut extra_note = String::new();
                                        if key == "default_provider" {
                                            app.persist_active_provider(val, None);
                                        } else if let Some(prefix) = key.strip_suffix("_api_key").or_else(|| key.strip_suffix("_token")) {
                                            let target_prov = Config::canonical_provider_name(prefix);
                                            if Config::is_builtin_provider(&target_prov) {
                                                app.agent.config.custom_providers.remove(&target_prov);
                                                let _ = app.agent.config.save();
                                            }
                                            if !val.trim().is_empty() && (app.active_provider == "lontar-free" || app.active_provider == target_prov) {
                                                app.persist_active_provider(&target_prov, None);
                                                extra_note = format!(" (provider '{}' otomatis diaktifkan & disimpan permanen)", target_prov);
                                            } else {
                                                let active_prov = app.active_provider.clone();
                                                app.agent.update_provider(&active_prov, None);
                                                extra_note = format!(" (tersimpan permanen; gunakan `/model {}` untuk memakai)", target_prov);
                                            }
                                        } else if key == "lontar_free_model" || key == "lontar-free_model" {
                                            if app.active_provider == "lontar-free" {
                                                app.persist_active_provider("lontar-free", Some(val));
                                            }
                                        } else if let Some(prefix) = key.strip_suffix("_model") {
                                            let target_prov = Config::canonical_provider_name(prefix);
                                            if app.active_provider == target_prov {
                                                app.persist_active_provider(&target_prov, Some(val));
                                            }
                                        } else {
                                            let active_prov = app.active_provider.clone();
                                            app.agent.update_provider(&active_prov, None);
                                        }
                                        app.api_error = None;
                                        app.messages.push(ChatMessageItem {
                                            sender: "System".to_string(),
                                            text: format!("✔ Berhasil menyetel {} = {}{}", key, val, extra_note),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    }
                                }
                            } else if trimmed == "/context list" {
                                app.refresh_docs();
                                if app.grounded_docs.is_empty() {
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: "Belum ada dokumen dalam context sesi ini.\nGunakan: `/context add <path_file>` untuk menambahkan PDF, Markdown, atau Source Code.".to_string(),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                } else {
                                    let mut list_str = format!("📚 Dokumen Grounded Aktif ({} file):\n", app.grounded_docs.len());
                                    for (i, (path, ftype, chunks)) in app.grounded_docs.iter().enumerate() {
                                        list_str.push_str(&format!("{}. `{}` ({}, {} chunks)\n", i + 1, path, ftype, chunks));
                                    }
                                    list_str.push_str("\n💡 Dokumen di atas otomatis digunakan sebagai grounded context saat Anda bertanya.");
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: list_str,
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                }
                            } else if trimmed == "/context add" || trimmed.starts_with("/context add ") || trimmed == "/context" || trimmed.starts_with("/context ") {
                                let rest = if trimmed.starts_with("/context add ") {
                                    trimmed.trim_start_matches("/context add ").trim()
                                } else if trimmed.starts_with("/context ") {
                                    let after = trimmed.trim_start_matches("/context").trim();
                                    if after.starts_with("add ") {
                                        after.trim_start_matches("add ").trim()
                                    } else if after == "add" {
                                        ""
                                    } else {
                                        after
                                    }
                                } else {
                                    ""
                                };

                                if rest.is_empty() {
                                    app.messages.push(ChatMessageItem {
                                        sender: "System".to_string(),
                                        text: "Cara menambah context dokumen (NotebookLM):\n• Perintah: `/context add <path_file> [pertanyaan]`\n• Tips: Drag & drop file PDF, Markdown, atau Source Code langsung dari Finder/Explorer ke terminal.\n• Contoh: `/context add ~/Downloads/dokumen.pdf tolong ringkas ini`".to_string(),
                                        mode_name: app.mode.name().to_string(),
                                        mode_color: app.mode.color(),
                                        model_info: format!("{} {}", app.active_model, app.active_provider),
                                    });
                                } else {
                                    let (file_path_buf, prompt_opt) = DocumentEngine::parse_path_and_prompt(rest);
                                    let path_str = file_path_buf.to_string_lossy().to_string();
                                    let file_name = file_path_buf
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| path_str.clone());

                                    if path_str.is_empty() || !file_path_buf.exists() {
                                        app.messages.push(ChatMessageItem {
                                            sender: "Error".to_string(),
                                            text: format!("File {:?} tidak ditemukan di disk. Pastikan path file benar.", file_path_buf.display()),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    } else if let Ok(db) = Database::open_default() {
                                        match DocumentEngine::ingest_file(&db, &app.session.id, &path_str) {
                                            Ok(doc) => {
                                                app.refresh_docs();
                                                app.messages.push(ChatMessageItem {
                                                    sender: "System".to_string(),
                                                    text: format!("✔ Dokumen '{}' ter-ingest ({} chunk, {} byte) ke context grounding.", file_name, doc.chunk_count, doc.file_size),
                                                    mode_name: app.mode.name().to_string(),
                                                    mode_color: app.mode.color(),
                                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                                });

                                                if let Some(user_prompt) = prompt_opt {
                                                    if !user_prompt.trim().is_empty() {
                                                        let current_mode_name = app.mode.name().to_string();
                                                        let current_mode_color = app.mode.color();
                                                        let current_model_info = format!("{} {}", app.active_model, app.active_provider);

                                                        app.messages.push(ChatMessageItem {
                                                            sender: "User".to_string(),
                                                            text: format!("[Konteks: {}] {}", file_name, user_prompt),
                                                            mode_name: current_mode_name,
                                                            mode_color: current_mode_color,
                                                            model_info: current_model_info,
                                                        });
                                                        app.scroll_to_bottom();
                                                        app.is_loading = true;
                                                        app.interrupt_armed = false;
                                                        app.armed_time = None;

                                                        active_task = Some(spawn_agent_task(
                                                            task_tx.clone(),
                                                            app.agent.clone(),
                                                            app.session.clone(),
                                                            user_prompt,
                                                            app.mode.name().to_string(),
                                                            None,
                                                        ));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                app.messages.push(ChatMessageItem {
                                                    sender: "Error".to_string(),
                                                    text: format!("Gagal ingest: {:#}", e),
                                                    mode_name: app.mode.name().to_string(),
                                                    mode_color: app.mode.color(),
                                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                                });
                                            }
                                        }
                                    }
                                }
                            } else if trimmed.starts_with("/study-guide") {
                                let topic = trimmed.trim_start_matches("/study-guide").trim();
                                let topic = if topic.is_empty() { "Ringkasan Dokumen Grounded" } else { topic };
                                let prompt = if let Ok(db) = Database::open_default() {
                                    let ctx = DocumentEngine::retrieve_context(&db, &app.session.id, topic, 6).unwrap_or_default();
                                    DocumentEngine::build_study_guide_prompt(topic, &ctx)
                                } else {
                                    format!("Buat study guide untuk: {}", topic)
                                };
                                app.messages.push(ChatMessageItem {
                                    sender: "User".to_string(),
                                    text: format!("Synthesize Study Guide untuk topik: '{}'", topic),
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                                app.scroll_to_bottom();
                                app.is_loading = true;
                                app.interrupt_armed = false;
                                app.armed_time = None;

                                active_task = Some(spawn_agent_task(
                                    task_tx.clone(),
                                    app.agent.clone(),
                                    app.session.clone(),
                                    prompt,
                                    "study".to_string(),
                                    Some("STUDY_GUIDE.md"),
                                ));
                            } else if trimmed.starts_with("/prd") {
                                let idea = trimmed.trim_start_matches("/prd").trim();
                                let idea = if idea.is_empty() { "Fitur Baru Lontar CLI" } else { idea };
                                let prompt = if let Ok(db) = Database::open_default() {
                                    let ctx = DocumentEngine::retrieve_context(&db, &app.session.id, idea, 6).unwrap_or_default();
                                    DocumentEngine::build_prd_prompt(idea, &ctx)
                                } else {
                                    format!("Buat PRD untuk: {}", idea)
                                };
                                app.messages.push(ChatMessageItem {
                                    sender: "User".to_string(),
                                    text: format!("Synthesize PRD untuk: '{}'", idea),
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                                app.scroll_to_bottom();
                                app.is_loading = true;
                                app.interrupt_armed = false;
                                app.armed_time = None;

                                active_task = Some(spawn_agent_task(
                                    task_tx.clone(),
                                    app.agent.clone(),
                                    app.session.clone(),
                                    prompt,
                                    "plan".to_string(),
                                    Some("PRD.md"),
                                ));
                            } else if trimmed.starts_with("/tasklist") {
                                let source = trimmed.trim_start_matches("/tasklist").trim();
                                let content = if Path::new(source).exists() {
                                    fs::read_to_string(source).unwrap_or_else(|_| source.to_string())
                                } else if Path::new("PRD.md").exists() {
                                    fs::read_to_string("PRD.md").unwrap_or_else(|_| source.to_string())
                                } else {
                                    source.to_string()
                                };
                                let prompt = DocumentEngine::build_tasklist_prompt(&content);
                                app.messages.push(ChatMessageItem {
                                    sender: "User".to_string(),
                                    text: "Synthesize Technical Tasklist".to_string(),
                                    mode_name: app.mode.name().to_string(),
                                    mode_color: app.mode.color(),
                                    model_info: format!("{} {}", app.active_model, app.active_provider),
                                });
                                app.scroll_to_bottom();
                                app.is_loading = true;
                                app.interrupt_armed = false;
                                app.armed_time = None;

                                active_task = Some(spawn_agent_task(
                                    task_tx.clone(),
                                    app.agent.clone(),
                                    app.session.clone(),
                                    prompt,
                                    "plan".to_string(),
                                    Some("TASKLIST.md"),
                                ));
                            } else if trimmed == "/diff" {
                                match Command::new("git").args(["diff", "--stat"]).output() {
                                    Ok(out) => {
                                        let diff_stat = String::from_utf8_lossy(&out.stdout).to_string();
                                        if diff_stat.trim().is_empty() {
                                            app.messages.push(ChatMessageItem {
                                                sender: "System".to_string(),
                                                text: "Tidak ada perubahan git diff.".to_string(),
                                                mode_name: app.mode.name().to_string(),
                                                mode_color: app.mode.color(),
                                                model_info: format!("{} {}", app.active_model, app.active_provider),
                                            });
                                        } else {
                                            app.messages.push(ChatMessageItem {
                                                sender: "System".to_string(),
                                                text: format!("Status Git Diff:\n```\n{}\n```", diff_stat.trim()),
                                                mode_name: app.mode.name().to_string(),
                                                mode_color: app.mode.color(),
                                                model_info: format!("{} {}", app.active_model, app.active_provider),
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        app.messages.push(ChatMessageItem {
                                            sender: "Error".to_string(),
                                            text: format!("Gagal menjalankan git diff: {:#}", e),
                                            mode_name: app.mode.name().to_string(),
                                            mode_color: app.mode.color(),
                                            model_info: format!("{} {}", app.active_model, app.active_provider),
                                        });
                                    }
                                }
                            } else {
                                // Normal User Message (Preserves exact mode for this prompt)
                                let current_mode_name = app.mode.name().to_string();
                                let current_mode_color = app.mode.color();
                                let current_model_info = format!("{} {}", app.active_model, app.active_provider);

                                app.messages.push(ChatMessageItem {
                                    sender: "User".to_string(),
                                    text: trimmed.clone(),
                                    mode_name: current_mode_name,
                                    mode_color: current_mode_color,
                                    model_info: current_model_info,
                                });
                                app.scroll_to_bottom();
                                app.is_loading = true;
                                app.interrupt_armed = false;
                                app.armed_time = None;

                                active_task = Some(spawn_agent_task(
                                    task_tx.clone(),
                                    app.agent.clone(),
                                    app.session.clone(),
                                    trimmed,
                                    app.mode.name().to_string(),
                                    None,
                                ));
                            }
                        }
                    }
                    (_, KeyCode::Char(c)) => {
                        if let Some(ch) = resolve_char_with_shift(KeyCode::Char(c), key.modifiers) {
                            app.input.insert(app.cursor_position, ch);
                            app.cursor_position += 1;
                            app.slash_selected_index = 0;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

        if app.should_quit {
            if let Some(h) = active_task.take() {
                h.abort();
            }
            break;
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &mut TuiApp) {
    let size = f.area();
    let is_home = app.messages.is_empty();

    if is_home {
        render_home_view(f, app, size);
    } else {
        render_active_view(f, app, size);
    }

    // Modal Overlays
    if app.show_commands_modal {
        render_commands_modal(f, app, size);
    }

    if app.show_models_modal {
        render_models_modal(f, app, size);
    }

    if app.show_sessions_modal {
        render_sessions_modal(f, app, size);
    }

    if app.show_sources_modal {
        render_sources_modal(f, app, size);
    }

    if app.show_connect_modal {
        render_connect_modal(f, app, size);
    }
}

// ── 1. Pure OpenCode Home View (Prompt at Bottom, Pixel Logo in Center) ──────
fn render_home_view(f: &mut Frame, app: &TuiApp, area: Rect) {
    let has_error = app.api_error.is_some();
    let error_height = if has_error { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),       // Top spacer
            Constraint::Length(11),           // Centered Lontar Symbol + Wordmark
            Constraint::Min(2),               // Mid spacer
            Constraint::Length(4),            // Floating Input Card
            Constraint::Length(1),            // Keybinds: tab mode   ctrl+p commands   ctrl+t sources
            Constraint::Length(error_height), // Error notice if present
            Constraint::Length(1),            // Centered Tip
            Constraint::Length(1),            // Bottom Bar: ~/cwd ... 0.2.0
        ])
        .split(area);

    // 1A. Centered Logo: Lontar Manuscript Chevron + Pixel Typography (Point 9)
    let logo_lines = vec![
        Line::from(vec![
            Span::styled("       ██        ▄██▄         ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("       ██          ▀██▄       ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("       ██            ▀██▄     ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("       ██              ███    ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("       ██            ▄██▀     ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("     ▄▄██████      ▄██▀    ▄█▄", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("     ▀▀▀▀▀▀▀▀    ▀▀▀▀      ▀▀▀", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("█    █▀▀█ █▀▀▄ ▀▀█▀▀ █▀▀█ █▀▀▄", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("█    █  █ █  █   █   █▄▄█ █▄▄▀", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("▀▀▀▀ ▀▀▀▀ ▀  ▀   ▀   ▀  ▀ ▀  ▀", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
    ];
    let logo_para = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo_para, chunks[1]);

    // 1B. Floating Input Card
    let card_width = 72.min(area.width.saturating_sub(4));
    let card_x = (area.width.saturating_sub(card_width)) / 2;
    let card_area = Rect {
        x: area.x + card_x,
        y: chunks[3].y,
        width: card_width,
        height: 4,
    };

    render_input_card(f, app, card_area);
    if !app.show_commands_modal
        && !app.show_models_modal
        && !app.show_sessions_modal
        && !app.show_sources_modal
        && !app.show_connect_modal
    {
        render_slash_suggestions(f, app, card_area);
    }

    // 1C. Under Card Keybinds (Or Knight Rider Scanner if loading)
    let keybinds_area = Rect {
        x: card_area.x,
        y: chunks[4].y,
        width: card_width,
        height: 1,
    };

    if app.is_loading {
        let mut left_spans = render_scanner_spans(app.spinner_frame, app.mode.color());
        left_spans.extend(render_interrupt_status_spans(app.interrupt_armed));
        f.render_widget(Paragraph::new(Line::from(left_spans)).alignment(Alignment::Left), keybinds_area);
    } else {
        let keybinds_line = Line::from(vec![
            Span::styled("tab ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled(format!("mode ({})  ", app.mode.name()), Style::default().fg(app.mode.color())),
            Span::styled("ctrl+p ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("commands  ", Style::default().fg(TEXT_DIM)),
            Span::styled("ctrl+t ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("sources", Style::default().fg(TEXT_DIM)),
        ]);
        f.render_widget(Paragraph::new(keybinds_line).alignment(Alignment::Right), keybinds_area);
    }

    // 1D. Error Line (If API disconnected)
    if has_error {
        if let Some(err_msg) = &app.api_error {
            let err_line = Line::from(vec![
                Span::styled(err_msg, Style::default().fg(ERROR_RED)),
            ]);
            f.render_widget(Paragraph::new(err_line).alignment(Alignment::Center), chunks[5]);
        }
    }

    // 1E. Centered Dynamic Tip Line
    let tip_line = Line::from(vec![
        Span::styled("● Tip ", Style::default().fg(ACCENT_ORANGE)),
        Span::styled(app.current_tip(), Style::default().fg(TEXT_DIM)),
    ]);
    f.render_widget(Paragraph::new(tip_line).alignment(Alignment::Center), chunks[6]);

    // 1F. Bottom Bar: `~` on left, `0.2.0` on right
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[7]);

    let cwd = TuiApp::cwd_name();
    f.render_widget(Paragraph::new(Line::from(Span::styled(format!("~/{}", cwd), Style::default().fg(TEXT_DARK)))), bottom_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(TEXT_DARK)))).alignment(Alignment::Right), bottom_chunks[1]);
}

// ── 2. Active View (Chat Stream matching OpenCode Image 5 & 6) ──────────────
fn render_active_view(f: &mut Frame, app: &mut TuiApp, area: Rect) {
    let has_error = app.api_error.is_some();
    let error_height = if has_error { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),            // Top Minimal Bar
            Constraint::Min(6),               // Chat Messages
            Constraint::Length(4),            // Floating Input Card
            Constraint::Length(1),            // Keybinds
            Constraint::Length(error_height), // Optional Error Notice
            Constraint::Length(1),            // Bottom Bar
        ])
        .split(area);

    // 2A. Minimal Top Bar
    let cwd = TuiApp::cwd_name();
    let branch = TuiApp::git_branch().unwrap_or_else(|| "main".to_string());

    let header_line = Line::from(vec![
        Span::styled(" lontar ", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(format!("~/{} ", cwd), Style::default().fg(TEXT_DIM)),
        Span::styled(format!("({}) ", branch), Style::default().fg(TEXT_DARK)),
        Span::raw("  "),
        Span::styled(format!("● Mode: {}", app.mode.name()), Style::default().fg(app.mode.color())),
    ]);

    let right_header = Line::from(vec![
        Span::styled(format!("{} {} ", app.active_model, app.active_provider), Style::default().fg(TEXT_MUTED)),
    ]);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[0]);

    f.render_widget(Paragraph::new(header_line), header_chunks[0]);
    f.render_widget(Paragraph::new(right_header).alignment(Alignment::Right), header_chunks[1]);

    // 2B. Unified Layout for Chat and Input Card (Matching OpenCode Image 5 & 6)
    let content_width = area.width.saturating_sub(4).min(92);
    let content_x = (area.width.saturating_sub(content_width)) / 2;

    let chat_area = Rect {
        x: area.x + content_x,
        y: chunks[1].y,
        width: content_width,
        height: chunks[1].height,
    };

    render_chat_stream(f, app, chat_area);

    // 2C. Floating Bottom Input Card
    let card_area = Rect {
        x: area.x + content_x,
        y: chunks[2].y,
        width: content_width,
        height: 4,
    };

    render_input_card(f, app, card_area);
    if !app.show_commands_modal
        && !app.show_models_modal
        && !app.show_sessions_modal
        && !app.show_sources_modal
        && !app.show_connect_modal
    {
        render_slash_suggestions(f, app, card_area);
    }

    // 2D. Shortcuts below Input Card (Knight Rider scanner during loading matching OpenCode)
    let shortcuts_area = Rect {
        x: card_area.x,
        y: chunks[3].y,
        width: content_width,
        height: 1,
    };

    if app.is_loading {
        let mut left_spans = render_scanner_spans(app.spinner_frame, app.mode.color());
        left_spans.extend(render_interrupt_status_spans(app.interrupt_armed));
        let left_para = Paragraph::new(Line::from(left_spans)).alignment(Alignment::Left);
        f.render_widget(left_para, shortcuts_area);

        let right_spans = vec![
            Span::styled("ctrl+c ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("quit", Style::default().fg(TEXT_DIM)),
        ];
        let right_para = Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right);
        f.render_widget(right_para, shortcuts_area);
    } else {
        let shortcuts_line = Line::from(vec![
            Span::styled("tab ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled(format!("mode ({})  ", app.mode.name()), Style::default().fg(app.mode.color())),
            Span::styled("ctrl+p ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("commands  ", Style::default().fg(TEXT_DIM)),
            Span::styled("ctrl+t ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("sources  ", Style::default().fg(TEXT_DIM)),
            Span::styled("ctrl+c ", Style::default().fg(TEXT_PRIMARY)),
            Span::styled("quit", Style::default().fg(TEXT_DIM)),
        ]);
        f.render_widget(Paragraph::new(shortcuts_line).alignment(Alignment::Right), shortcuts_area);
    }

    // 2E. Error Notice if present
    if has_error {
        if let Some(err_msg) = &app.api_error {
            let err_line = Line::from(vec![
                Span::styled(err_msg, Style::default().fg(ERROR_RED)),
            ]);
            f.render_widget(Paragraph::new(err_line).alignment(Alignment::Center), chunks[4]);
        }
    }

    // 2F. Bottom Bar
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[5]);

    f.render_widget(Paragraph::new(Line::from(Span::styled(format!("~/{}", cwd), Style::default().fg(TEXT_DARK)))), bottom_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(TEXT_DARK)))).alignment(Alignment::Right), bottom_chunks[1]);
}

// ── Floating Slash Command Suggestions Palette ───────────────────────────────
fn render_slash_suggestions(f: &mut Frame, app: &TuiApp, card_area: Rect) {
    let matches = app.get_matching_slash_commands();
    if matches.is_empty() {
        return;
    }

    let max_visible = 6.min(matches.len());
    let popup_height = (max_visible + 2) as u16;
    if card_area.y < popup_height {
        return;
    }

    let popup_y = card_area.y - popup_height;
    let popup_width = 72.min(card_area.width);
    let popup_rect = Rect {
        x: card_area.x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .title(Span::styled(" Commands (↑/↓ pilih · Tab/Enter lengkapi · Esc batal) ", Style::default().fg(TEXT_DIM)))
        .style(Style::default().bg(BG_MODAL));

    let inner = block.inner(popup_rect);
    f.render_widget(block, popup_rect);

    let sel = app.slash_selected_index.min(matches.len().saturating_sub(1));
    let start_idx = if sel >= max_visible {
        sel + 1 - max_visible
    } else {
        0
    };
    let end_idx = (start_idx + max_visible).min(matches.len());

    let mut lines = Vec::new();
    for (i, cmd) in matches[start_idx..end_idx].iter().enumerate() {
        let actual_idx = start_idx + i;
        let is_selected = actual_idx == sel;

        if is_selected {
            lines.push(Line::from(vec![
                Span::styled(" ▶ ", Style::default().fg(app.mode.color()).bg(BG_CARD).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:<15}", cmd.name), Style::default().fg(Color::White).bg(BG_CARD).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}", cmd.description), Style::default().fg(TEXT_PRIMARY).bg(BG_CARD)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default().bg(BG_MODAL)),
                Span::styled(format!("{:<15}", cmd.name), Style::default().fg(app.mode.color()).bg(BG_MODAL)),
                Span::styled(format!(" {}", cmd.description), Style::default().fg(TEXT_MUTED).bg(BG_MODAL)),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Shared Input Card with Mode Colors & Blinking Inverted Cursor ────────────
fn render_input_card(f: &mut Frame, app: &TuiApp, card_area: Rect) {
    let mode_color = app.mode.color();

    // Clear card area so chat background cannot bleed through
    f.render_widget(Clear, card_area);

    // Solid dark card background
    let card_bg = Block::default().style(Style::default().bg(BG_CARD));
    f.render_widget(card_bg, card_area);

    // Blinking cursor calculation (530ms cycle)
    let blink_on = (app.start_time.elapsed().as_millis() / 530) % 2 == 0;

    let (line_1, cursor_x) = if app.input.is_empty() {
        let placeholder = format!("Ask anything... \"{}\"", app.current_placeholder());
        let mut chars = placeholder.chars();
        let first_char = chars.next().unwrap_or('A');
        let rest: String = chars.collect();

        let first_char_span = if blink_on {
            Span::styled(
                first_char.to_string(),
                Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                first_char.to_string(),
                Style::default().fg(TEXT_DIM).bg(BG_CARD),
            )
        };

        let spans = vec![
            Span::styled("▌ ", Style::default().fg(mode_color).bg(BG_CARD)),
            Span::styled(" ", Style::default().bg(BG_CARD)),
            first_char_span,
            Span::styled(rest, Style::default().fg(TEXT_DIM).bg(BG_CARD)),
        ];

        (Line::from(spans), card_area.x + 3)
    } else {
        let display_input = if app.input.contains('\n') {
            app.input.replace('\n', " ↵ ")
        } else {
            app.input.clone()
        };

        let chars: Vec<char> = display_input.chars().collect();
        let cur_pos = app.cursor_position.min(chars.len());
        let total_chars = chars.len();

        // Horizontal scrolling viewport calculation
        // card_area.x + 3 is the prefix "▌  " (width 3).
        // Right side needs at least 3 cells padding.
        let avail_width = (card_area.width as usize).saturating_sub(6).max(10);

        let (view_start, view_end) = if total_chars <= avail_width {
            (0, total_chars)
        } else {
            let margin = 4.min(avail_width / 4);
            let start = if cur_pos < avail_width.saturating_sub(margin) {
                0
            } else {
                (cur_pos + margin + 1)
                    .saturating_sub(avail_width)
                    .min(total_chars.saturating_sub(avail_width))
            };
            let end = (start + avail_width).min(total_chars);
            (start, end)
        };

        let mut spans = vec![
            Span::styled("▌ ", Style::default().fg(mode_color).bg(BG_CARD)),
        ];

        // Left overflow indicator
        if view_start > 0 {
            spans.push(Span::styled("‹", Style::default().fg(TEXT_MUTED).bg(BG_CARD)));
        } else {
            spans.push(Span::styled(" ", Style::default().bg(BG_CARD)));
        }

        let slice_chars = &chars[view_start..view_end];
        let slice_cur_pos = if cur_pos >= view_start && cur_pos <= view_end {
            cur_pos - view_start
        } else if cur_pos < view_start {
            0
        } else {
            slice_chars.len()
        };

        let before: String = slice_chars[..slice_cur_pos.min(slice_chars.len())].iter().collect();
        if !before.is_empty() {
            spans.push(Span::styled(before, Style::default().fg(TEXT_PRIMARY).bg(BG_CARD)));
        }

        if slice_cur_pos < slice_chars.len() {
            let under_cursor = slice_chars[slice_cur_pos].to_string();
            let cursor_span = if blink_on {
                Span::styled(
                    under_cursor,
                    Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(under_cursor, Style::default().fg(TEXT_PRIMARY).bg(BG_CARD))
            };
            spans.push(cursor_span);
            let after: String = slice_chars[slice_cur_pos + 1..].iter().collect();
            if !after.is_empty() {
                spans.push(Span::styled(after, Style::default().fg(TEXT_PRIMARY).bg(BG_CARD)));
            }
        } else {
            let cursor_span = if blink_on {
                Span::styled("█", Style::default().fg(Color::White).bg(BG_CARD))
            } else {
                Span::styled(" ", Style::default().bg(BG_CARD))
            };
            spans.push(cursor_span);
        }

        // Right overflow indicator
        if view_end < total_chars {
            spans.push(Span::styled("›", Style::default().fg(TEXT_MUTED).bg(BG_CARD)));
        }

        let cx = card_area.x + 3 + (slice_cur_pos as u16);
        (Line::from(spans), cx)
    };

    let line_2 = Line::from(vec![
        Span::styled("▌ ", Style::default().fg(mode_color).bg(BG_CARD)),
        Span::styled(" ", Style::default().bg(BG_CARD)),
        Span::styled(app.mode.name(), Style::default().fg(mode_color).bg(BG_CARD).add_modifier(Modifier::BOLD)),
        Span::styled(" · ", Style::default().fg(TEXT_DARK).bg(BG_CARD)),
        Span::styled(format!("{} {}", app.active_model, app.active_provider), Style::default().fg(TEXT_MUTED).bg(BG_CARD)),
    ]);

    let card_lines = vec![
        Line::from(Span::styled(" ", Style::default().bg(BG_CARD))),
        line_1,
        line_2,
        Line::from(Span::styled(" ", Style::default().bg(BG_CARD))),
    ];

    f.render_widget(Paragraph::new(card_lines).style(Style::default().bg(BG_CARD)), card_area);
    f.set_cursor_position((cursor_x, card_area.y + 1));
}

// ── 3. Centered Command Palette Modal (Point 3 - Neatly Aligned & Scaled) ───
fn render_commands_modal(f: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 58.min(area.width.saturating_sub(4));
    let modal_height = 20.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_rect = Rect {
        x: area.x + modal_x,
        y: area.y + modal_y,
        width: modal_width,
        height: modal_height,
    };

    f.render_widget(Clear, modal_rect);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .style(Style::default().bg(BG_MODAL));

    let inner = modal_block.inner(modal_rect);
    f.render_widget(modal_block, modal_rect);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Search
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Scrollable List
        ])
        .split(inner);

    // Header: Commands | esc
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner_chunks[0]);

    f.render_widget(Paragraph::new(Line::from(Span::styled("Commands", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)))), header_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled("esc", Style::default().fg(TEXT_DIM)))).alignment(Alignment::Right), header_chunks[1]);

    // Search bar with cursor
    let search_line = if app.command_search.is_empty() {
        Line::from(vec![
            Span::styled("S", Style::default().fg(Color::Black).bg(HIGHLIGHT_PEACH).add_modifier(Modifier::BOLD)),
            Span::styled("earch", Style::default().fg(TEXT_DIM)),
        ])
    } else {
        Line::from(Span::styled(&app.command_search, Style::default().fg(TEXT_PRIMARY)))
    };
    f.render_widget(Paragraph::new(search_line), inner_chunks[2]);

    // Scrollable items
    let filtered = app.get_filtered_palette();
    let visible_window_size = 7;
    let start_idx = app.palette_scroll_offset;
    let end_idx = (start_idx + visible_window_size).min(filtered.len());

    let mut list_items = Vec::new();
    let mut current_group = "";

    for (idx, item) in filtered.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
        if item.group != current_group {
            current_group = item.group;
            list_items.push(ListItem::new(Line::from(Span::styled(current_group, Style::default().fg(ACCENT_PURPLE)))));
        }

        let is_selected = idx == app.selected_command_index;
        let item_width = inner_chunks[4].width as usize;
        let label = item.name;
        let shortcut = item.shortcut;
        let pad = item_width.saturating_sub(label.len() + shortcut.len() + 3);

        if is_selected {
            let full_row = format!(" {:<label_w$}{:>shortcut_w$} ", label, shortcut, label_w = item_width.saturating_sub(shortcut.len() + 3), shortcut_w = shortcut.len());
            list_items.push(ListItem::new(Line::from(Span::styled(
                full_row,
                Style::default().bg(HIGHLIGHT_PEACH).fg(Color::Black).add_modifier(Modifier::BOLD),
            ))));
        } else {
            list_items.push(ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(label, Style::default().fg(TEXT_PRIMARY)),
                Span::raw(" ".repeat(pad)),
                Span::styled(shortcut, Style::default().fg(TEXT_DIM)),
                Span::raw(" "),
            ])));
        }
    }

    f.render_widget(List::new(list_items), inner_chunks[4]);
}

// ── 4. Dedicated Select Model Modal (Point 4 - Image 4 Replica) ─────────────
fn render_models_modal(f: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 66.min(area.width.saturating_sub(4));
    let modal_height = 21.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_rect = Rect {
        x: area.x + modal_x,
        y: area.y + modal_y,
        width: modal_width,
        height: modal_height,
    };

    f.render_widget(Clear, modal_rect);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .style(Style::default().bg(BG_MODAL));

    let inner = modal_block.inner(modal_rect);
    f.render_widget(modal_block, modal_rect);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Search
            Constraint::Length(1), // Spacer
            Constraint::Min(8),    // Models list
            Constraint::Length(1), // Footer actions
        ])
        .split(inner);

    // Header: Select model | esc
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner_chunks[0]);

    f.render_widget(Paragraph::new(Line::from(Span::styled("Select model", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)))), header_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled("esc", Style::default().fg(TEXT_DIM)))).alignment(Alignment::Right), header_chunks[1]);

    // Search bar with cursor
    let search_line = if app.model_search.is_empty() {
        Line::from(vec![
            Span::styled("S", Style::default().fg(Color::Black).bg(HIGHLIGHT_PEACH).add_modifier(Modifier::BOLD)),
            Span::styled("earch", Style::default().fg(TEXT_DIM)),
        ])
    } else {
        Line::from(Span::styled(&app.model_search, Style::default().fg(TEXT_PRIMARY)))
    };
    f.render_widget(Paragraph::new(search_line), inner_chunks[2]);

    // Scrollable models
    let filtered = app.get_filtered_models();
    let visible_window_size = 8;
    let start_idx = app.model_scroll_offset;
    let end_idx = (start_idx + visible_window_size).min(filtered.len());

    let mut list_items = Vec::new();
    let mut current_group = "";

    for (idx, item) in filtered.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
        if item.group.as_str() != current_group {
            current_group = item.group.as_str();
            list_items.push(ListItem::new(Line::from(Span::styled(current_group, Style::default().fg(ACCENT_PURPLE)))));
        }

        let is_selected = idx == app.selected_model_index;
        let item_width = inner_chunks[4].width as usize;
        let tag = item.tag.as_str();
        let tag_chars = tag.chars().count();
        let max_label_w = item_width.saturating_sub(tag_chars + 4);
        let name_label_full = format!("{} {}", item.name, item.provider_id);
        let name_label_chars = name_label_full.chars().count();
        let name_label = if name_label_chars > max_label_w && max_label_w > 1 {
            let mut s: String = name_label_full.chars().take(max_label_w - 1).collect();
            s.push('…');
            s
        } else {
            name_label_full
        };
        let label_chars = name_label.chars().count();
        let pad = max_label_w.saturating_sub(label_chars) + 1;
        let tag_color = if tag == "● Active" {
            COLOR_STUDY
        } else {
            TEXT_DIM
        };

        if is_selected {
            let row = format!(" {}{}{} ", name_label, " ".repeat(pad), tag);
            list_items.push(ListItem::new(Line::from(Span::styled(
                row,
                Style::default().bg(HIGHLIGHT_PEACH).fg(Color::Black).add_modifier(Modifier::BOLD),
            ))));
        } else {
            let mut line_spans = vec![Span::raw(" ")];
            if name_label_chars <= max_label_w || max_label_w <= 1 {
                line_spans.push(Span::styled(&item.name, Style::default().fg(TEXT_PRIMARY)));
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(&item.provider_id, Style::default().fg(TEXT_DIM)));
            } else {
                let primary_len = item.name.chars().count().min(label_chars);
                let primary_part: String = name_label.chars().take(primary_len).collect();
                let rest_part: String = name_label.chars().skip(primary_len).collect();
                line_spans.push(Span::styled(primary_part, Style::default().fg(TEXT_PRIMARY)));
                line_spans.push(Span::styled(rest_part, Style::default().fg(TEXT_DIM)));
            }
            line_spans.push(Span::raw(" ".repeat(pad)));
            line_spans.push(Span::styled(tag, Style::default().fg(tag_color)));
            line_spans.push(Span::raw(" "));
            list_items.push(ListItem::new(Line::from(line_spans)));
        }
    }

    f.render_widget(List::new(list_items), inner_chunks[4]);

    // Footer actions (Image 4)
    let footer_line = Line::from(vec![
        Span::styled("Connect ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("ctrl+a   ", Style::default().fg(TEXT_DIM)),
        Span::styled("Sync live API ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("ctrl+r   ", Style::default().fg(TEXT_DIM)),
        Span::styled("Favorite ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("ctrl+f", Style::default().fg(TEXT_DIM)),
    ]);
    f.render_widget(Paragraph::new(footer_line), inner_chunks[5]);
}

// ── 5. Dedicated Sessions Modal (Point 5 - Image 5 Replica) ─────────────────
fn render_sessions_modal(f: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 70.min(area.width.saturating_sub(4));
    let modal_height = 21.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_rect = Rect {
        x: area.x + modal_x,
        y: area.y + modal_y,
        width: modal_width,
        height: modal_height,
    };

    f.render_widget(Clear, modal_rect);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .style(Style::default().bg(BG_MODAL));

    let inner = modal_block.inner(modal_rect);
    f.render_widget(modal_block, modal_rect);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Search
            Constraint::Length(1), // Spacer
            Constraint::Min(8),    // Sessions list
            Constraint::Length(1), // Footer actions
        ])
        .split(inner);

    // Header: Sessions | esc
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner_chunks[0]);

    f.render_widget(Paragraph::new(Line::from(Span::styled("Sessions", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)))), header_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled("esc", Style::default().fg(TEXT_DIM)))).alignment(Alignment::Right), header_chunks[1]);

    // Search bar
    let search_line = if app.session_search.is_empty() {
        Line::from(vec![
            Span::styled("S", Style::default().fg(Color::Black).bg(HIGHLIGHT_PEACH).add_modifier(Modifier::BOLD)),
            Span::styled("earch", Style::default().fg(TEXT_DIM)),
        ])
    } else {
        Line::from(Span::styled(&app.session_search, Style::default().fg(TEXT_PRIMARY)))
    };
    f.render_widget(Paragraph::new(search_line), inner_chunks[2]);

    // Scrollable sessions
    let filtered = app.get_filtered_sessions();
    let visible_window_size = 8;
    let start_idx = app.session_scroll_offset;
    let end_idx = (start_idx + visible_window_size).min(filtered.len());

    let mut list_items = Vec::new();
    let now = chrono::Utc::now();

    if filtered.is_empty() {
        list_items.push(ListItem::new(Line::from(Span::styled("Tidak ada sesi yang cocok.", Style::default().fg(TEXT_DIM)))));
    } else {
        list_items.push(ListItem::new(Line::from(Span::styled("Today", Style::default().fg(ACCENT_PURPLE)))));

        for (idx, sess) in filtered.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
            let is_selected = idx == app.selected_session_index;
            let is_active = sess.id == app.session.id;
            let bullet = if is_active { "● " } else { "  " };

            let time_str = if sess.updated_at.date_naive() == now.date_naive() {
                sess.updated_at.format("%H:%M").to_string()
            } else {
                sess.updated_at.format("%Y-%m-%d").to_string()
            };

            let label = format!("{}{}", bullet, sess.name);
            let item_width = inner_chunks[4].width as usize;
            let right_meta = format!("{} ({} msg)", time_str, sess.message_count);
            let pad = item_width.saturating_sub(label.len() + right_meta.len() + 3);

            if is_selected {
                let row = format!(" {:<label_w$}{:>meta_w$} ", label, right_meta, label_w = item_width.saturating_sub(right_meta.len() + 3), meta_w = right_meta.len());
                list_items.push(ListItem::new(Line::from(Span::styled(
                    row,
                    Style::default().bg(HIGHLIGHT_PEACH).fg(Color::Black).add_modifier(Modifier::BOLD),
                ))));
            } else {
                list_items.push(ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(bullet, Style::default().fg(COLOR_BUILD)),
                    Span::styled(&sess.name, Style::default().fg(TEXT_PRIMARY)),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(right_meta, Style::default().fg(TEXT_DIM)),
                    Span::raw(" "),
                ])));
            }
        }
    }

    f.render_widget(List::new(list_items), inner_chunks[4]);

    // Footer actions (Image 5)
    let footer_line = Line::from(vec![
        Span::styled("new session ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("ctrl+n    ", Style::default().fg(TEXT_DIM)),
        Span::styled("delete ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("ctrl+d    ", Style::default().fg(TEXT_DIM)),
        Span::styled("select ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("enter", Style::default().fg(TEXT_DIM)),
    ]);
    f.render_widget(Paragraph::new(footer_line), inner_chunks[5]);
}

// ── 6. Sources & Tasks Modal (Ctrl+T) ───────────────────────────────────────
fn render_sources_modal(f: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 64.min(area.width.saturating_sub(4));
    let modal_height = 20.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_rect = Rect {
        x: area.x + modal_x,
        y: area.y + modal_y,
        width: modal_width,
        height: modal_height,
    };

    f.render_widget(Clear, modal_rect);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .style(Style::default().bg(BG_MODAL));

    let inner = modal_block.inner(modal_rect);
    f.render_widget(modal_block, modal_rect);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(1), // Spacer
            Constraint::Length(7), // Grounded Sources
            Constraint::Length(1), // Divider
            Constraint::Min(6),    // Tasks
        ])
        .split(inner);

    // Header
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(inner_chunks[0]);

    f.render_widget(Paragraph::new(Line::from(Span::styled("Grounded Sources & Tasklist", Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)))), header_chunks[0]);
    f.render_widget(Paragraph::new(Line::from(Span::styled("esc / ctrl+t", Style::default().fg(TEXT_DIM)))).alignment(Alignment::Right), header_chunks[1]);

    // Grounded Documents List
    let doc_items: Vec<ListItem> = if app.grounded_docs.is_empty() {
        vec![
            ListItem::new(Line::from(Span::styled("Belum ada dokumen yang di-ingest.", Style::default().fg(TEXT_DIM)))),
            ListItem::new(Line::from(Span::styled("Gunakan: /context add <path_file>", Style::default().fg(COLOR_STUDY)))),
        ]
    } else {
        app.grounded_docs.iter().map(|(path, _ftype, chunks)| {
            let name = Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
            ListItem::new(Line::from(vec![
                Span::styled("📄 ", Style::default().fg(TEXT_DIM)),
                Span::styled(name, Style::default().fg(TEXT_PRIMARY)),
                Span::styled(format!(" ({} chunks)", chunks), Style::default().fg(TEXT_DIM)),
            ]))
        }).collect()
    };
    f.render_widget(List::new(doc_items).block(Block::default().title(" 📚 NotebookLM Documents ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(BORDER_SUBTLE))), inner_chunks[2]);

    // Task Checklist
    let task_items: Vec<ListItem> = app.tasklist.iter().map(|(done, text)| {
        let icon = if *done { "✔ " } else { "○ " };
        let style = if *done { Style::default().fg(COLOR_STUDY) } else { Style::default().fg(TEXT_DIM) };
        ListItem::new(Line::from(vec![
            Span::styled(icon, style),
            Span::styled(text.clone(), if *done { Style::default().fg(TEXT_DIM) } else { Style::default().fg(TEXT_PRIMARY) }),
        ]))
    }).collect();
    f.render_widget(List::new(task_items).block(Block::default().title(" 📋 Kiro Task Roadmap ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(BORDER_SUBTLE))), inner_chunks[4]);
}

// ── 6B. Connect Provider Modal (OpenCode DialogIntegration parity) ─────────
fn render_connect_modal(f: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 62.min(area.width.saturating_sub(4));
    let modal_height = 10.min(area.height.saturating_sub(2));
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_rect = Rect {
        x: area.x + modal_x,
        y: area.y + modal_y,
        width: modal_width,
        height: modal_height,
    };

    f.render_widget(Clear, modal_rect);

    let modal_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_MODAL))
        .style(Style::default().bg(BG_MODAL));

    let inner = modal_block.inner(modal_rect);
    f.render_widget(modal_block, modal_rect);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Subtitle
            Constraint::Length(3), // Input box
            Constraint::Length(1), // Hint
            Constraint::Length(1), // Footer actions
        ])
        .split(inner);

    // Title: Connect <Provider>
    let title = Line::from(vec![
        Span::styled(format!("Connect Provider: {}", app.connect_provider_id.to_uppercase()), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(title), inner_chunks[0]);

    // Subtitle
    let model_display = if !app.connect_model_id.is_empty() {
        format!("Model: {} • 1 API Key berlaku untuk semua model {}", app.connect_model_id, app.connect_provider_id)
    } else {
        format!("1 API Key berlaku untuk seluruh model provider {}", app.connect_provider_id)
    };
    let subtitle = Line::from(vec![
        Span::styled(model_display, Style::default().fg(COLOR_STUDY)),
    ]);
    f.render_widget(Paragraph::new(subtitle), inner_chunks[1]);

    // Input Box (Card style)
    let masked_len = app.connect_api_key_input.len();
    let display_key = if masked_len == 0 {
        "Paste or type API Key (sk-...)...".to_string()
    } else if masked_len <= 8 {
        "*".repeat(masked_len)
    } else {
        format!("{}...{}", &app.connect_api_key_input[..4], "*".repeat(masked_len.saturating_sub(4)))
    };

    let input_para = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(display_key, if masked_len == 0 { Style::default().fg(TEXT_DARK) } else { Style::default().fg(COLOR_BUILD) }),
    ]))
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(BORDER_SUBTLE)).style(Style::default().bg(BG_CARD)));
    f.render_widget(input_para, inner_chunks[2]);

    // Hint / Tip
    let hint_line = Line::from(vec![
        Span::styled("💡 Tip: ", Style::default().fg(HIGHLIGHT_PEACH)),
        Span::styled("Gunakan /models fetch untuk auto-detect model terbaru dari API", Style::default().fg(TEXT_DIM)),
    ]);
    f.render_widget(Paragraph::new(hint_line), inner_chunks[3]);

    // Footer actions
    let footer_line = Line::from(vec![
        Span::styled("enter ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("connect    ", Style::default().fg(TEXT_DIM)),
        Span::styled("esc ", Style::default().fg(TEXT_PRIMARY)),
        Span::styled("cancel", Style::default().fg(TEXT_DIM)),
    ]);
    f.render_widget(Paragraph::new(footer_line).alignment(Alignment::Right), inner_chunks[4]);
}

// ── 7. Chat Stream & Markdown Formatting (OpenCode Parity) ───────────────────
fn visible_len(s: &str) -> usize {
    s.replace("**", "")
        .replace("__", "")
        .replace("`", "")
        .replace("~~", "")
        .chars()
        .count()
}

fn wrap_words(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;

    for word in words {
        let word_vis_len = visible_len(word);

        if current_line.is_empty() {
            current_line.push_str(word);
            current_len = word_vis_len;
        } else if current_len + 1 + word_vis_len <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
            current_len += 1 + word_vis_len;
        } else {
            lines.push(current_line);
            current_line = word.to_string();
            current_len = word_vis_len;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

pub fn parse_inline_spans(input: &str, base_style: Style, _mode_color: Color) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut current_text = String::new();

    let code_style = Style::default()
        .fg(Color::Rgb(250, 204, 21)) // warm amber
        .add_modifier(Modifier::BOLD);

    let bold_style = base_style
        .fg(TEXT_PRIMARY)
        .add_modifier(Modifier::BOLD);

    let italic_style = base_style
        .add_modifier(Modifier::ITALIC);

    let bold_italic_style = base_style
        .fg(TEXT_PRIMARY)
        .add_modifier(Modifier::BOLD | Modifier::ITALIC);

    let strike_style = base_style
        .add_modifier(Modifier::CROSSED_OUT);

    let flush = |spans: &mut Vec<Span<'static>>, buf: &mut String, style: Style| {
        if !buf.is_empty() {
            spans.push(Span::styled(buf.clone(), style));
            buf.clear();
        }
    };

    while i < len {
        // 1. Inline code: `code`
        if chars[i] == '`' {
            flush(&mut spans, &mut current_text, base_style);
            i += 1;
            let mut code_buf = String::new();
            while i < len && chars[i] != '`' {
                code_buf.push(chars[i]);
                i += 1;
            }
            if i < len && chars[i] == '`' {
                i += 1;
            }
            spans.push(Span::styled(code_buf, code_style));
            continue;
        }

        // 2. Bold italic: ***text***
        if i + 2 < len && chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*' {
            flush(&mut spans, &mut current_text, base_style);
            i += 3;
            let mut buf = String::new();
            while i + 2 < len && !(chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*') {
                buf.push(chars[i]);
                i += 1;
            }
            if i + 2 < len {
                i += 3;
            }
            spans.push(Span::styled(buf, bold_italic_style));
            continue;
        }

        // 3. Bold: **text** or __text__
        if (i + 1 < len && chars[i] == '*' && chars[i + 1] == '*') || (i + 1 < len && chars[i] == '_' && chars[i + 1] == '_') {
            let marker = chars[i];
            flush(&mut spans, &mut current_text, base_style);
            i += 2;
            let mut buf = String::new();
            while i + 1 < len && !(chars[i] == marker && chars[i + 1] == marker) {
                buf.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            spans.push(Span::styled(buf, bold_style));
            continue;
        }

        // 4. Strikethrough: ~~text~~
        if i + 1 < len && chars[i] == '~' && chars[i + 1] == '~' {
            flush(&mut spans, &mut current_text, base_style);
            i += 2;
            let mut buf = String::new();
            while i + 1 < len && !(chars[i] == '~' && chars[i + 1] == '~') {
                buf.push(chars[i]);
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            spans.push(Span::styled(buf, strike_style));
            continue;
        }

        // 5. Italic: *text* (when not followed by space and closed before newline)
        if chars[i] == '*' && i + 1 < len && chars[i + 1] != ' ' {
            flush(&mut spans, &mut current_text, base_style);
            i += 1;
            let mut buf = String::new();
            while i < len && chars[i] != '*' && chars[i] != '\n' {
                buf.push(chars[i]);
                i += 1;
            }
            if i < len && chars[i] == '*' {
                i += 1;
            }
            spans.push(Span::styled(buf, italic_style));
            continue;
        }

        // 6. Links: [label](url)
        if chars[i] == '[' {
            let mut label = String::new();
            let mut j = i + 1;
            while j < len && chars[j] != ']' && chars[j] != '\n' {
                label.push(chars[j]);
                j += 1;
            }
            if j + 1 < len && chars[j] == ']' && chars[j + 1] == '(' {
                let mut k = j + 2;
                while k < len && chars[k] != ')' && chars[k] != '\n' {
                    k += 1;
                }
                if k < len && chars[k] == ')' {
                    flush(&mut spans, &mut current_text, base_style);
                    spans.push(Span::styled(label, Style::default().fg(Color::Rgb(96, 165, 250)).add_modifier(Modifier::UNDERLINED)));
                    i = k + 1;
                    continue;
                }
            }
        }

        current_text.push(chars[i]);
        i += 1;
    }

    flush(&mut spans, &mut current_text, base_style);
    spans
}

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !is_table_row(trimmed) {
        return false;
    }
    let content = trimmed.trim_matches('|');
    content.chars().all(|c| c == '-' || c == ':' || c == '|' || c == ' ')
}

fn parse_numbered_list(line: &str) -> Option<(&str, &str)> {
    if let Some(dot_idx) = line.find(". ") {
        let prefix = &line[..dot_idx];
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            return Some((prefix, line[dot_idx + 2..].trim()));
        }
    }
    None
}

fn render_table(table_lines: &[&str], mode_color: Color, max_width: usize) -> Vec<Line<'static>> {
    let mut lines_out = Vec::new();
    let mut parsed_rows: Vec<Vec<String>> = Vec::new();

    for t_line in table_lines {
        let trimmed = t_line.trim();
        if is_table_separator(trimmed) {
            continue;
        }
        let cells: Vec<String> = trimmed
            .split('|')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if !cells.is_empty() {
            parsed_rows.push(cells);
        }
    }

    if parsed_rows.is_empty() {
        return lines_out;
    }

    let num_cols = parsed_rows[0].len();
    if num_cols == 0 {
        return lines_out;
    }

    let avail_width = max_width.saturating_sub(6);

    // If available width is too narrow or columns > 4, render as clean indented cards
    if avail_width < 50 || num_cols > 4 {
        let headers = &parsed_rows[0];
        for row in parsed_rows.iter().skip(1) {
            let title = row.get(0).map(|s| s.as_str()).unwrap_or("Item");
            lines_out.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("▪ ", Style::default().fg(mode_color)),
                Span::styled(format!("{}: ", headers.get(0).map(|s| s.as_str()).unwrap_or("")), Style::default().fg(TEXT_DARK)),
            ]));
            let mut title_spans = vec![Span::raw("     ")];
            title_spans.extend(parse_inline_spans(title, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD), mode_color));
            lines_out.push(Line::from(title_spans));

            for c in 1..num_cols {
                if let Some(val) = row.get(c) {
                    let header_name = headers.get(c).map(|s| s.as_str()).unwrap_or("");
                    let prefix = format!("     {}: ", header_name);
                    let wrapped = wrap_words(val, avail_width.saturating_sub(prefix.len() + 2));
                    for (w_idx, w) in wrapped.iter().enumerate() {
                        let mut spans = Vec::new();
                        if w_idx == 0 {
                            spans.push(Span::styled(prefix.clone(), Style::default().fg(TEXT_DARK)));
                        } else {
                            spans.push(Span::raw(" ".repeat(prefix.len())));
                        }
                        spans.extend(parse_inline_spans(w, Style::default().fg(TEXT_MUTED), mode_color));
                        lines_out.push(Line::from(spans));
                    }
                }
            }
            lines_out.push(Line::from(""));
        }
        return lines_out;
    }

    // Normal width: Render Unicode Box-Drawing Table
    let border_overhead = num_cols * 3 + 1;
    let usable_text_width = avail_width.saturating_sub(border_overhead).max(num_cols * 8);

    // Calculate desired widths for each column
    let mut desired_widths = vec![0; num_cols];
    for row in &parsed_rows {
        for (c, cell) in row.iter().enumerate() {
            if c < num_cols {
                desired_widths[c] = desired_widths[c].max(visible_len(cell));
            }
        }
    }

    let total_desired: usize = desired_widths.iter().sum();
    let mut col_widths = vec![8; num_cols];

    if total_desired <= usable_text_width {
        col_widths = desired_widths;
    } else {
        // Distribute usable_text_width proportionally
        let mut allocated = 0;
        for c in 0..num_cols {
            let proportion = (desired_widths[c] as f32 / total_desired.max(1) as f32) * usable_text_width as f32;
            let w = (proportion.round() as usize).max(8);
            col_widths[c] = w;
            allocated += w;
        }
        if allocated < usable_text_width {
            col_widths[num_cols - 1] += usable_text_width - allocated;
        } else if allocated > usable_text_width {
            let diff = allocated - usable_text_width;
            col_widths[num_cols - 1] = col_widths[num_cols - 1].saturating_sub(diff).max(8);
        }
    }

    // Top border: ┌───────┬────────┬────────┐
    let mut top = String::from("   ┌");
    for (c, &w) in col_widths.iter().enumerate() {
        top.push_str(&"─".repeat(w + 2));
        if c + 1 < num_cols {
            top.push('┬');
        } else {
            top.push('┐');
        }
    }
    lines_out.push(Line::from(Span::styled(top, Style::default().fg(BORDER_SUBTLE))));

    // Header row
    let header_row = &parsed_rows[0];
    let mut header_sublines: Vec<Vec<String>> = Vec::new();
    for c in 0..num_cols {
        let text = header_row.get(c).map(|s| s.as_str()).unwrap_or("");
        header_sublines.push(wrap_words(text, col_widths[c]));
    }
    let header_height = header_sublines.iter().map(|v| v.len()).max().unwrap_or(1);
    for h in 0..header_height {
        let mut spans = vec![
            Span::raw("   "),
            Span::styled("│", Style::default().fg(BORDER_SUBTLE)),
        ];
        for c in 0..num_cols {
            let cell_text = header_sublines[c].get(h).map(|s| s.as_str()).unwrap_or("");
            let pad = col_widths[c].saturating_sub(visible_len(cell_text));
            spans.push(Span::raw(" "));
            spans.extend(parse_inline_spans(cell_text, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD), mode_color));
            spans.push(Span::raw(" ".repeat(pad)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled("│", Style::default().fg(BORDER_SUBTLE)));
        }
        lines_out.push(Line::from(spans));
    }

    // Header separator: ├───────┼────────┼────────┤
    let mut sep = String::from("   ├");
    for (c, &w) in col_widths.iter().enumerate() {
        sep.push_str(&"─".repeat(w + 2));
        if c + 1 < num_cols {
            sep.push('┼');
        } else {
            sep.push('┤');
        }
    }
    lines_out.push(Line::from(Span::styled(sep, Style::default().fg(BORDER_SUBTLE))));

    // Data rows
    for row in parsed_rows.iter().skip(1) {
        let mut row_sublines: Vec<Vec<String>> = Vec::new();
        for c in 0..num_cols {
            let text = row.get(c).map(|s| s.as_str()).unwrap_or("");
            row_sublines.push(wrap_words(text, col_widths[c]));
        }
        let row_height = row_sublines.iter().map(|v| v.len()).max().unwrap_or(1);
        for h in 0..row_height {
            let mut spans = vec![
                Span::raw("   "),
                Span::styled("│", Style::default().fg(BORDER_SUBTLE)),
            ];
            for c in 0..num_cols {
                let cell_text = row_sublines[c].get(h).map(|s| s.as_str()).unwrap_or("");
                let pad = col_widths[c].saturating_sub(visible_len(cell_text));
                spans.push(Span::raw(" "));
                let base_cell_style = if c == 0 {
                    Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT_MUTED)
                };
                spans.extend(parse_inline_spans(cell_text, base_cell_style, mode_color));
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(Span::raw(" "));
                spans.push(Span::styled("│", Style::default().fg(BORDER_SUBTLE)));
            }
            lines_out.push(Line::from(spans));
        }
    }

    // Bottom border: └───────┴────────┴────────┘
    let mut bot = String::from("   └");
    for (c, &w) in col_widths.iter().enumerate() {
        bot.push_str(&"─".repeat(w + 2));
        if c + 1 < num_cols {
            bot.push('┴');
        } else {
            bot.push('┘');
        }
    }
    lines_out.push(Line::from(Span::styled(bot, Style::default().fg(BORDER_SUBTLE))));
    lines_out.push(Line::from(""));

    lines_out
}

pub fn render_assistant_markdown(text: &str, mode_color: Color, max_width: usize) -> Vec<Line<'static>> {
    let mut lines_out = Vec::new();
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut in_code_block = false;

    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

        // 1. Code block toggles
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            let lang = trimmed.trim_start_matches('`').trim();
            if in_code_block {
                let title = if lang.is_empty() { "code".to_string() } else { lang.to_string() };
                let bar_len = max_width.saturating_sub(title.len() + 10).min(50);
                lines_out.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(format!("┌─ {} {}", title, "─".repeat(bar_len)), Style::default().fg(BORDER_SUBTLE)),
                ]));
            } else {
                let bar_len = max_width.saturating_sub(6).min(56);
                lines_out.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(format!("└{}", "─".repeat(bar_len)), Style::default().fg(BORDER_SUBTLE)),
                ]));
                lines_out.push(Line::from(""));
            }
            i += 1;
            continue;
        }

        // Inside code block: preserve exact indentation, don't parse markdown
        if in_code_block {
            lines_out.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("│ ", Style::default().fg(BORDER_SUBTLE)),
                Span::styled(line.to_string(), Style::default().fg(Color::Rgb(220, 225, 235))),
            ]));
            i += 1;
            continue;
        }

        // 2. Empty line
        if trimmed.is_empty() {
            lines_out.push(Line::from(""));
            i += 1;
            continue;
        }

        // 3. Markdown Table detection
        if is_table_row(trimmed) {
            let mut table_block = Vec::new();
            while i < raw_lines.len() && is_table_row(raw_lines[i].trim()) {
                table_block.push(raw_lines[i].trim());
                i += 1;
            }
            let mut rendered_table = render_table(&table_block, mode_color, max_width);
            lines_out.append(&mut rendered_table);
            continue;
        }

        // 4. Headings
        if trimmed.starts_with("# ") {
            let title = trimmed[2..].trim();
            lines_out.push(Line::from(""));
            let mut spans = vec![Span::raw("   ")];
            spans.extend(parse_inline_spans(title, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD | Modifier::UNDERLINED), mode_color));
            lines_out.push(Line::from(spans));
            lines_out.push(Line::from(""));
            i += 1;
            continue;
        }
        if trimmed.starts_with("## ") {
            let title = trimmed[3..].trim();
            lines_out.push(Line::from(""));
            let mut spans = vec![Span::raw("   ")];
            spans.extend(parse_inline_spans(title, Style::default().fg(mode_color).add_modifier(Modifier::BOLD), mode_color));
            lines_out.push(Line::from(spans));
            i += 1;
            continue;
        }
        if trimmed.starts_with("### ") {
            let title = trimmed[4..].trim();
            let mut spans = vec![Span::raw("   ")];
            spans.extend(parse_inline_spans(title, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD), mode_color));
            lines_out.push(Line::from(spans));
            i += 1;
            continue;
        }
        if trimmed.starts_with("#### ") {
            let title = trimmed[5..].trim();
            let mut spans = vec![Span::raw("   ")];
            spans.extend(parse_inline_spans(title, Style::default().fg(TEXT_MUTED).add_modifier(Modifier::BOLD), mode_color));
            lines_out.push(Line::from(spans));
            i += 1;
            continue;
        }

        // 5. Horizontal divider
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            let line_len = max_width.saturating_sub(6).min(64);
            lines_out.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("─".repeat(line_len), Style::default().fg(BORDER_SUBTLE)),
            ]));
            i += 1;
            continue;
        }

        // 6. Blockquote (> ...)
        if trimmed.starts_with("> ") {
            let quote_text = &trimmed[2..];
            let wrapped = wrap_words(quote_text, max_width.saturating_sub(8));
            for q in wrapped {
                let mut spans = vec![
                    Span::raw("   "),
                    Span::styled("│ ", Style::default().fg(mode_color)),
                ];
                spans.extend(parse_inline_spans(&q, Style::default().fg(TEXT_MUTED).add_modifier(Modifier::ITALIC), mode_color));
                lines_out.push(Line::from(spans));
            }
            i += 1;
            continue;
        }

        // 7. Unordered list (- item, * item, + item)
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            let item_text = &trimmed[2..];
            let wrapped = wrap_words(item_text, max_width.saturating_sub(8));
            for (idx, w) in wrapped.iter().enumerate() {
                let mut spans = Vec::new();
                if idx == 0 {
                    spans.push(Span::raw("   "));
                    spans.push(Span::styled("• ", Style::default().fg(mode_color)));
                } else {
                    spans.push(Span::raw("     "));
                }
                spans.extend(parse_inline_spans(w, Style::default().fg(TEXT_MUTED), mode_color));
                lines_out.push(Line::from(spans));
            }
            i += 1;
            continue;
        }

        // 8. Ordered list (1. item, 2. item, etc.)
        if let Some((num_str, rest)) = parse_numbered_list(trimmed) {
            let prefix = format!("   {}. ", num_str);
            let continuation_spaces = " ".repeat(prefix.len());
            let wrapped = wrap_words(rest, max_width.saturating_sub(prefix.len() + 3));
            for (idx, w) in wrapped.iter().enumerate() {
                let mut spans = Vec::new();
                if idx == 0 {
                    spans.push(Span::styled(prefix.clone(), Style::default().fg(mode_color)));
                } else {
                    spans.push(Span::raw(continuation_spaces.clone()));
                }
                spans.extend(parse_inline_spans(w, Style::default().fg(TEXT_MUTED), mode_color));
                lines_out.push(Line::from(spans));
            }
            i += 1;
            continue;
        }

        // 9. Document grounding source citation tags
        if trimmed.contains("[Source:") || trimmed.contains("[Grounding:") || trimmed.contains("[Konten File") {
            lines_out.push(Line::from(vec![
                Span::raw("   "),
                Span::styled("📄 ", Style::default().fg(ACCENT_ORANGE)),
                Span::styled(trimmed.to_string(), Style::default().fg(ACCENT_ORANGE)),
            ]));
            i += 1;
            continue;
        }

        // 10. Normal paragraph
        let wrapped = wrap_words(line, max_width.saturating_sub(6));
        for p in wrapped {
            let mut spans = vec![Span::raw("   ")];
            spans.extend(parse_inline_spans(&p, Style::default().fg(TEXT_MUTED), mode_color));
            lines_out.push(Line::from(spans));
        }
        i += 1;
    }

    lines_out
}

fn render_chat_stream(f: &mut Frame, app: &mut TuiApp, area: Rect) {
    let mut chat_lines = Vec::new();
    let width = area.width.max(10) as usize;

    for msg in &app.messages {
        match msg.sender.as_str() {
            "User" => {
                // User Prompt Card with its OWN Mode Tag & Color
                let user_max_w = width.saturating_sub(6);
                for u_line in msg.text.lines() {
                    let wrapped = wrap_words(u_line, user_max_w);
                    for (idx, w) in wrapped.iter().enumerate() {
                        let prefix = if idx == 0 { " ▌ " } else { " ▌ " };
                        chat_lines.push(Line::from(vec![
                            Span::styled(prefix, Style::default().fg(msg.mode_color)),
                            Span::styled(w.clone(), Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
                        ]));
                    }
                }
                chat_lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled("▣ ", Style::default().fg(msg.mode_color)),
                    Span::styled(&msg.mode_name, Style::default().fg(msg.mode_color)),
                    Span::styled(" · ", Style::default().fg(TEXT_DARK)),
                    Span::styled(&msg.model_info, Style::default().fg(TEXT_MUTED)),
                ]));
                chat_lines.push(Line::from(""));
            }
            "Lontar" => {
                chat_lines.push(Line::from(vec![
                    Span::styled(" ◆ lontar", Style::default().fg(msg.mode_color).add_modifier(Modifier::BOLD)),
                ]));

                let assistant_lines = render_assistant_markdown(&msg.text, msg.mode_color, width);
                chat_lines.extend(assistant_lines);
                chat_lines.push(Line::from(""));
            }
            "Tool" => {
                chat_lines.push(Line::from(vec![
                    Span::styled("   ╭─ ⚙ Tool Executed ──────────────────────────────╮", Style::default().fg(TEXT_DIM)),
                ]));
                for line in msg.text.lines() {
                    chat_lines.push(Line::from(vec![
                        Span::styled("   │ ", Style::default().fg(TEXT_DIM)),
                        Span::styled(line, Style::default().fg(TEXT_MUTED)),
                    ]));
                }
                chat_lines.push(Line::from(vec![
                    Span::styled("   ╰────────────────────────────────────────────────╯", Style::default().fg(TEXT_DIM)),
                ]));
                chat_lines.push(Line::from(""));
            }
            "Error" => {
                for (i, line) in msg.text.lines().enumerate() {
                    let prefix = if i == 0 { "   ✖ Error: " } else { "     " };
                    chat_lines.push(Line::from(vec![
                        Span::styled(
                            format!("{}{}", prefix, line),
                            Style::default().fg(ERROR_RED).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
                chat_lines.push(Line::from(""));
            }
            _ => {
                for (i, line) in msg.text.lines().enumerate() {
                    let prefix = if i == 0 { "   ℹ " } else { "     " };
                    chat_lines.push(Line::from(vec![
                        Span::styled(format!("{}{}", prefix, line), Style::default().fg(TEXT_DIM)),
                    ]));
                }
                chat_lines.push(Line::from(""));
            }
        }
    }

    // Pending assistant row while loading
    if app.is_loading {
        chat_lines.push(Line::from(vec![
            Span::styled(" ◆ lontar", Style::default().fg(app.mode.color()).add_modifier(Modifier::BOLD)),
        ]));
        let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spin = spinner_chars[(app.spinner_frame / 2) % spinner_chars.len()];
        chat_lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(format!("Thinking... {}", spin), Style::default().fg(TEXT_DIM)),
        ]));
        chat_lines.push(Line::from(""));
    }

    // Dynamic Visual Lines & Scroll Calculation
    let mut total_visual_lines = 0;
    for line in &chat_lines {
        let len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        let rows = if len == 0 { 1 } else { (len + width - 1) / width };
        total_visual_lines += rows;
    }

    let viewport_height = area.height as usize;
    let max_scroll = total_visual_lines.saturating_sub(viewport_height);
    app.max_scroll = max_scroll;

    let scroll_y = max_scroll.saturating_sub(app.scroll_offset_from_bottom);

    let chat_paragraph = Paragraph::new(chat_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0));

    f.render_widget(chat_paragraph, area);
}
