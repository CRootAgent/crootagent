pub mod anthropic;
pub mod gemini;
pub mod openai;

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::config::Config;
use crate::session::{ChatMessage, ToolCall};
use crate::tools::ToolDefinition;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

#[derive(Debug, Clone)]
pub struct FailoverInfo {
    pub from_provider: String,
    pub from_model: String,
    pub to_provider: String,
    pub to_model: String,
    pub reason: String,
    pub discovered_models: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub model_name: String,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
    pub failover: Option<FailoverInfo>,
}

pub fn is_likely_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    let skip_terms = [
        "whisper",
        "distil-whisper",
        "tts",
        "orpheus",
        "playai",
        "kokoro",
        "dall-e",
        "embedding",
        "embed-",
        "rerank",
        "moderation",
        "guard",
        "prompt-guard",
        "safeguard",
        "shield",
        "imagen",
        "veo",
        "lyria",
        "aqa",
        "bison",
        "gecko",
        "transcribe",
        "audio",
    ];
    !skip_terms.iter().any(|t| lower.contains(t))
}

/// Rank detected chat models so flagship conversational models come first.
pub fn rank_chat_models(models: &[String]) -> Vec<String> {
    let chat_pool: Vec<&String> = models.iter().filter(|m| is_likely_chat_model(m)).collect();
    let mut ranked: Vec<String> = if chat_pool.is_empty() {
        models.to_vec()
    } else {
        chat_pool.into_iter().cloned().collect()
    };

    let score = |id: &str| -> i32 {
        let l = id.to_lowercase();
        let mut s = 0;
        if l.contains("120b") || l.contains("70b") {
            s += 40;
        } else if l.contains("32b") || l.contains("27b") || l.contains("20b") {
            s += 25;
        }
        if l.contains("flash") || l.contains("versatile") || l.contains("pro") {
            s += 30;
        }
        if l.contains("gpt-oss") || l.contains("qwen") || l.contains("deepseek") || l.contains("gemini") || l.contains("llama") {
            s += 20;
        }
        if l.contains("lite") || l.contains("instant") || l.contains("mini") || l.contains("7b") || l.contains("8b") {
            s -= 5;
        }
        s
    };

    ranked.sort_by(|a, b| score(b).cmp(&score(a)));
    ranked
}

/// Score detected chat models so flagship conversational models are chosen before tiny/niche ones.
pub fn pick_preferred_chat_model(models: &[String]) -> Option<String> {
    rank_chat_models(models).into_iter().next()
}

fn intern_provider_name(name: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static INTERNED: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let map = INTERNED.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().expect("provider name lock");
    if let Some(&existing) = guard.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    guard.insert(name.to_string(), leaked);
    leaked
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn model(&self) -> &str;
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ProviderResponse>;
}

pub fn create_provider(
    provider_name: &str,
    model_override: Option<&str>,
    config: &Config,
) -> Result<Arc<dyn LlmProvider>> {
    let p = Config::canonical_provider_name(provider_name);
    match p.as_str() {
        "lontar-free" => {
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.lontar_free_model.clone());
            let base_url = "https://text.pollinations.ai/openai".to_string();
            Ok(Arc::new(OpenAiProvider::new(
                "lontar-free",
                "anonymous".to_string(),
                model,
                base_url,
            )))
        }
        "groq" => {
            let api_key = config
                .groq_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Groq belum diset. Jalankan 'lontar config set groq_api_key <KEY>' atau set env GROQ_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.groq_model.clone());
            let base_url = config
                .groq_base_url
                .clone()
                .unwrap_or_else(|| "https://api.groq.com/openai/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("groq", api_key, model, base_url)))
        }
        "gemini" | "google" => {
            let api_key = config
                .gemini_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Gemini belum diset. Jalankan 'lontar config set gemini_api_key <KEY>' atau set env GEMINI_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.gemini_model.clone());
            let base_url = config
                .gemini_base_url
                .clone()
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
            Ok(Arc::new(GeminiProvider::new(api_key, model, base_url)))
        }
        "openrouter" => {
            let api_key = config
                .openrouter_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key OpenRouter belum diset. Jalankan 'lontar config set openrouter_api_key <KEY>' atau set env OPENROUTER_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.openrouter_model.clone());
            let base_url = config
                .openrouter_base_url
                .clone()
                .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("openrouter", api_key, model, base_url)))
        }
        "deepseek" => {
            let api_key = config
                .get_api_key("deepseek")
                .ok_or_else(|| anyhow!("API Key DeepSeek belum diset. Jalankan 'lontar config set deepseek_api_key <KEY>' atau set env DEEPSEEK_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.deepseek_model.clone());
            let base_url = config
                .deepseek_base_url
                .clone()
                .unwrap_or_else(|| "https://api.deepseek.com".to_string());
            Ok(Arc::new(OpenAiProvider::new("deepseek", api_key, model, base_url)))
        }
        "github" => {
            let api_key = config
                .github_token
                .clone()
                .ok_or_else(|| anyhow!("GitHub Token belum diset. Jalankan 'lontar config set github_token <TOKEN>' atau set env GITHUB_TOKEN"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.github_model.clone());
            let base_url = config
                .github_base_url
                .clone()
                .unwrap_or_else(|| "https://models.inference.ai.azure.com".to_string());
            Ok(Arc::new(OpenAiProvider::new("github", api_key, model, base_url)))
        }
        "sambanova" => {
            let api_key = config
                .sambanova_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key SambaNova belum diset. Jalankan 'lontar config set sambanova_api_key <KEY>' atau set env SAMBANOVA_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.sambanova_model.clone());
            let base_url = config
                .sambanova_base_url
                .clone()
                .unwrap_or_else(|| "https://api.sambanova.ai/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("sambanova", api_key, model, base_url)))
        }
        "mistral" => {
            let api_key = config
                .mistral_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Mistral belum diset. Jalankan 'lontar config set mistral_api_key <KEY>' atau set env MISTRAL_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.mistral_model.clone());
            let base_url = config
                .mistral_base_url
                .clone()
                .unwrap_or_else(|| "https://api.mistral.ai/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("mistral", api_key, model, base_url)))
        }
        "ollama" => {
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.ollama_model.clone());
            let base_url = config
                .ollama_base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new(
                "ollama",
                "ollama".to_string(),
                model,
                base_url,
            )))
        }
        "openai" => {
            let api_key = config
                .openai_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key OpenAI belum diset. Jalankan 'lontar config set openai_api_key <KEY>' atau set env OPENAI_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.openai_model.clone());
            let base_url = config
                .openai_base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("openai", api_key, model, base_url)))
        }
        "anthropic" | "claude" => {
            let api_key = config
                .anthropic_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Anthropic belum diset. Jalankan 'lontar config set anthropic_api_key <KEY>' atau set env ANTHROPIC_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.anthropic_model.clone());
            let base_url = config
                .anthropic_base_url
                .clone()
                .unwrap_or_else(|| "https://api.anthropic.com".to_string());
            Ok(Arc::new(AnthropicProvider::new(api_key, model, base_url)))
        }
        _ => {
            if let Some(custom) = config.custom_providers.get(&p) {
                let api_key = custom
                    .api_key
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "anonymous".to_string());
                let model = model_override
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| custom.model.clone());
                let interned_name = intern_provider_name(&custom.name);
                Ok(Arc::new(OpenAiProvider::new(
                    interned_name,
                    api_key,
                    model,
                    custom.base_url.clone(),
                )))
            } else {
                Err(anyhow!(
                    "Provider '{}' tidak dikenal. Provider gratis: groq, gemini, openrouter, deepseek, github, sambanova, mistral, ollama. Komersial: openai, anthropic, atau provider kustom.",
                    provider_name
                ))
            }
        }
    }
}

pub async fn fetch_remote_models(
    provider_name: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let p = provider_name.to_lowercase();
    if p == "gemini" || p == "google" {
        let key = api_key.ok_or_else(|| anyhow!("API key Gemini diperlukan untuk mengambil daftar model"))?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={}", key);
        let res = client.get(&url).send().await?;
        if !res.status().is_success() {
            let err_txt = res.text().await.unwrap_or_default();
            bail!("Gagal memanggil API Gemini: {}", err_txt);
        }
        let json_val: serde_json::Value = res.json().await?;
        let mut models = Vec::new();
        if let Some(list) = json_val.get("models").and_then(|m| m.as_array()) {
            for item in list {
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let methods = item.get("supportedGenerationMethods").and_then(|m| m.as_array());
                let supports_generate = methods.map_or(false, |arr| {
                    arr.iter().any(|m| m.as_str() == Some("generateContent"))
                });
                if supports_generate {
                    let clean_name = name.trim_start_matches("models/").to_string();
                    models.push(clean_name);
                }
            }
        }
        return Ok(models);
    }

    // OpenAI-compatible /models endpoint
    let base = base_url.unwrap_or("https://api.openai.com/v1");
    let mut url = base.trim_end_matches('/').to_string();
    if !url.ends_with("/models") {
        url.push_str("/models");
    }

    let mut req = client.get(&url);
    if let Some(key) = api_key {
        if !key.is_empty() && key != "anonymous" && key != "ollama" {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let res = req.send().await?;
    if !res.status().is_success() {
        let err_txt = res.text().await.unwrap_or_default();
        bail!("Gagal memanggil endpoint /models provider {}: {}", provider_name, err_txt);
    }

    let json_val: serde_json::Value = res.json().await?;
    let mut models = Vec::new();
    if let Some(data) = json_val.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                models.push(id.to_string());
            }
        }
    }
    Ok(models)
}
