use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "crootagent",
    author = "Ahmad Zakky Nurfauzan <ahmadzakky2862@gmail.com>",
    version = "0.1.0",
    about = "Ultra-lightweight CLI coding agent with multi-API session management and developer tool execution",
    long_about = "CRootAgent is a blazingly fast, lightweight agentic coding assistant built in Rust.\nSupports multiple LLM providers (OpenAI, Anthropic Claude, Google Gemini, Groq, Ollama), multi-session management, and sandboxed tool calling (file editing, terminal commands, directory listing)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Provider to use (openai, anthropic, gemini, groq, ollama)
    #[arg(short, long, global = true)]
    pub provider: Option<String>,

    /// Model name to use (e.g. gpt-4o, claude-3-5-sonnet, gemini-1.5-pro, llama-3.3-70b)
    #[arg(short, long, global = true)]
    pub model: Option<String>,

    /// Session ID or name to resume
    #[arg(short, long, global = true)]
    pub session: Option<String>,

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

    /// Start an interactive chat REPL session
    Chat(ChatArgs),

    /// Manage multi-API chat sessions
    Session(SessionArgs),

    /// View or update CRootAgent configuration
    Config(ConfigArgs),

    /// List supported providers and default models
    Models,
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
        /// Output file path (defaults to session-id.md)
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
    /// Set a configuration value (e.g. crootagent config set default_provider openai)
    Set {
        key: String,
        value: String,
    },
    /// Initialize or reset configuration
    Init,
}
