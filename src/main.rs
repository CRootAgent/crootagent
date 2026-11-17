use anyhow::Result;
use clap::Parser;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crootagent::agent::Agent;
use crootagent::cli::{Cli, Commands, ConfigCommands, SessionCommands};
use crootagent::config::Config;
use crootagent::provider::create_provider;
use crootagent::session::SessionStore;
use crootagent::tools::ToolRegistry;

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
        Some(Commands::Models) => {
            print_supported_models(&config);
            return Ok(());
        }
        Some(Commands::Session(args)) => {
            handle_session_command(&args.action, &config)?;
            return Ok(());
        }
        Some(Commands::Run(args)) => {
            let prompt = args.prompt.join(" ");
            run_single_task(&prompt, &cli, &config).await?;
            return Ok(());
        }
        Some(Commands::Chat(args)) => {
            run_interactive_session(args.initial_prompt.clone(), &cli, &config).await?;
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

    // Default to interactive session
    run_interactive_session(None, &cli, &config).await?;
    Ok(())
}

fn handle_config_command(action: Option<ConfigCommands>, config: &mut Config) -> Result<()> {
    match action.unwrap_or(ConfigCommands::Show) {
        ConfigCommands::Show => {
            println!("{}", "═══════════════════════════════════════════".cyan());
            println!("{}", "       🔧 Konfigurasi CRootAgent          ".bold().cyan());
            println!("{}", "═══════════════════════════════════════════".cyan());
            println!("Path Config: {:?}", Config::config_path());
            println!("Provider Default : {}", config.default_provider.green());
            println!("Auto Confirm     : {}", if config.auto_confirm { "Ya".green() } else { "Tidak (HITL Aktif)".yellow() });
            println!("Max Iterations   : {}", config.max_iterations);
            println!("───────────────────────────────────────────");
            println!("OpenAI Model     : {}", config.openai_model);
            println!("OpenAI API Key   : {}", mask_key(&config.openai_api_key));
            println!("Anthropic Model  : {}", config.anthropic_model);
            println!("Anthropic API Key: {}", mask_key(&config.anthropic_api_key));
            println!("Gemini Model     : {}", config.gemini_model);
            println!("Gemini API Key   : {}", mask_key(&config.gemini_api_key));
            println!("Groq Model       : {}", config.groq_model);
            println!("Groq API Key     : {}", mask_key(&config.groq_api_key));
            println!("Ollama URL       : {:?}", config.ollama_base_url);
            println!("Ollama Model     : {}", config.ollama_model);
            println!("{}", "═══════════════════════════════════════════".cyan());
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
            println!("{}", "══════════════════════════════════════════════════════════════════════".cyan());
            println!("{}", "                       📋 Daftar Sesi CRootAgent                     ".bold().cyan());
            println!("{}", "══════════════════════════════════════════════════════════════════════".cyan());
            println!("{:<36} {:<15} {:<12} {:<6} {:<20}", "ID Sesi", "Nama", "Provider", "Pesan", "Terakhir Diperbarui");
            println!("{}", "──────────────────────────────────────────────────────────────────────".dimmed());
            for s in list {
                println!(
                    "{:<36} {:<15} {:<12} {:<6} {:<20}",
                    s.id.dimmed(),
                    s.name.bold(),
                    s.provider.green(),
                    s.message_count,
                    s.updated_at.format("%Y-%m-%d %H:%M").to_string().dimmed()
                );
            }
            println!("{}", "══════════════════════════════════════════════════════════════════════".cyan());
        }
        SessionCommands::New { name } => {
            let provider = config.default_provider.clone();
            let model = config.get_model(&provider);
            let sess = SessionStore::create(name.clone(), provider, model)?;
            println!("{} Sesi baru berhasil dibuat: {} ({})", "✔".green().bold(), sess.name.bold(), sess.id.cyan());
        }
        SessionCommands::Show { id } => {
            let sess = SessionStore::load(id)?;
            println!("Sesi: {} ({})", sess.name.bold(), sess.id.dimmed());
            println!("Provider: {} | Model: {}", sess.provider.green(), sess.model.yellow());
            println!("Jumlah pesan: {}", sess.messages.len());
            println!("───────────────────────────────────────────");
            for msg in sess.messages {
                let badge = match msg.role.as_str() {
                    "user" => "👤 User".blue().bold(),
                    "assistant" => "🤖 Assistant".green().bold(),
                    "system" => "⚙️ System".dimmed(),
                    "tool" => "🔧 Tool".magenta(),
                    _ => msg.role.normal(),
                };
                let content = msg.content.unwrap_or_else(|| "[Tool Call / Output]".to_string());
                println!("{}: {}", badge, content);
            }
        }
        SessionCommands::Delete { id } => {
            SessionStore::delete(id)?;
            println!("{} Sesi '{}' berhasil dihapus.", "✔".green().bold(), id);
        }
        SessionCommands::Export { id, output } => {
            let sess = SessionStore::load(id)?;
            let out_path = SessionStore::export_markdown(&sess, output.clone())?;
            println!("{} Sesi berhasil diekspor ke: {:?}", "✔".green().bold(), out_path);
        }
    }
    Ok(())
}

fn print_supported_models(config: &Config) {
    println!("{}", "════════════════════════════════════════════════════════════════".cyan());
    println!("{}", "           🌐 Provider & Model yang Didukung CRootAgent         ".bold().cyan());
    println!("{}", "════════════════════════════════════════════════════════════════".cyan());
    println!("{:<12} {:<25} {:<20}", "Provider", "Model Rekomendasi", "Variabel Lingkungan");
    println!("{}", "────────────────────────────────────────────────────────────────".dimmed());
    println!("{:<12} {:<25} {:<20}", "openai", config.openai_model, "OPENAI_API_KEY");
    println!("{:<12} {:<25} {:<20}", "anthropic", config.anthropic_model, "ANTHROPIC_API_KEY");
    println!("{:<12} {:<25} {:<20}", "gemini", config.gemini_model, "GEMINI_API_KEY");
    println!("{:<12} {:<25} {:<20}", "groq", config.groq_model, "GROQ_API_KEY");
    println!("{:<12} {:<25} {:<20}", "ollama", config.ollama_model, "OLLAMA_HOST");
    println!("{}", "════════════════════════════════════════════════════════════════".cyan());
}

async fn run_single_task(prompt: &str, cli: &Cli, config: &Config) -> Result<()> {
    let provider_name = cli
        .provider
        .as_deref()
        .unwrap_or(&config.default_provider);

    let provider = create_provider(provider_name, cli.model.as_deref(), config)?;
    let tools = ToolRegistry::default();
    let agent = Agent::new(provider.clone(), tools, config.clone(), cli.yes);

    let mut session = if let Some(sess_id) = &cli.session {
        SessionStore::load(sess_id)?
    } else {
        SessionStore::create(
            Some("Task-OneShot".to_string()),
            provider.name().to_string(),
            provider.model().to_string(),
        )?
    };

    println!("{}", format!("🚀 [CRootAgent] Menjalankan tugas: \"{}\"", prompt).bold());
    agent.step(&mut session, prompt).await?;
    Ok(())
}

async fn run_interactive_session(
    initial_prompt: Option<String>,
    cli: &Cli,
    config: &Config,
) -> Result<()> {
    print_banner();

    let mut current_provider_name = cli
        .provider
        .clone()
        .unwrap_or_else(|| config.default_provider.clone());

    let current_model_override = cli.model.clone();

    let mut session = if let Some(sess_id) = &cli.session {
        SessionStore::load(sess_id)?
    } else {
        let provider = create_provider(&current_provider_name, current_model_override.as_deref(), config)?;
        SessionStore::create(
            None,
            provider.name().to_string(),
            provider.model().to_string(),
        )?
    };

    println!(
        "{} {} ({})",
        "Sesi Aktif :".bold(),
        session.name.cyan(),
        session.id.dimmed()
    );
    println!(
        "{} {} | {} {}",
        "Provider   :".bold(),
        session.provider.green(),
        "Model:".bold(),
        session.model.yellow()
    );
    println!("Ketik '/help' untuk bantuan, '/exit' untuk keluar.\n");

    let tools = ToolRegistry::default();
    let mut rl = DefaultEditor::new()?;

    // Execute initial prompt if provided
    if let Some(init) = initial_prompt {
        let provider = create_provider(&session.provider, Some(&session.model), config)?;
        let agent = Agent::new(provider, tools.clone(), config.clone(), cli.yes);
        agent.step(&mut session, &init).await?;
    }

    loop {
        let prompt_str = format!("{}> ", "crootagent".bold().cyan());
        match rl.readline(&prompt_str) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);

                // Handle slash commands
                if trimmed.starts_with('/') {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    match parts[0] {
                        "/exit" | "/quit" | "/q" => {
                            println!("{}", "Sampai jumpa! Sesi Anda tersimpan otomatis.".green());
                            break;
                        }
                        "/help" => {
                            print_interactive_help();
                            continue;
                        }
                        "/session" => {
                            if parts.len() > 1 {
                                match SessionStore::load(parts[1]) {
                                    Ok(loaded) => {
                                        session = loaded;
                                        println!("{} Beralih ke sesi: {}", "✔".green(), session.name.bold());
                                    }
                                    Err(e) => eprintln!("{} {}", "❌".red(), e),
                                }
                            } else {
                                println!("ID Sesi saat ini: {}", session.id.cyan());
                            }
                            continue;
                        }
                        "/provider" => {
                            if parts.len() > 1 {
                                current_provider_name = parts[1].to_string();
                                session.provider = current_provider_name.clone();
                                session.model = config.get_model(&current_provider_name);
                                SessionStore::save(&session)?;
                                println!("{} Provider diganti ke: {} ({})", "✔".green(), session.provider.green(), session.model.yellow());
                            } else {
                                println!("Provider saat ini: {}", session.provider.green());
                            }
                            continue;
                        }
                        "/model" => {
                            if parts.len() > 1 {
                                session.model = parts[1].to_string();
                                SessionStore::save(&session)?;
                                println!("{} Model diganti ke: {}", "✔".green(), session.model.yellow());
                            } else {
                                println!("Model saat ini: {}", session.model.yellow());
                            }
                            continue;
                        }
                        "/clear" => {
                            session.messages.clear();
                            SessionStore::save(&session)?;
                            println!("{} Riwayat percakapan sesi telah dibersihkan.", "✔".green());
                            continue;
                        }
                        "/export" => {
                            let out_path = SessionStore::export_markdown(&session, None)?;
                            println!("{} Sesi diekspor ke: {:?}", "✔".green(), out_path);
                            continue;
                        }
                        _ => {
                            println!("{}", "Perintah tidak dikenali. Ketik '/help' untuk melihat daftar perintah.".yellow());
                            continue;
                        }
                    }
                }

                // Normal user prompt
                let provider = match create_provider(&session.provider, Some(&session.model), config) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("{} Inisialisasi provider gagal: {:#}", "❌".red(), e);
                        continue;
                    }
                };

                let agent = Agent::new(provider, tools.clone(), config.clone(), cli.yes);
                if let Err(e) = agent.step(&mut session, trimmed).await {
                    eprintln!("{} Eksekusi agen gagal: {:#}", "❌".red(), e);
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("\n{}", "Sesi selesai. Data tersimpan.".green());
                break;
            }
            Err(err) => {
                eprintln!("Error membaca input: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}

fn print_banner() {
    println!("{}", r#"
   ____ ____                  _      _                    _   
  / ___|  _ \ ___   ___  _ |_|__   / \   __ _  ___ _ __  | |_ 
 | |   | |_) / _ \ / _ \| __/ __| / _ \ / _` |/ _ \ '_ \ | __|
 | |___|  _ < (_) | (_) | || (__ / ___ \ (_| |  __/ | | || |_ 
  \____|_| \_\___/ \___/ \__\___/_/   \_\__, |\___|_| |_| \__|
                                         |___/                 
"#.bold().cyan());
    println!(
        "{} v0.1.0 — {}",
        "CRootAgent".bold(),
        "Ultra-Lightweight CLI AI Coding Assistant (Rust)".dimmed()
    );
    println!("Dibuat oleh: Ahmad Zakky Nurfauzan (714250028) | D4TI ULBI Bandung\n");
}

fn print_interactive_help() {
    println!("{}", "Perintah REPL Interaktif:".bold());
    println!("  /help                 Menampilkan pesan bantuan ini");
    println!("  /provider <nama>      Beralih provider (openai, anthropic, gemini, groq, ollama)");
    println!("  /model <nama>         Beralih model untuk provider aktif");
    println!("  /session <id/nama>    Beralih atau menampilkan sesi aktif");
    println!("  /export               Ekspor riwayat sesi ke file Markdown");
    println!("  /clear                Hapus riwayat pesan pada sesi ini");
    println!("  /exit, /quit, /q      Keluar dari aplikasi");
}

fn mask_key(opt: &Option<String>) -> String {
    match opt {
        Some(k) if k.len() > 8 => format!("{}****{}", &k[..4], &k[k.len() - 4..]),
        Some(_) => "********".to_string(),
        None => "[Belum diatur]".dimmed().to_string(),
    }
}
