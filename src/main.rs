use anyhow::Result;
use clap::Parser;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use lontar::agent::Agent;
use lontar::cli::{
    AuthCommands, Cli, Commands, ConfigCommands, ContextCommands, DbCommands, SessionCommands,
    StudyGuideArgs, PrdArgs, TasklistArgs,
};
use lontar::config::Config;
use lontar::db::{Database, DbSession, DbTestLog};
use lontar::document::DocumentEngine;
use lontar::router::{LlmRouter, SUPPORTED_PROVIDERS};
use lontar::session::{Session, SessionStore};
use lontar::tools::ToolRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load().unwrap_or_default();

    // Check subcommands
    match &cli.command {
        Some(Commands::Config(args)) => {
            handle_config_command(args.action.clone(), &mut config)?;
            return Ok(());
        }
        Some(Commands::Auth(args)) => {
            handle_auth_command(args.action.clone(), &mut config)?;
            return Ok(());
        }
        Some(Commands::Guide(args)) => {
            let guide_text = lontar::tui::get_provider_guide(args.provider.as_deref());
            println!("{}", guide_text);
            return Ok(());
        }
        Some(Commands::Models) => {
            print_supported_models(&config);
            return Ok(());
        }
        Some(Commands::Session(args)) => {
            handle_session_command(&args.action, &config)?;
            return Ok(());
        }
        Some(Commands::Context(args)) => {
            handle_context_command(&args.action, &cli, &config)?;
            return Ok(());
        }
        Some(Commands::StudyGuide(args)) => {
            handle_study_guide_command(args, &cli, &config).await?;
            return Ok(());
        }
        Some(Commands::Prd(args)) => {
            handle_prd_command(args, &cli, &config).await?;
            return Ok(());
        }
        Some(Commands::Tasklist(args)) => {
            handle_tasklist_command(args, &cli, &config).await?;
            return Ok(());
        }
        Some(Commands::Db(args)) => {
            handle_db_command(&args.action)?;
            return Ok(());
        }
        Some(Commands::Run(args)) => {
            let prompt = args.prompt.join(" ");
            run_single_task(&prompt, &cli, &config).await?;
            return Ok(());
        }
        Some(Commands::Chat(args)) => {
            if args.classic {
                run_interactive_session(args.initial_prompt.clone(), &cli, &config).await?;
            } else {
                let session = get_or_create_session(&cli, &config)?;
                lontar::tui::run_tui(session, config.clone(), cli.yes).await?;
            }
            return Ok(());
        }
        None => {}
    }

    // If one-shot prompt was passed directly without subcommand
    if !cli.prompt.is_empty() {
        let prompt = cli.prompt.join(" ");
        run_single_task(&prompt, &cli, &config).await?;
        return Ok(());
    }

    // Default to interactive full Ratatui TUI session (OpenCode & Kiro style)
    let session = get_or_create_session(&cli, &config)?;
    lontar::tui::run_tui(session, config.clone(), cli.yes).await?;
    Ok(())
}

fn print_lontar_banner() {
    let banner = r#"
 ╔═════════════════════════════════════════════════════════════════════╗
 ║   🌴 LONTAR CLI - Zero-Cost AI Agent & Document Intelligence        ║
 ║   Kiro & OpenCode Terminal UX │ NotebookLM Grounding │ 100% Rust    ║
 ╚═════════════════════════════════════════════════════════════════════╝"#;
    println!("{}", banner.bold().cyan());
}

fn handle_auth_command(action: Option<AuthCommands>, config: &mut Config) -> Result<()> {
    match action.unwrap_or(AuthCommands::List) {
        AuthCommands::List => {
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("{}", "            🔑 Authenticated Integrations & Status         ".bold().cyan());
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("{:<18} {:<24} {}", "INTEGRATION", "CREDENTIAL / MODEL", "STATUS");
            println!("{}", "─".repeat(60).dimmed());

            let providers = [
                ("lontar-free", "Lontar Free AI", "openai-fast / deepseek", true),
                ("gemini", "Google Gemini", &config.gemini_model, config.get_api_key("gemini").is_some()),
                ("groq", "Groq Cloud", &config.groq_model, config.get_api_key("groq").is_some()),
                ("deepseek", "DeepSeek", &config.deepseek_model, config.get_api_key("deepseek").is_some()),
                ("openrouter", "OpenRouter", &config.openrouter_model, config.get_api_key("openrouter").is_some()),
                ("github", "GitHub Models", &config.github_model, config.get_api_key("github").is_some()),
                ("sambanova", "SambaNova", &config.sambanova_model, config.get_api_key("sambanova").is_some()),
                ("mistral", "Mistral AI", &config.mistral_model, config.get_api_key("mistral").is_some()),
                ("ollama", "Ollama (Local)", &config.ollama_model, true),
                ("openai", "OpenAI", &config.openai_model, config.get_api_key("openai").is_some()),
                ("anthropic", "Anthropic", &config.anthropic_model, config.get_api_key("anthropic").is_some()),
            ];

            for (_id, name, model, connected) in providers {
                let status = if connected {
                    "connected (stored)".green().bold()
                } else {
                    "not connected".dimmed()
                };
                println!("{:<18} {:<24} {}", name.bold(), model, status);
            }
            if !config.custom_providers.is_empty() {
                println!("───────────────────────────────────────────────────────────");
                println!("{}", "CUSTOM PROVIDERS:".bold().magenta());
                for (name, custom) in &config.custom_providers {
                    let key_stat = if custom.api_key.is_some() { "connected (stored)".green() } else { "no key".dimmed() };
                    println!("{:<18} {:<24} {} (URL: {})", name.bold(), custom.model, key_stat, custom.base_url.dimmed());
                }
            }
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("💡 Hubungkan provider baru: lontar auth login");
            println!("💡 Tambah custom provider: lontar auth custom <nama> <url> [-k key] [-m model]");
            println!("💡 Tutorial API gratis: lontar guide");
        }
        AuthCommands::Guide { provider } => {
            let guide_text = lontar::tui::get_provider_guide(provider.as_deref());
            println!("{}", guide_text);
        }
        AuthCommands::Custom { name, base_url, api_key, model } => {
            let model_val = model.unwrap_or_else(|| "default".to_string());
            config.set_custom_provider(&name, &base_url, api_key.as_deref(), &model_val)?;
            config.default_provider = name.to_lowercase();
            config.save()?;
            println!("{} Berhasil mendaftarkan custom provider '{}'!", "✔".green().bold(), name.bold());
            println!("   • Base URL : {}", base_url.cyan());
            println!("   • Model    : {}", model_val.yellow());
            println!("   • API Key  : {}", if api_key.is_some() { "Tersimpan" } else { "None" });
            println!("{} Provider default kini disetel ke: {}", "●".cyan(), name.green().bold());
        }
        AuthCommands::Login { provider } => {
            let target_prov = match provider {
                Some(p) => p.to_lowercase(),
                None => {
                    println!("{}", "╭─ Connect an integration (OpenCode style) ────────────────╮".bold().cyan());
                    println!("│ Pilih provider AI yang ingin dihubungkan:                │");
                    println!("│   1. Google Gemini (Gemini 2.0 Flash / 2.5 Flash)        │");
                    println!("│   2. Groq Cloud (Llama 3.3 70B Versatile @ 300 t/s)      │");
                    println!("│   3. DeepSeek (DeepSeek V3 / R1)                         │");
                    println!("│   4. OpenRouter (:free / commercial hub)                 │");
                    println!("│   5. GitHub Models (GPT-4o Mini free tier)               │");
                    println!("│   6. OpenAI (GPT-4o, GPT-4o Mini)                        │");
                    println!("│   7. Anthropic (Claude 3.5 Sonnet)                       │");
                    println!("│   8. SambaNova (Llama 3.3 70B Fast RDU)                  │");
                    println!("│   9. Mistral AI (Mistral NeMo 12B)                       │");
                    println!("│  10. Ollama Local (Local Offline host)                   │");
                    println!("│  11. Tambah Custom Provider (Cerebras, Together, vLLM)   │");
                    println!("╰──────────────────────────────────────────────────────────╯");

                    let mut rl = DefaultEditor::new()?;
                    let choice = rl.readline("Pilih nomor [1-11] (default: 1): ")?;
                    match choice.trim() {
                        "1" | "" => "gemini".to_string(),
                        "2" => "groq".to_string(),
                        "3" => "deepseek".to_string(),
                        "4" => "openrouter".to_string(),
                        "5" => "github".to_string(),
                        "6" => "openai".to_string(),
                        "7" => "anthropic".to_string(),
                        "8" => "sambanova".to_string(),
                        "9" => "mistral".to_string(),
                        "10" => "ollama".to_string(),
                        "11" => {
                            let name = rl.readline("Nama Provider (contoh: cerebras, together, vllm): ")?;
                            let name = name.trim().to_lowercase();
                            if name.is_empty() {
                                println!("{}", "✖ Nama provider tidak boleh kosong.".yellow());
                                return Ok(());
                            }
                            let base_url = rl.readline("Base URL (contoh: https://api.cerebras.ai/v1): ")?;
                            let base_url = base_url.trim().to_string();
                            if base_url.is_empty() {
                                println!("{}", "✖ Base URL tidak boleh kosong.".yellow());
                                return Ok(());
                            }
                            let key = rl.readline("API Key (kosongkan jika tanpa auth): ")?;
                            let api_key = if key.trim().is_empty() { None } else { Some(key.trim().to_string()) };
                            let model = rl.readline("Nama Model (default: default): ")?;
                            let model = if model.trim().is_empty() { "default".to_string() } else { model.trim().to_string() };

                            config.set_custom_provider(&name, &base_url, api_key.as_deref(), &model)?;
                            config.default_provider = name.clone();
                            config.save()?;
                            println!("{} Berhasil mendaftarkan custom provider '{}'!", "✔".green().bold(), name.bold());
                            println!("{} Provider default kini disetel ke: {}", "●".cyan(), name.green().bold());
                            return Ok(());
                        }
                        other => other.to_lowercase(),
                    }
                }
            };

            let mut rl = DefaultEditor::new()?;
            if target_prov == "ollama" {
                let host = rl.readline("Masukkan Ollama Host URL (default: http://localhost:11434/v1): ")?;
                let host = if host.trim().is_empty() { "http://localhost:11434/v1" } else { host.trim() };
                config.set_value("ollama_base_url", host)?;
                config.default_provider = "ollama".to_string();
                config.save()?;
                println!("{} Berhasil menghubungkan Ollama di {}", "✔".green().bold(), host.bold());
            } else {
                let prompt_label = format!("Masukkan API Key untuk '{}': ", target_prov);
                let key = rl.readline(&prompt_label)?;
                let key = key.trim();
                if key.is_empty() {
                    println!("{}", "✖ Input dibatalkan (API key kosong).".yellow());
                    return Ok(());
                }

                let key_name = if target_prov == "github" { "github_token" } else { &format!("{}_api_key", target_prov) };
                config.set_value(key_name, key)?;
                config.default_provider = target_prov.clone();
                config.save()?;
                println!("{} Berhasil login & menyimpan kredensial '{}'!", "✔".green().bold(), target_prov.bold());
                println!("{} Provider default kini disetel ke: {}", "●".cyan(), target_prov.green().bold());
            }
        }
        AuthCommands::Logout { provider } => {
            let key_name = if provider == "github" { "github_token" } else { &format!("{}_api_key", provider) };
            let _ = config.set_value(key_name, "");
            config.save()?;
            println!("{} Berhasil logout dari provider '{}'.", "✔".green().bold(), provider.bold());
        }
    }
    Ok(())
}

fn handle_config_command(action: Option<ConfigCommands>, config: &mut Config) -> Result<()> {
    match action.unwrap_or(ConfigCommands::Show) {
        ConfigCommands::Show => {
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("{}", "               🔧 Konfigurasi LONTAR CLI                   ".bold().cyan());
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
            println!("Path Config       : {:?}", Config::config_path());
            println!("Provider Default  : {}", config.default_provider.green().bold());
            println!("Auto Confirm (HITL): {}", if config.auto_confirm { "Ya".green() } else { "Tidak (Diff Preview Aktif)".yellow() });
            println!("Max Iterations    : {}", config.max_iterations);
            println!("───────────────────────────────────────────────────────────");
            println!("{}", "GRATIS / ZERO-COST PROVIDERS:".bold().magenta());
            println!("1. Groq (Llama-3.3 70B) : Key: {} | Model: {}", mask_key(&config.groq_api_key), config.groq_model);
            println!("2. Google Gemini Flash  : Key: {} | Model: {}", mask_key(&config.gemini_api_key), config.gemini_model);
            println!("3. OpenRouter Free (:free): Key: {} | Model: {}", mask_key(&config.openrouter_api_key), config.openrouter_model);
            println!("4. DeepSeek (V3/R1)     : Key: {} | Model: {}", mask_key(&config.deepseek_api_key), config.deepseek_model);
            println!("5. GitHub Models        : Key: {} | Model: {}", mask_key(&config.github_token), config.github_model);
            println!("6. SambaNova Cloud      : Key: {} | Model: {}", mask_key(&config.sambanova_api_key), config.sambanova_model);
            println!("7. Mistral NeMo         : Key: {} | Model: {}", mask_key(&config.mistral_api_key), config.mistral_model);
            println!("8. Ollama Local Offline : Host: {:?} | Model: {}", config.ollama_base_url, config.ollama_model);
            println!("───────────────────────────────────────────────────────────");
            println!("{}", "KOMERSIAL (OPSIONAL):".bold().dimmed());
            println!("OpenAI GPT-4o           : Key: {} | Model: {}", mask_key(&config.openai_api_key), config.openai_model);
            println!("Anthropic Claude Sonnet : Key: {} | Model: {}", mask_key(&config.anthropic_api_key), config.anthropic_model);
            println!("{}", "═══════════════════════════════════════════════════════════".cyan());
        }
        ConfigCommands::Set { key, value } => {
            config.set_value(&key, &value)?;
            println!("{} Berhasil menyetel {} = {}", "✔".green().bold(), key.bold(), value);
        }
        ConfigCommands::Init => {
            let def = Config::default();
            def.save()?;
            println!("{} Berhasil menginisialisasi config baru di {:?}", "✔".green().bold(), Config::config_path());
        }
    }
    Ok(())
}

fn handle_session_command(action: &SessionCommands, config: &Config) -> Result<()> {
    match action {
        SessionCommands::List => {
            let list = SessionStore::list()?;
            if list.is_empty() {
                println!("{}", "Belum ada sesi tersimpan.".yellow());
                return Ok(());
            }
            println!("{}", "📋 Daftar Sesi Lontar Tersimpan:".bold().cyan());
            println!("{:<38} {:<24} {:<12} {:<10} {}", "ID", "NAMA", "PROVIDER", "PESAN", "TERAKHIR DIPERBARUI");
            println!("{}", "─".repeat(95).dimmed());
            for s in list {
                println!(
                    "{:<38} {:<24} {:<12} {:<10} {}",
                    s.id.cyan(),
                    s.name.bold(),
                    s.provider.green(),
                    s.message_count.to_string().yellow(),
                    s.updated_at.format("%Y-%m-%d %H:%M").to_string().dimmed()
                );
            }
        }
        SessionCommands::New { name } => {
            let provider = config.default_provider.clone();
            let model = config.get_model(&provider);
            let sess = SessionStore::create(name.clone(), provider, model)?;
            if let Ok(db) = Database::open_default() {
                let _ = db.insert_session(&DbSession {
                    id: sess.id.clone(),
                    name: sess.name.clone(),
                    created_at: sess.created_at.to_rfc3339(),
                    updated_at: sess.updated_at.to_rfc3339(),
                    provider: sess.provider.clone(),
                    model: sess.model.clone(),
                    total_tokens: 0,
                });
            }
            println!("{} Sesi baru berhasil dibuat: {} ({})", "✔".green().bold(), sess.name.bold(), sess.id.cyan());
        }
        SessionCommands::Show { id } => {
            let sess = SessionStore::load(id)?;
            println!("═══════════════════════════════════════════");
            println!("Sesi      : {} ({})", sess.name.bold(), sess.id.cyan());
            println!("Provider  : {} ({})", sess.provider.green(), sess.model.yellow());
            println!("Total Pesan: {}", sess.messages.len());
            println!("───────────────────────────────────────────");
            for (idx, msg) in sess.messages.iter().enumerate() {
                let badge = match msg.role.as_str() {
                    "user" => "👤 User".blue().bold(),
                    "assistant" => "🤖 Lontar".green().bold(),
                    "system" => "⚙️ System".dimmed(),
                    "tool" => "🔧 Tool".magenta(),
                    _ => msg.role.normal(),
                };
                println!("[#{}] {}", idx + 1, badge);
                if let Some(content) = &msg.content {
                    println!("    {}", content.trim());
                }
            }
            println!("═══════════════════════════════════════════");
        }
        SessionCommands::Delete { id } => {
            SessionStore::delete(id)?;
            if let Ok(db) = Database::open_default() {
                let _ = db.delete_session(id);
            }
            println!("{} Sesi '{}' berhasil dihapus.", "✔".green().bold(), id);
        }
        SessionCommands::Export { id, output } => {
            let sess = SessionStore::load(id)?;
            let path = SessionStore::export_markdown(&sess, output.clone())?;
            println!("{} Sesi berhasil diekspor ke: {:?}", "✔".green().bold(), path);
        }
    }
    Ok(())
}

fn handle_context_command(action: &ContextCommands, cli: &Cli, config: &Config) -> Result<()> {
    let db = Database::open_default()?;
    let session = get_or_create_session(cli, config)?;

    match action {
        ContextCommands::Add { paths } => {
            println!("{}", "📥 Melakukan ingest dokumen ke NotebookLM Grounding Index...".bold().cyan());
            for p in paths {
                match DocumentEngine::ingest_file(&db, &session.id, p) {
                    Ok(doc) => {
                        println!(
                            "  {} Ingested: {} ({} chunk, {} byte)",
                            "✔".green().bold(),
                            doc.file_path.bold(),
                            doc.chunk_count.to_string().cyan(),
                            doc.file_size.to_string().dimmed()
                        );
                    }
                    Err(e) => {
                        eprintln!("  {} Gagal ingest '{}': {}", "✖".red().bold(), p, e);
                    }
                }
            }
        }
        ContextCommands::List => {
            let docs = db.list_documents(&session.id)?;
            if docs.is_empty() {
                println!("{}", "Belum ada dokumen yang di-ingest pada sesi aktif ini.".yellow());
                println!("Gunakan: lontar context add <file.md|file.pdf|file.txt>");
                return Ok(());
            }
            println!("📚 Dokumen Tergrounding pada Sesi: {} ({})", session.name.bold(), session.id.cyan());
            println!("{:<4} {:<40} {:<8} {:<10} {}", "#", "PATH", "TIPE", "CHUNKS", "UKURAN");
            println!("{}", "─".repeat(75).dimmed());
            for (idx, d) in docs.iter().enumerate() {
                println!(
                    "{:<4} {:<40} {:<8} {:<10} {} byte",
                    idx + 1,
                    d.file_path.cyan(),
                    d.file_type.yellow(),
                    d.chunk_count.to_string().green(),
                    d.file_size
                );
            }
        }
        ContextCommands::Clear => {
            db.clear_session_documents(&session.id)?;
            println!("{} Seluruh dokumen konteks sesi aktif berhasil dibersihkan.", "✔".green().bold());
        }
    }
    Ok(())
}

async fn handle_study_guide_command(args: &StudyGuideArgs, cli: &Cli, config: &Config) -> Result<()> {
    let db = Database::open_default()?;
    let mut session = get_or_create_session(cli, config)?;

    if let Some(file_path) = &args.file {
        println!("{} Meng-ingest file rujukan: {}...", "📄".cyan(), file_path);
        let _ = DocumentEngine::ingest_file(&db, &session.id, file_path)?;
    }

    let grounded_context = DocumentEngine::retrieve_context(&db, &session.id, &args.topic, 6)?;
    let prompt = DocumentEngine::build_study_guide_prompt(&args.topic, &grounded_context);

    let provider_name = cli.provider.as_deref().unwrap_or(&config.default_provider);
    let router = LlmRouter::new(provider_name, cli.model.as_deref(), config)?;
    let agent = Agent::new(Arc::new(router), ToolRegistry::new(), config.clone(), cli.yes);

    println!("{}", format!("📚 Menyintesis Study Guide untuk: '{}'...", args.topic).bold().cyan());
    let response = agent.step(&mut session, &prompt).await?;

    let out_path = args.output.clone().unwrap_or_else(|| "STUDY_GUIDE.md".to_string());
    fs::write(&out_path, &response)?;
    println!("\n{} Study Guide berhasil disimpan ke: {}", "✔".green().bold(), out_path.bold().cyan());

    Ok(())
}

async fn handle_prd_command(args: &PrdArgs, cli: &Cli, config: &Config) -> Result<()> {
    let db = Database::open_default()?;
    let mut session = get_or_create_session(cli, config)?;

    let grounded_context = DocumentEngine::retrieve_context(&db, &session.id, &args.idea, 6)?;
    let prompt = DocumentEngine::build_prd_prompt(&args.idea, &grounded_context);

    let provider_name = cli.provider.as_deref().unwrap_or(&config.default_provider);
    let router = LlmRouter::new(provider_name, cli.model.as_deref(), config)?;
    let agent = Agent::new(Arc::new(router), ToolRegistry::new(), config.clone(), cli.yes);

    println!("{}", format!("📋 Menyintesis Product Requirements Document (PRD) untuk: '{}'...", args.idea).bold().cyan());
    let response = agent.step(&mut session, &prompt).await?;

    let out_path = args.output.clone().unwrap_or_else(|| "PRD.md".to_string());
    fs::write(&out_path, &response)?;
    println!("\n{} Dokumen PRD berhasil disimpan ke: {}", "✔".green().bold(), out_path.bold().cyan());

    Ok(())
}

async fn handle_tasklist_command(args: &TasklistArgs, cli: &Cli, config: &Config) -> Result<()> {
    let mut session = get_or_create_session(cli, config)?;

    let source_content = if Path::new(&args.source).exists() {
        fs::read_to_string(&args.source)?
    } else {
        args.source.clone()
    };

    let prompt = DocumentEngine::build_tasklist_prompt(&source_content);

    let provider_name = cli.provider.as_deref().unwrap_or(&config.default_provider);
    let router = LlmRouter::new(provider_name, cli.model.as_deref(), config)?;
    let agent = Agent::new(Arc::new(router), ToolRegistry::new(), config.clone(), cli.yes);

    println!("{}", "📝 Menyintesis Actionable Tasklist & Checklist...".bold().cyan());
    let response = agent.step(&mut session, &prompt).await?;

    let out_path = args.output.clone().unwrap_or_else(|| "TASKLIST.md".to_string());
    fs::write(&out_path, &response)?;
    println!("\n{} Tasklist berhasil disimpan ke: {}", "✔".green().bold(), out_path.bold().cyan());

    Ok(())
}

fn handle_db_command(action: &DbCommands) -> Result<()> {
    let db = Database::open_default()?;
    match action {
        DbCommands::Stats => {
            let sessions = db.list_sessions()?;
            let providers = db.list_providers()?;
            let logs = db.list_test_logs()?;
            println!("{}", "═══════════════════════════════════════════".cyan());
            println!("{}", "      📊 Lontar SQLite Database Stats      ".bold().cyan());
            println!("{}", "═══════════════════════════════════════════".cyan());
            println!("Path DB            : {:?}", Config::config_dir().join("lontar.db"));
            println!("Total Sessions     : {}", sessions.len().to_string().green());
            println!("Registered Providers: {}", providers.len().to_string().cyan());
            println!("Recent Test Logs   : {}", logs.len().to_string().yellow());
            println!("{}", "═══════════════════════════════════════════".cyan());
        }
        DbCommands::Providers => {
            let providers = db.list_providers()?;
            println!("{}", "Registered Providers in SQLite:".bold().cyan());
            println!("{:<14} {:<32} {:<6} {}", "ID", "DISPLAY NAME", "FREE", "DEFAULT MODEL");
            println!("{}", "─".repeat(70).dimmed());
            for p in providers {
                println!(
                    "{:<14} {:<32} {:<6} {}",
                    p.id.cyan(),
                    p.display_name,
                    if p.is_free { "YES".green() } else { "NO".dimmed() },
                    p.default_model.yellow()
                );
            }
        }
        DbCommands::Logs => {
            let logs = db.list_test_logs()?;
            if logs.is_empty() {
                println!("{}", "Belum ada riwayat test log tercatat di SQLite.".yellow());
                return Ok(());
            }
            println!("{}", "🧪 50 Riwayat Test Terakhir:".bold().cyan());
            for log in logs {
                let status_badge = if log.status == "PASS" {
                    "PASS".green().bold()
                } else {
                    "FAIL".red().bold()
                };
                println!(
                    "[{}] {} - {} ({} ms) | {}",
                    status_badge,
                    log.test_name.bold(),
                    log.session_id.dimmed(),
                    log.duration_ms,
                    log.timestamp.dimmed()
                );
            }
        }
    }
    Ok(())
}

fn print_supported_models(config: &Config) {
    println!("{}", "═══════════════════════════════════════════════════════════════════════".cyan());
    println!("{}", "        🚀 Provider & Model AI yang Didukung LONTAR CLI                 ".bold().cyan());
    println!("{}", "═══════════════════════════════════════════════════════════════════════".cyan());
    println!("{:<12} {:<32} {:<8} {}", "PROVIDER", "MODEL DEFAULT", "STATUS", "GRATIS?");
    println!("{}", "─".repeat(72).dimmed());

    for spec in SUPPORTED_PROVIDERS {
        let has_key = config.get_api_key(spec.id).is_some() || spec.id == "ollama";
        let status = if has_key {
            "SIAP".green().bold()
        } else {
            "BELUM SET".yellow()
        };
        let free_badge = if spec.is_free {
            "GRATIS".green().bold()
        } else {
            "Berbayar".dimmed()
        };

        println!(
            "{:<12} {:<32} {:<8} {}",
            spec.id.cyan(),
            spec.default_model.bold(),
            status,
            free_badge
        );
    }
    println!("{}", "═══════════════════════════════════════════════════════════════════════".cyan());
    println!("💡 Tip: Set API key dengan: lontar config set <provider>_api_key <KEY>");
}

async fn run_single_task(prompt: &str, cli: &Cli, config: &Config) -> Result<()> {
    let mut session = get_or_create_session(cli, config)?;
    let mut provider_name = cli.provider.as_deref().unwrap_or(&config.default_provider);
    if !config.has_configured_provider(provider_name) {
        provider_name = "lontar-free";
    }
    let router = LlmRouter::new(provider_name, cli.model.as_deref(), config)?;
    let tools = ToolRegistry::new();
    let agent = Agent::new(Arc::new(router), tools, config.clone(), cli.yes);

    println!("{}", format!("🚀 Menjalankan task: '{}'", prompt).bold());
    let _ = agent.step(&mut session, prompt).await?;
    Ok(())
}

async fn run_interactive_session(
    initial_prompt: Option<String>,
    cli: &Cli,
    config: &Config,
) -> Result<()> {
    print_lontar_banner();

    let mut session = get_or_create_session(cli, config)?;
    let mut provider_name = cli.provider.as_deref().unwrap_or(&config.default_provider);
    if !config.has_configured_provider(provider_name) {
        provider_name = "lontar-free";
    }
    let router = LlmRouter::new(provider_name, cli.model.as_deref(), config)?;
    let tools = ToolRegistry::new();
    let agent = Agent::new(Arc::new(router), tools, config.clone(), cli.yes);

    let db = Database::open_default().ok();
    let doc_count = if let Some(d) = &db {
        d.list_documents(&session.id).map(|v| v.len()).unwrap_or(0)
    } else {
        0
    };

    // OpenCode & Kiro Style Formatted Status Box
    println!("╭─────────────────────────────────────────────────────────────────────────╮");
    println!(
        "│ {} {:<56} │",
        "Sesi   :".bold().cyan(),
        format!("{} ({})", session.name.bold(), &session.id[..8]).white()
    );
    println!(
        "│ {} {:<56} │",
        "Model  :".bold().cyan(),
        format!("{} [{}]", config.get_model(provider_name).yellow().bold(), provider_name.green().bold())
    );
    println!(
        "│ {} {:<56} │",
        "Konteks:".bold().cyan(),
        format!("{} dokumen tergrounding │ Failover 8 Free AI Ready", doc_count).white()
    );
    println!("╰─────────────────────────────────────────────────────────────────────────╯");
    println!("{}", "Ketik pertanyaan Anda, '@file' untuk lampiran, '/help' untuk bantuan, atau '/exit'.".dimmed());
    println!();

    if let Some(prompt) = initial_prompt {
        println!("{}> {}", "lontar".bold().cyan(), prompt);
        let _ = agent.step(&mut session, &prompt).await?;
    }

    let mut rl = DefaultEditor::new()?;
    let history_file = Config::config_dir().join("history.txt");
    let _ = rl.load_history(&history_file);

    loop {
        let prompt_str = format!("{}> ", "lontar".bold().cyan());
        match rl.readline(&prompt_str) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);

                // Inline slash commands
                if trimmed.starts_with('/') {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    match parts[0] {
                        "/exit" | "/quit" => {
                            println!("{}", "Sampai jumpa! Sesi Anda telah tersimpan.".green());
                            break;
                        }
                        "/clear" => {
                            print!("\x1B[2J\x1B[1;1H");
                            continue;
                        }
                        "/help" => {
                            print_help();
                            continue;
                        }
                        "/model" => {
                            if parts.len() > 1 {
                                println!("Untuk mengganti model, jalankan: lontar config set default_provider {}", parts[1]);
                            } else {
                                print_supported_models(config);
                            }
                            continue;
                        }
                        "/context" => {
                            if parts.len() > 1 && parts[1] == "add" && parts.len() > 2 {
                                if let Some(d) = &db {
                                    for p in &parts[2..] {
                                        let _ = DocumentEngine::ingest_file(d, &session.id, p);
                                        println!("{} Dokumen '{}' ter-ingest ke konteks.", "✔".green(), p);
                                    }
                                }
                            } else if let Some(d) = &db {
                                let docs = d.list_documents(&session.id).unwrap_or_default();
                                println!("Grounded docs ({}):", docs.len());
                                for doc in docs {
                                    println!("- {} ({} chunks)", doc.file_path, doc.chunk_count);
                                }
                            }
                            continue;
                        }
                        "/test" => {
                            run_workspace_tests(&session.id).await;
                            continue;
                        }
                        _ => {
                            println!("Perintah '{}' tidak dikenal. Ketik /help untuk daftar perintah.", parts[0]);
                            continue;
                        }
                    }
                }

                // Regular chat turn
                if let Err(err) = agent.step(&mut session, trimmed).await {
                    eprintln!("{}", format!("Terjadi kesalahan: {:#}", err).red());
                }
                println!();
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("\n{}", "Sesi selesai. Sampai jumpa!".green());
                break;
            }
            Err(err) => {
                eprintln!("Readline error: {:#}", err);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_file);
    Ok(())
}

async fn run_workspace_tests(session_id: &str) {
    println!("{}", "🧪 Menjalankan uji verifikasi kode (cargo test)...".bold().cyan());
    let start = Instant::now();
    let output = std::process::Command::new("cargo")
        .arg("test")
        .output();

    let duration = start.elapsed().as_millis() as i64;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);
            let status = if out.status.success() { "PASS" } else { "FAIL" };

            if out.status.success() {
                println!("{} Seluruh unit test lulus dalam {} ms!", "✔".green().bold(), duration);
            } else {
                eprintln!("{} Pengujian gagal dalam {} ms!\n{}", "✖".red().bold(), duration, stderr);
            }

            if let Ok(db) = Database::open_default() {
                let _ = db.insert_test_log(&DbTestLog {
                    id: 0,
                    session_id: session_id.to_string(),
                    test_name: "cargo_test_workspace".to_string(),
                    status: status.to_string(),
                    output: combined,
                    duration_ms: duration,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
            }
        }
        Err(e) => {
            eprintln!("Gagal menjalankan cargo test: {}", e);
        }
    }
}

fn print_help() {
    println!("{}", "Perintah Interaktif LONTAR CLI:".bold().cyan());
    println!("  /help               Tampilkan bantuan ini");
    println!("  /clear              Bersihkan layar terminal");
    println!("  /model              Lihat daftar provider & model AI yang tersedia");
    println!("  /context            Lihat dokumen konteks yang tergrounding");
    println!("  /context add <file> Ingest file (.pdf, .md, .txt) ke konteks sesi");
    println!("  /test               Jalankan pengujian unit tes & catat ke SQLite");
    println!("  /exit, /quit        Keluar dari sesi interaktif");
    println!();
    println!("{}", "Fitur Pintar:".bold().yellow());
    println!("  Ketik '@nama_file' di pesan untuk otomatis menyertakan konten file ke prompt.");
}

fn get_or_create_session(cli: &Cli, config: &Config) -> Result<Session> {
    let mut provider = cli
        .provider
        .as_deref()
        .map(Config::canonical_provider_name)
        .unwrap_or_else(|| Config::canonical_provider_name(&config.default_provider));
    let mut model = cli
        .model
        .clone()
        .unwrap_or_else(|| config.get_model(&provider));

    // If active provider isn't configured (missing key for non-local/non-custom), fallback to lontar-free
    if !config.has_configured_provider(&provider) {
        provider = "lontar-free".to_string();
        model = config.lontar_free_model.clone();
    }

    if let Some(id_or_prefix) = &cli.session {
        SessionStore::load(id_or_prefix)
    } else if cli.r#continue {
        // Reuse most recent session if --continue passed
        let list = SessionStore::list().unwrap_or_default();
        if let Some(recent) = list.first() {
            if let Ok(sess) = SessionStore::load(&recent.id) {
                return Ok(sess);
            }
        }

        let sess = SessionStore::create(None, provider, model)?;
        if let Ok(db) = Database::open_default() {
            let _ = db.insert_session(&DbSession {
                id: sess.id.clone(),
                name: sess.name.clone(),
                created_at: sess.created_at.to_rfc3339(),
                updated_at: sess.updated_at.to_rfc3339(),
                provider: sess.provider.clone(),
                model: sess.model.clone(),
                total_tokens: 0,
            });
        }
        Ok(sess)
    } else {
        // By default, start a clean fresh session on the OpenCode home screen!
        let sess = SessionStore::create(None, provider, model)?;
        if let Ok(db) = Database::open_default() {
            let _ = db.insert_session(&DbSession {
                id: sess.id.clone(),
                name: sess.name.clone(),
                created_at: sess.created_at.to_rfc3339(),
                updated_at: sess.updated_at.to_rfc3339(),
                provider: sess.provider.clone(),
                model: sess.model.clone(),
                total_tokens: 0,
            });
        }
        Ok(sess)
    }
}

fn mask_key(key: &Option<String>) -> String {
    match key {
        Some(k) if k.len() > 8 => format!("{}...{}", &k[..4], &k[k.len() - 4..]),
        Some(_) => "********".to_string(),
        None => "(belum diset)".dimmed().to_string(),
    }
}
