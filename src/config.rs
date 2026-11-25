use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_provider: String,
    pub lontar_free_model: String,
    pub auto_confirm: bool,
    pub max_iterations: usize,
    pub system_prompt: Option<String>,

    // Groq (Free fast inference)
    pub groq_api_key: Option<String>,
    pub groq_base_url: Option<String>,
    pub groq_model: String,

    // Google Gemini (Free API tier)
    pub gemini_api_key: Option<String>,
    pub gemini_base_url: Option<String>,
    pub gemini_model: String,

    // OpenRouter (Free community models)
    pub openrouter_api_key: Option<String>,
    pub openrouter_base_url: Option<String>,
    pub openrouter_model: String,

    // DeepSeek (V3 / R1 / V4)
    pub deepseek_api_key: Option<String>,
    pub deepseek_base_url: Option<String>,
    pub deepseek_model: String,

    // GitHub Models (Free PAT tier)
    pub github_token: Option<String>,
    pub github_base_url: Option<String>,
    pub github_model: String,

    // SambaNova (Fast Llama-3.3 70B)
    pub sambanova_api_key: Option<String>,
    pub sambanova_base_url: Option<String>,
    pub sambanova_model: String,

    // Mistral NeMo
    pub mistral_api_key: Option<String>,
    pub mistral_base_url: Option<String>,
    pub mistral_model: String,

    // Ollama Local Offline
    pub ollama_base_url: Option<String>,
    pub ollama_model: String,

    // Commercial options (OpenAI & Anthropic)
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: String,

    pub anthropic_api_key: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub anthropic_model: String,

    // Custom OpenAI-compatible Providers (e.g. z.ai, cerebras, together, vLLM, LMStudio, etc.)
    pub custom_providers: std::collections::HashMap<String, CustomProviderConfig>,

    // Per-provider models dynamically synced from live `/models` endpoints
    pub synced_models: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelSyncOutcome {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub total_live: usize,
    pub switched_default_to: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: "lontar-free".to_string(),
            lontar_free_model: "openai-fast".to_string(),
            auto_confirm: false,
            max_iterations: 25,
            system_prompt: None,

            groq_api_key: std::env::var("GROQ_API_KEY").ok(),
            groq_base_url: std::env::var("GROQ_BASE_URL").ok(),
            groq_model: "llama-3.3-70b-versatile".to_string(),

            gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            gemini_base_url: std::env::var("GEMINI_BASE_URL").ok(),
            gemini_model: "gemini-3.5-flash-lite".to_string(),

            openrouter_api_key: std::env::var("OPENROUTER_API_KEY").ok(),
            openrouter_base_url: std::env::var("OPENROUTER_BASE_URL").ok(),
            openrouter_model: "meta-llama/llama-3.3-70b-instruct:free".to_string(),

            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
            deepseek_base_url: std::env::var("DEEPSEEK_BASE_URL").ok(),
            deepseek_model: "deepseek-flash".to_string(),

            github_token: std::env::var("GITHUB_TOKEN").ok(),
            github_base_url: std::env::var("GITHUB_BASE_URL").ok(),
            github_model: "gpt-4o-mini".to_string(),

            sambanova_api_key: std::env::var("SAMBANOVA_API_KEY").ok(),
            sambanova_base_url: std::env::var("SAMBANOVA_BASE_URL").ok(),
            sambanova_model: "Meta-Llama-3.3-70B-Instruct".to_string(),

            mistral_api_key: std::env::var("MISTRAL_API_KEY").ok(),
            mistral_base_url: std::env::var("MISTRAL_BASE_URL").ok(),
            mistral_model: "mistral-nemo".to_string(),

            ollama_base_url: std::env::var("OLLAMA_HOST").ok().or_else(|| Some("http://localhost:11434/v1".to_string())),
            ollama_model: "llama3.2".to_string(),

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            openai_base_url: std::env::var("OPENAI_BASE_URL").ok(),
            openai_model: "gpt-4o-mini".to_string(),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
            anthropic_model: "claude-3-5-sonnet-20241022".to_string(),

            custom_providers: std::collections::HashMap::new(),
            synced_models: std::collections::HashMap::new(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        if let Ok(override_dir) = std::env::var("LONTAR_CONFIG_DIR") {
            if !override_dir.trim().is_empty() {
                return PathBuf::from(override_dir);
            }
        }
        if let Some(proj_dirs) = ProjectDirs::from("id", "lontar", "lontar") {
            proj_dirs.config_dir().to_path_buf()
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config").join("lontar")
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let mut config = if path.exists() {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Gagal membaca config dari {:?}", path))?;
            toml::from_str::<Config>(&content)
                .with_context(|| "Gagal parse format config.toml")?
        } else {
            let def = Config::default();
            let _ = def.save_to(path);
            def
        };

        // Environment variables override config file values only when non-empty
        let env_non_empty = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        if let Some(key) = env_non_empty("GROQ_API_KEY") {
            config.groq_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("GEMINI_API_KEY") {
            config.gemini_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("OPENROUTER_API_KEY") {
            config.openrouter_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("DEEPSEEK_API_KEY") {
            config.deepseek_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("GITHUB_TOKEN") {
            config.github_token = Some(key);
        }
        if let Some(key) = env_non_empty("SAMBANOVA_API_KEY") {
            config.sambanova_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("MISTRAL_API_KEY") {
            config.mistral_api_key = Some(key);
        }
        if let Some(url) = env_non_empty("OLLAMA_HOST") {
            config.ollama_base_url = Some(url);
        }
        if let Some(key) = env_non_empty("OPENAI_API_KEY") {
            config.openai_api_key = Some(key);
        }
        if let Some(key) = env_non_empty("ANTHROPIC_API_KEY") {
            config.anthropic_api_key = Some(key);
        }

        Ok(config)
    }

    /// Normalize provider aliases into a single canonical ID.
    pub fn canonical_provider_name(provider: &str) -> String {
        match provider.trim().to_lowercase().as_str() {
            "lontar-free" | "free" | "lontar" | "zero" => "lontar-free".to_string(),
            "gemini" | "google" => "gemini".to_string(),
            "sambanova" | "samba" => "sambanova".to_string(),
            "anthropic" | "claude" => "anthropic".to_string(),
            "z.ai" | "zai" | "zhipu" | "bigmodel" => "z.ai".to_string(),
            other => other.to_string(),
        }
    }

    pub fn is_builtin_provider(provider: &str) -> bool {
        matches!(
            Self::canonical_provider_name(provider).as_str(),
            "lontar-free"
                | "groq"
                | "gemini"
                | "openrouter"
                | "deepseek"
                | "github"
                | "sambanova"
                | "mistral"
                | "ollama"
                | "openai"
                | "anthropic"
        )
    }

    pub fn has_configured_provider(&self, provider: &str) -> bool {
        let p = Self::canonical_provider_name(provider);
        p == "ollama"
            || p == "lontar-free"
            || self.get_api_key(&p).is_some()
            || self.custom_providers.contains_key(&p)
    }

    pub fn save(&self) -> Result<()> {
        Self::save_to(self, &Self::config_path())
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("Gagal membuat direktori config {:?}", dir))?;
        }
        let toml_str = toml::to_string_pretty(self)
            .with_context(|| "Gagal serialize config to TOML")?;
        fs::write(path, toml_str)
            .with_context(|| format!("Gagal menulis file config ke {:?}", path))?;
        Ok(())
    }

    /// Set the saved default model for a built-in or custom provider in memory.
    pub fn set_provider_model_in_memory(&mut self, provider: &str, model: &str) {
        let canonical = Self::canonical_provider_name(provider);
        let m = model.trim();
        if m.is_empty() {
            return;
        }
        match canonical.as_str() {
            "lontar-free" => self.lontar_free_model = m.to_string(),
            "groq" => self.groq_model = m.to_string(),
            "gemini" => self.gemini_model = m.to_string(),
            "openrouter" => self.openrouter_model = m.to_string(),
            "deepseek" => self.deepseek_model = m.to_string(),
            "github" => self.github_model = m.to_string(),
            "sambanova" => self.sambanova_model = m.to_string(),
            "mistral" => self.mistral_model = m.to_string(),
            "ollama" => self.ollama_model = m.to_string(),
            "openai" => self.openai_model = m.to_string(),
            "anthropic" => self.anthropic_model = m.to_string(),
            _ => {
                if let Some(custom) = self.custom_providers.get_mut(&canonical) {
                    custom.model = m.to_string();
                }
            }
        }
    }

    pub fn get_synced_models(&self, provider: &str) -> Option<&Vec<String>> {
        let canonical = Self::canonical_provider_name(provider);
        self.synced_models.get(&canonical)
    }

    /// Replace a provider's active model list with live-fetched models in memory,
    /// returning which models were added/removed and automatically switching the
    /// provider's default model if the previous model is no longer available.
    pub fn sync_provider_models_in_memory(
        &mut self,
        provider: &str,
        live_chat_models: &[String],
        baseline_static_models: &[&str],
    ) -> ModelSyncOutcome {
        let canonical = Self::canonical_provider_name(provider);

        let previous_list: Vec<String> = if let Some(prev) = self.synced_models.get(&canonical) {
            prev.clone()
        } else {
            let mut base: Vec<String> = baseline_static_models.iter().map(|s| s.to_string()).collect();
            let current = self.get_model(&canonical);
            if !current.is_empty()
                && current != "default-model"
                && !base.iter().any(|x| x.eq_ignore_ascii_case(&current))
            {
                base.push(current);
            }
            base
        };

        let mut dedup_live: Vec<String> = Vec::new();
        for m in live_chat_models {
            let trimmed = m.trim();
            if !trimmed.is_empty() && !dedup_live.iter().any(|x| x.eq_ignore_ascii_case(trimmed)) {
                dedup_live.push(trimmed.to_string());
            }
        }

        let added: Vec<String> = dedup_live
            .iter()
            .filter(|m| !previous_list.iter().any(|p| p.eq_ignore_ascii_case(m)))
            .cloned()
            .collect();

        let removed: Vec<String> = previous_list
            .iter()
            .filter(|p| !dedup_live.iter().any(|m| m.eq_ignore_ascii_case(p)))
            .cloned()
            .collect();

        self.synced_models.insert(canonical.clone(), dedup_live.clone());

        let current_model = self.get_model(&canonical);
        let switched_default_to = if !dedup_live.is_empty()
            && !dedup_live.iter().any(|m| m.eq_ignore_ascii_case(&current_model))
        {
            if let Some(best) = crate::provider::pick_preferred_chat_model(&dedup_live) {
                self.set_provider_model_in_memory(&canonical, &best);
                Some(best)
            } else {
                None
            }
        } else {
            None
        };

        ModelSyncOutcome {
            added,
            removed,
            total_live: dedup_live.len(),
            switched_default_to,
        }
    }

    pub fn sync_provider_models(
        &mut self,
        provider: &str,
        live_chat_models: &[String],
        baseline_static_models: &[&str],
    ) -> Result<ModelSyncOutcome> {
        let outcome = self.sync_provider_models_in_memory(provider, live_chat_models, baseline_static_models);
        self.save()?;
        Ok(outcome)
    }

    /// Update active default_provider and model in memory without touching disk.
    pub fn activate_provider_in_memory(&mut self, provider: &str, model: Option<&str>) {
        let canonical = Self::canonical_provider_name(provider);
        self.default_provider = canonical.clone();
        if let Some(m) = model {
            self.set_provider_model_in_memory(&canonical, m);
        }
    }

    /// Persist active default_provider and model cleanly to disk so restarts preserve user choice.
    pub fn activate_provider_and_save(&mut self, provider: &str, model: Option<&str>) -> Result<()> {
        self.activate_provider_in_memory(provider, model);
        self.save()
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        self.set_value_in_memory(key, value)?;
        self.save()
    }

    pub fn set_value_in_memory(&mut self, key: &str, value: &str) -> Result<()> {
        let trimmed = value.trim();
        let opt_val = || -> Option<String> {
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        match key {
            "default_provider" => self.default_provider = Self::canonical_provider_name(trimmed),
            "lontar_free_model" | "lontar-free_model" => self.lontar_free_model = trimmed.to_string(),
            "auto_confirm" => self.auto_confirm = trimmed.parse().unwrap_or(false),
            "max_iterations" => self.max_iterations = trimmed.parse().unwrap_or(25),

            "groq_api_key" => self.groq_api_key = opt_val(),
            "groq_model" => self.groq_model = trimmed.to_string(),
            "groq_base_url" => self.groq_base_url = opt_val(),

            "gemini_api_key" => self.gemini_api_key = opt_val(),
            "gemini_model" => self.gemini_model = trimmed.to_string(),
            "gemini_base_url" => self.gemini_base_url = opt_val(),

            "openrouter_api_key" => self.openrouter_api_key = opt_val(),
            "openrouter_model" => self.openrouter_model = trimmed.to_string(),
            "openrouter_base_url" => self.openrouter_base_url = opt_val(),

            "deepseek_api_key" => self.deepseek_api_key = opt_val(),
            "deepseek_model" => self.deepseek_model = trimmed.to_string(),
            "deepseek_base_url" => self.deepseek_base_url = opt_val(),

            "github_token" => self.github_token = opt_val(),
            "github_model" => self.github_model = trimmed.to_string(),
            "github_base_url" => self.github_base_url = opt_val(),

            "sambanova_api_key" => self.sambanova_api_key = opt_val(),
            "sambanova_model" => self.sambanova_model = trimmed.to_string(),
            "sambanova_base_url" => self.sambanova_base_url = opt_val(),

            "mistral_api_key" => self.mistral_api_key = opt_val(),
            "mistral_model" => self.mistral_model = trimmed.to_string(),
            "mistral_base_url" => self.mistral_base_url = opt_val(),

            "ollama_base_url" => self.ollama_base_url = opt_val(),
            "ollama_model" => self.ollama_model = trimmed.to_string(),

            "openai_api_key" => self.openai_api_key = opt_val(),
            "openai_model" => self.openai_model = trimmed.to_string(),
            "openai_base_url" => self.openai_base_url = opt_val(),

            "anthropic_api_key" => self.anthropic_api_key = opt_val(),
            "anthropic_model" => self.anthropic_model = trimmed.to_string(),
            "anthropic_base_url" => self.anthropic_base_url = opt_val(),

            _ => {
                // Support dynamic `<custom_provider>_{api_key,model,base_url}` updates
                let handled = if let Some(prefix) = key.strip_suffix("_api_key") {
                    let p = Self::canonical_provider_name(prefix);
                    if let Some(custom) = self.custom_providers.get_mut(&p) {
                        custom.api_key = opt_val();
                        true
                    } else {
                        false
                    }
                } else if let Some(prefix) = key.strip_suffix("_model") {
                    let p = Self::canonical_provider_name(prefix);
                    if let Some(custom) = self.custom_providers.get_mut(&p) {
                        custom.model = trimmed.to_string();
                        true
                    } else {
                        false
                    }
                } else if let Some(prefix) = key.strip_suffix("_base_url") {
                    let p = Self::canonical_provider_name(prefix);
                    if let Some(custom) = self.custom_providers.get_mut(&p) {
                        custom.base_url = trimmed.to_string();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !handled {
                    anyhow::bail!("Key config '{}' tidak dikenali. Pilihan: default_provider, auto_confirm, groq_api_key, gemini_api_key, openrouter_api_key, deepseek_api_key, github_token, sambanova_api_key, mistral_api_key, ollama_base_url, openai_api_key, anthropic_api_key, atau <custom>_model / <custom>_api_key.", key);
                }
            }
        }
        Ok(())
    }

    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        let p = Self::canonical_provider_name(provider);
        let non_empty = |opt: &Option<String>| opt.clone().filter(|s| !s.trim().is_empty());

        if Self::is_builtin_provider(&p) {
            return match p.as_str() {
                "lontar-free" => Some("anonymous".to_string()),
                "groq" => non_empty(&self.groq_api_key),
                "gemini" => non_empty(&self.gemini_api_key),
                "openrouter" => non_empty(&self.openrouter_api_key),
                "deepseek" => non_empty(&self.deepseek_api_key),
                "github" => non_empty(&self.github_token),
                "sambanova" => non_empty(&self.sambanova_api_key),
                "mistral" => non_empty(&self.mistral_api_key),
                "ollama" => Some("ollama_local".to_string()),
                "openai" => non_empty(&self.openai_api_key),
                "anthropic" => non_empty(&self.anthropic_api_key),
                _ => None,
            }
            .or_else(|| self.custom_providers.get(&p).and_then(|c| non_empty(&c.api_key)));
        }

        self.custom_providers.get(&p).and_then(|c| non_empty(&c.api_key))
    }

    pub fn get_model(&self, provider: &str) -> String {
        let p = Self::canonical_provider_name(provider);
        if Self::is_builtin_provider(&p) {
            return match p.as_str() {
                "lontar-free" => self.lontar_free_model.clone(),
                "groq" => self.groq_model.clone(),
                "gemini" => self.gemini_model.clone(),
                "openrouter" => self.openrouter_model.clone(),
                "deepseek" => self.deepseek_model.clone(),
                "github" => self.github_model.clone(),
                "sambanova" => self.sambanova_model.clone(),
                "mistral" => self.mistral_model.clone(),
                "ollama" => self.ollama_model.clone(),
                "openai" => self.openai_model.clone(),
                "anthropic" => self.anthropic_model.clone(),
                _ => "default-model".to_string(),
            };
        }
        if let Some(custom) = self.custom_providers.get(&p) {
            return custom.model.clone();
        }
        "default-model".to_string()
    }

    pub fn get_base_url(&self, provider: &str) -> Option<String> {
        let p = Self::canonical_provider_name(provider);
        match p.as_str() {
            "groq" => self.groq_base_url.clone().or_else(|| Some("https://api.groq.com/openai/v1".to_string())),
            "gemini" => self.gemini_base_url.clone().or_else(|| Some("https://generativelanguage.googleapis.com".to_string())),
            "openrouter" => self.openrouter_base_url.clone().or_else(|| Some("https://openrouter.ai/api/v1".to_string())),
            "deepseek" => self.deepseek_base_url.clone().or_else(|| Some("https://api.deepseek.com".to_string())),
            "github" => self.github_base_url.clone().or_else(|| Some("https://models.inference.ai.azure.com".to_string())),
            "sambanova" => self.sambanova_base_url.clone().or_else(|| Some("https://api.sambanova.ai/v1".to_string())),
            "mistral" => self.mistral_base_url.clone().or_else(|| Some("https://api.mistral.ai/v1".to_string())),
            "ollama" => self.ollama_base_url.clone().or_else(|| Some("http://localhost:11434/v1".to_string())),
            "openai" => self.openai_base_url.clone().or_else(|| Some("https://api.openai.com/v1".to_string())),
            "anthropic" => self.anthropic_base_url.clone().or_else(|| Some("https://api.anthropic.com".to_string())),
            _ => self.custom_providers.get(&p).map(|c| c.base_url.clone()),
        }
    }

    pub fn set_custom_provider_in_memory(&mut self, name: &str, base_url: &str, api_key: Option<&str>, model: &str) {
        let key = Self::canonical_provider_name(name);
        let entry = CustomProviderConfig {
            name: key.clone(),
            base_url: base_url.trim().to_string(),
            api_key: api_key.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            model: model.trim().to_string(),
        };
        self.custom_providers.insert(key, entry);
    }

    pub fn set_custom_provider(&mut self, name: &str, base_url: &str, api_key: Option<&str>, model: &str) -> Result<()> {
        self.set_custom_provider_in_memory(name, base_url, api_key, model);
        self.save()
    }
}
