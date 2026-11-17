pub mod anthropic;
pub mod gemini;
pub mod openai;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::config::Config;
use crate::session::{ChatMessage, ToolCall};
use crate::tools::ToolDefinition;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub model_name: String,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
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
    let p = provider_name.to_lowercase();
    match p.as_str() {
        "openai" => {
            let api_key = config
                .openai_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key OpenAI belum diset. Jalankan 'crootagent config set openai_api_key <KEY>' atau set env OPENAI_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.openai_model.clone());
            let base_url = config
                .openai_base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("openai", api_key, model, base_url)))
        }
        "groq" => {
            let api_key = config
                .groq_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Groq belum diset. Jalankan 'crootagent config set groq_api_key <KEY>' atau set env GROQ_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.groq_model.clone());
            let base_url = config
                .groq_base_url
                .clone()
                .unwrap_or_else(|| "https://api.groq.com/openai/v1".to_string());
            Ok(Arc::new(OpenAiProvider::new("groq", api_key, model, base_url)))
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
        "anthropic" | "claude" => {
            let api_key = config
                .anthropic_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Anthropic belum diset. Jalankan 'crootagent config set anthropic_api_key <KEY>' atau set env ANTHROPIC_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.anthropic_model.clone());
            let base_url = config
                .anthropic_base_url
                .clone()
                .unwrap_or_else(|| "https://api.anthropic.com".to_string());
            Ok(Arc::new(AnthropicProvider::new(api_key, model, base_url)))
        }
        "gemini" | "google" => {
            let api_key = config
                .gemini_api_key
                .clone()
                .ok_or_else(|| anyhow!("API Key Gemini belum diset. Jalankan 'crootagent config set gemini_api_key <KEY>' atau set env GEMINI_API_KEY"))?;
            let model = model_override
                .map(|m| m.to_string())
                .unwrap_or_else(|| config.gemini_model.clone());
            let base_url = config
                .gemini_base_url
                .clone()
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
            Ok(Arc::new(GeminiProvider::new(api_key, model, base_url)))
        }
        _ => Err(anyhow!(
            "Provider '{}' tidak dikenal. Provider yang didukung: openai, groq, ollama, anthropic, gemini",
            provider_name
        )),
    }
}
