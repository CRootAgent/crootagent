use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub default_provider: String,
    pub auto_confirm: bool,
    pub max_iterations: usize,
    pub system_prompt: Option<String>,

    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: String,

    pub anthropic_api_key: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub anthropic_model: String,

    pub gemini_api_key: Option<String>,
    pub gemini_base_url: Option<String>,
    pub gemini_model: String,

    pub groq_api_key: Option<String>,
    pub groq_base_url: Option<String>,
    pub groq_model: String,

    pub ollama_base_url: Option<String>,
    pub ollama_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: "openai".to_string(),
            auto_confirm: false,
            max_iterations: 25,
            system_prompt: None,

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            openai_base_url: std::env::var("OPENAI_BASE_URL").ok(),
            openai_model: "gpt-4o".to_string(),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
            anthropic_model: "claude-3-5-sonnet-20241022".to_string(),

            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_base_url: std::env::var("GEMINI_BASE_URL").ok(),
            gemini_model: "gemini-1.5-pro".to_string(),

            groq_api_key: std::env::var("GROQ_API_KEY").ok(),
            groq_base_url: std::env::var("GROQ_BASE_URL").ok(),
            groq_model: "llama-3.3-70b-versatile".to_string(),

            ollama_base_url: std::env::var("OLLAMA_HOST").ok().or_else(|| Some("http://localhost:11434".to_string())),
            ollama_model: "llama3.2".to_string(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("id", "crootagent", "crootagent") {
            proj_dirs.config_dir().to_path_buf()
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config").join("crootagent")
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        let mut config = if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Gagal membaca config dari {:?}", path))?;
            toml::from_str::<Config>(&content)
                .with_context(|| "Gagal parse format config.toml")?
        } else {
            let def = Config::default();
            let _ = def.save();
            def
        };

        // Environment variables override config file values if present
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            config.openai_api_key = Some(key);
        }
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            config.anthropic_api_key = Some(key);
        }
        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            config.gemini_api_key = Some(key);
        }
        if let Ok(key) = std::env::var("GROQ_API_KEY") {
            config.groq_api_key = Some(key);
        }
        if let Ok(url) = std::env::var("OLLAMA_HOST") {
            config.ollama_base_url = Some(url);
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("Gagal membuat direktori config {:?}", dir))?;
        let toml_str = toml::to_string_pretty(self)
            .with_context(|| "Gagal serialize config to TOML")?;
        fs::write(Self::config_path(), toml_str)
            .with_context(|| "Gagal menulis file config.toml")?;
        Ok(())
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "default_provider" => self.default_provider = value.to_string(),
            "auto_confirm" => self.auto_confirm = value.parse().unwrap_or(false),
            "max_iterations" => self.max_iterations = value.parse().unwrap_or(25),
            "openai_api_key" => self.openai_api_key = Some(value.to_string()),
            "openai_model" => self.openai_model = value.to_string(),
            "openai_base_url" => self.openai_base_url = Some(value.to_string()),
            "anthropic_api_key" => self.anthropic_api_key = Some(value.to_string()),
            "anthropic_model" => self.anthropic_model = value.to_string(),
            "anthropic_base_url" => self.anthropic_base_url = Some(value.to_string()),
            "gemini_api_key" => self.gemini_api_key = Some(value.to_string()),
            "gemini_model" => self.gemini_model = value.to_string(),
            "gemini_base_url" => self.gemini_base_url = Some(value.to_string()),
            "groq_api_key" => self.groq_api_key = Some(value.to_string()),
            "groq_model" => self.groq_model = value.to_string(),
            "ollama_base_url" => self.ollama_base_url = Some(value.to_string()),
            "ollama_model" => self.ollama_model = value.to_string(),
            _ => anyhow::bail!("Key config '{}' tidak dikenali. Pilihan: default_provider, auto_confirm, openai_api_key, anthropic_api_key, gemini_api_key, groq_api_key, ollama_base_url, dll.", key),
        }
        self.save()
    }

    #[allow(dead_code)]
    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        match provider.to_lowercase().as_str() {
            "openai" => self.openai_api_key.clone(),
            "anthropic" | "claude" => self.anthropic_api_key.clone(),
            "gemini" | "google" => self.gemini_api_key.clone(),
            "groq" => self.groq_api_key.clone(),
            "ollama" => Some("ollama_local".to_string()),
            _ => None,
        }
    }

    pub fn get_model(&self, provider: &str) -> String {
        match provider.to_lowercase().as_str() {
            "openai" => self.openai_model.clone(),
            "anthropic" | "claude" => self.anthropic_model.clone(),
            "gemini" | "google" => self.gemini_model.clone(),
            "groq" => self.groq_model.clone(),
            "ollama" => self.ollama_model.clone(),
            _ => "default-model".to_string(),
        }
    }
}
