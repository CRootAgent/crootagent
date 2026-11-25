use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "lontar",
    author = "Ahmad Zakky Nurfauzan <ahmadzakky2862@gmail.com>",
    version = "0.2.0",
    about = "Ultra-lightweight CLI AI Agent with NotebookLM document integration, universal free LLM routing, and OpenCode-inspired terminal UX",
    long_about = "LONTAR CLI is an ultra-lightweight, resilient AI Agent built in Rust.\nFeatures deep document grounding (NotebookLM-in-terminal), instant access to universal free LLMs (Groq, Gemini, DeepSeek, OpenRouter, GitHub Models, SambaNova, Mistral, Ollama) with auto-failover, and OpenCode-inspired terminal UI."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Provider to use (groq, gemini, openrouter, deepseek, github, sambanova, mistral, ollama, openai, anthropic)
    #[arg(short, long, global = true)]
    pub provider: Option<String>,

    /// Model name to use (e.g. llama-3.3-70b-versatile, gemini-1.5-flash, deepseek-chat, gpt-4o-mini)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Session ID or name to resume
    #[arg(short, long, global = true)]
    pub session: Option<String>,

    /// Continue the most recent session
    #[arg(short = 'c', long, global = true)]
    pub r#continue: bool,

    /// Auto-confirm tool executions without prompting (Human-In-The-Loop bypass)
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    /// One-shot prompt to execute without entering interactive mode
    #[arg(trailing_var_arg = true)]
    pub prompt: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Execute a task directly in one shot
    Run(RunArgs),

    /// Start an interactive chat REPL session (OpenCode / Kiro style TUI)
    Chat(ChatArgs),

    /// Manage multi-API chat sessions
    Session(SessionArgs),

    /// Manage provider authentication and API keys (OpenCode parity: 'lontar auth login')
    Auth(AuthArgs),

    /// View or update Lontar configuration
    Config(ConfigArgs),

    /// List supported free and commercial providers and models
    Models,

    /// Ingest and manage NotebookLM grounded document context
    Context(ContextArgs),

    /// Synthesize a structured Study Guide from grounded documents
    StudyGuide(StudyGuideArgs),

    /// Synthesize a Product Requirement Document (PRD.md)
    Prd(PrdArgs),

    /// Synthesize an actionable technical Tasklist (TASKLIST.md)
    Tasklist(TasklistArgs),

    /// Inspect local SQLite CRUD database
    Db(DbArgs),

    /// View guide and tutorials to obtain free API keys from AI providers
    Guide(GuideArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    /// The prompt or instruction for the agent
    #[arg(required = true)]
    pub prompt: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ChatArgs {
    /// Initial prompt to start the session with
    pub initial_prompt: Option<String>,

    /// Run in classic terminal REPL mode instead of full Ratatui TUI
    #[arg(long)]
    pub classic: bool,
}

#[derive(Args, Debug)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub action: SessionCommands,
}

#[derive(Subcommand, Debug)]
pub enum SessionCommands {
    /// List all saved sessions
    List,
    /// Create a new session
    New {
        /// Optional name for the new session
        name: Option<String>,
    },
    /// Show details and history of a session
    Show {
        /// Session ID
        id: String,
    },
    /// Delete a session
    Delete {
        /// Session ID
        id: String,
    },
    /// Export session history as Markdown
    Export {
        /// Session ID
        id: String,
        /// Output file path (defaults to lontar-session-id.md)
        output: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: Option<ConfigCommands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a configuration value (e.g. lontar config set default_provider groq)
    Set {
        key: String,
        value: String,
    },
    /// Initialize or reset configuration
    Init,
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub action: ContextCommands,
}

#[derive(Subcommand, Debug)]
pub enum ContextCommands {
    /// Ingest one or more documents (.pdf, .md, .txt, code files) into active session
    Add {
        /// File paths to ingest
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// List grounded documents and chunks in the active session
    List,
    /// Clear all grounded documents in the active session
    Clear,
}

#[derive(Args, Debug)]
pub struct StudyGuideArgs {
    /// Topic or learning objective
    #[arg(required = true)]
    pub topic: String,

    /// Optional specific file to ground the study guide on
    #[arg(short, long)]
    pub file: Option<String>,

    /// Output file path (defaults to STUDY_GUIDE.md)
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Args, Debug)]
pub struct PrdArgs {
    /// Feature idea or problem statement
    #[arg(required = true)]
    pub idea: String,

    /// Output file path (defaults to PRD.md)
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Args, Debug)]
pub struct TasklistArgs {
    /// Path to PRD file or prompt
    #[arg(required = true)]
    pub source: String,

    /// Output file path (defaults to TASKLIST.md)
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(Args, Debug)]
pub struct DbArgs {
    #[command(subcommand)]
    pub action: DbCommands,
}

#[derive(Subcommand, Debug)]
pub enum DbCommands {
    /// Show SQLite database statistics and record counts
    Stats,
    /// List registered free LLM providers in SQLite
    Providers,
    /// List test execution logs
    Logs,
}

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: Option<AuthCommands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum AuthCommands {
    /// Connect a provider interactively (OpenCode 'auth login' style)
    Login {
        /// Provider name (gemini, groq, openai, anthropic, deepseek, openrouter, etc.)
        provider: Option<String>,
    },
    /// Register a custom OpenAI-compatible provider
    Custom {
        /// Custom provider name (e.g. cerebras, together, vllm)
        name: String,
        /// Base URL (e.g. https://api.cerebras.ai/v1)
        base_url: String,
        /// API Key
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        /// Model name (default: default)
        #[arg(short = 'm', long)]
        model: Option<String>,
    },
    /// View guide and tutorials to obtain free API keys
    Guide {
        /// Specific provider to view guide for
        provider: Option<String>,
    },
    /// List authenticated providers and credentials
    List,
    /// Disconnect a provider and remove stored API key
    Logout {
        /// Provider name to disconnect
        provider: String,
    },
}

#[derive(Args, Debug, Clone)]
pub struct GuideArgs {
    /// Specific provider to view guide for (gemini, groq, openrouter, nvidia, cerebras, etc.)
    pub provider: Option<String>,
}
