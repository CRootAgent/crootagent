use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use std::collections::HashSet;
use std::sync::Arc;

use crate::config::Config;
use crate::provider::{
    create_provider, fetch_remote_models, is_likely_chat_model, rank_chat_models, FailoverInfo,
    LlmProvider, ProviderResponse,
};
use crate::session::ChatMessage;
use crate::tools::ToolDefinition;

#[derive(Debug, Clone)]
pub struct ProviderSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_model: &'static str,
    pub is_free: bool,
    pub env_var: &'static str,
    pub base_url: &'static str,
}

pub const SUPPORTED_PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "lontar-free",
        display_name: "Lontar Free AI (Zero-Config · Siap Pakai)",
        default_model: "openai-fast",
        is_free: true,
        env_var: "NONE",
        base_url: "https://text.pollinations.ai/openai",
    },
    ProviderSpec {
        id: "groq",
        display_name: "Groq Cloud (Fast Llama 3.3 70B)",
        default_model: "llama-3.3-70b-versatile",
        is_free: true,
        env_var: "GROQ_API_KEY",
        base_url: "https://api.groq.com/openai/v1",
    },
    ProviderSpec {
        id: "gemini",
        display_name: "Google Gemini 3.5 Flash-Lite (Free API)",
        default_model: "gemini-3.5-flash-lite",
        is_free: true,
        env_var: "GEMINI_API_KEY",
        base_url: "https://generativelanguage.googleapis.com",
    },
    ProviderSpec {
        id: "openrouter",
        display_name: "OpenRouter Free Models (:free)",
        default_model: "meta-llama/llama-3.3-70b-instruct:free",
        is_free: true,
        env_var: "OPENROUTER_API_KEY",
        base_url: "https://openrouter.ai/api/v1",
    },
    ProviderSpec {
        id: "deepseek",
        display_name: "DeepSeek (V4.1 Flash / V4 Pro / R1)",
        default_model: "deepseek-flash",
        is_free: true,
        env_var: "DEEPSEEK_API_KEY",
        base_url: "https://api.deepseek.com",
    },
    ProviderSpec {
        id: "github",
        display_name: "GitHub Models (Free Tier via PAT)",
        default_model: "gpt-4o-mini",
        is_free: true,
        env_var: "GITHUB_TOKEN",
        base_url: "https://models.inference.ai.azure.com",
    },
    ProviderSpec {
        id: "sambanova",
        display_name: "SambaNova Cloud (Llama 3.3 70B Free)",
        default_model: "Meta-Llama-3.3-70B-Instruct",
        is_free: true,
        env_var: "SAMBANOVA_API_KEY",
        base_url: "https://api.sambanova.ai/v1",
    },
    ProviderSpec {
        id: "mistral",
        display_name: "Mistral NeMo / Small",
        default_model: "mistral-nemo",
        is_free: true,
        env_var: "MISTRAL_API_KEY",
        base_url: "https://api.mistral.ai/v1",
    },
    ProviderSpec {
        id: "ollama",
        display_name: "Ollama Local (100% Free & Offline)",
        default_model: "llama3.2",
        is_free: true,
        env_var: "OLLAMA_HOST",
        base_url: "http://localhost:11434/v1",
    },
    ProviderSpec {
        id: "openai",
        display_name: "OpenAI GPT-4o / gpt-4o-mini",
        default_model: "gpt-4o-mini",
        is_free: false,
        env_var: "OPENAI_API_KEY",
        base_url: "https://api.openai.com/v1",
    },
    ProviderSpec {
        id: "anthropic",
        display_name: "Anthropic Claude 3.5 Sonnet",
        default_model: "claude-3-5-sonnet-20241022",
        is_free: false,
        env_var: "ANTHROPIC_API_KEY",
        base_url: "https://api.anthropic.com",
    },
];

/// Return human-readable failover reason if an error indicates token/quota exhaustion or unavailable model/server.
/// Returns None for missing credentials / invalid API keys (401) so auth bugs fail loudly.
pub fn classify_failover_reason(err_msg: &str) -> Option<&'static str> {
    let lower = err_msg.to_lowercase();

    // Missing or invalid credentials should be reported directly rather than hidden behind a different provider.
    if lower.contains("belum diset")
        || lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("unauthenticated")
        || lower.contains("api_key_invalid")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || lower.contains("api key not valid")
    {
        return None;
    }

    if lower.contains("429")
        || lower.contains("402")
        || lower.contains("rate_limit")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("resource_exhausted")
        || lower.contains("quota")
        || lower.contains("exhausted")
        || lower.contains("insufficient")
        || lower.contains("balance")
        || lower.contains("credit")
        || lower.contains("free_limit")
        || lower.contains("free limit")
        || lower.contains("limit reached")
        || lower.contains("tokens per minute")
        || lower.contains("token limit")
        || lower.contains("out of tokens")
        || lower.contains("exceeded")
    {
        return Some("kuota/token habis atau rate limit");
    }

    if lower.contains("404")
        || lower.contains("410")
        || lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
        || lower.contains("model_not_found")
        || lower.contains("model not found")
        || lower.contains("not_found")
        || lower.contains("does not exist")
        || lower.contains("is not available")
        || lower.contains("not available")
        || lower.contains("unavailable")
        || lower.contains("deprecated")
        || lower.contains("decommissioned")
        || lower.contains("retired")
        || lower.contains("overloaded")
        || lower.contains("service unavailable")
        || lower.contains("no endpoints found")
        || lower.contains("timed out")
    {
        return Some("model tidak tersedia / server sibuk");
    }

    None
}

/// Returns true when the entire account/billing balance on a provider is depleted,
/// so retrying alternate models on that same provider wouldn't help.
pub fn is_account_exhausted_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    lower.contains("402")
        || lower.contains("insufficient balance")
        || lower.contains("insufficient_quota")
        || lower.contains("billing")
        || lower.contains("credit")
}

/// Returns true when the specific model ID was rejected (404 / deprecated / unknown),
/// making live `/models` discovery on the same provider worthwhile.
pub fn is_model_unavailable_error(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    lower.contains("404")
        || lower.contains("410")
        || lower.contains("model_not_found")
        || lower.contains("model not found")
        || lower.contains("does not exist")
        || lower.contains("deprecated")
        || lower.contains("decommissioned")
        || lower.contains("retired")
}

/// Alternate chat models within the same provider when a specific model hits TPM/RPM or is unavailable.
pub fn fallback_models_for_provider(provider: &str, current_model: &str) -> Vec<String> {
    let p = Config::canonical_provider_name(provider);
    let pool: &[&str] = match p.as_str() {
        "gemini" => &[
            "gemini-3.5-flash-lite",
            "gemini-3.5-flash",
            "gemini-3.8-flash",
            "gemini-flash-latest",
            "gemini-2.5-flash-lite",
        ],
        "deepseek" => &["deepseek-flash", "deepseek-chat", "deepseek-v4-pro", "deepseek-reasoner"],
        "groq" => &[
            "llama-3.3-70b-versatile",
            "llama-3.1-8b-instant",
            "openai/gpt-oss-120b",
        ],
        "openrouter" => &[
            "nvidia/nemotron-3.5-lightning:free",
            "meta-llama/llama-3.3-70b-instruct:free",
            "google/gemma-4-26b-a4b-it:free",
            "deepseek/deepseek-r1:free",
            "openrouter/free",
        ],
        "z.ai" => &["glm-4.5-flash", "glm-4.7-flash"],
        "sambanova" => &["Meta-Llama-3.3-70B-Instruct", "Qwen2.5-72B-Instruct"],
        "cerebras" => &["llama3.3-70b", "llama-3.3-70b", "llama3.1-8b"],
        "mistral" => &["mistral-small-latest", "mistral-nemo"],
        "lontar-free" => &["openai-fast", "deepseek", "openai"],
        _ => &[],
    };

    pool.iter()
        .filter(|&&m| !m.eq_ignore_ascii_case(current_model))
        .map(|&m| m.to_string())
        .collect()
}

pub struct PlaceholderProvider {
    pub provider_id: String,
    pub model_name: String,
}

#[async_trait]
impl LlmProvider for PlaceholderProvider {
    fn name(&self) -> &'static str {
        "placeholder"
    }

    fn model(&self) -> &str {
        &self.model_name
    }

    async fn chat(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
    ) -> Result<ProviderResponse> {
        anyhow::bail!(
            "API key untuk '{}' belum diset. Gunakan /config set {}_api_key <KEY> atau /model ollama",
            self.provider_id,
            self.provider_id
        )
    }
}

pub struct LlmRouter {
    primary_provider: Arc<dyn LlmProvider>,
    primary_name: String,
    config: Config,
    pub quiet: bool,
}

impl LlmRouter {
    pub fn new(provider_name: &str, model_override: Option<&str>, config: &Config) -> Result<Self> {
        let canonical = Config::canonical_provider_name(provider_name);
        let model = model_override
            .map(|m| m.to_string())
            .unwrap_or_else(|| config.get_model(&canonical));

        let primary: Arc<dyn LlmProvider> = match create_provider(&canonical, model_override, config) {
            Ok(p) => p,
            Err(_) => Arc::new(PlaceholderProvider {
                provider_id: canonical.clone(),
                model_name: model,
            }),
        };

        Ok(Self {
            primary_provider: primary,
            primary_name: canonical,
            config: config.clone(),
            quiet: false,
        })
    }

    pub fn update_provider(&mut self, provider_name: &str, model_override: Option<&str>, config: &Config) {
        self.config = config.clone();
        let canonical = Config::canonical_provider_name(provider_name);
        self.primary_name = canonical.clone();
        let model = model_override
            .map(|m| m.to_string())
            .unwrap_or_else(|| config.get_model(&canonical));

        self.primary_provider = match create_provider(&canonical, model_override, config) {
            Ok(p) => p,
            Err(_) => Arc::new(PlaceholderProvider {
                provider_id: canonical,
                model_name: model,
            }),
        };
    }

    /// Build ordered failover candidates:
    /// 1. Alternate models on the same provider (unless account balance is depleted)
    /// 2. User-configured custom providers (z.ai, cerebras, etc.)
    /// 3. User-configured built-in providers with API keys set
    /// 4. Zero-config `lontar-free` safety net & local `ollama`
    pub fn build_candidates(&self, err_msg: &str) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        let primary_model = self.primary_provider.model().to_string();
        seen.insert((self.primary_name.to_lowercase(), primary_model.to_lowercase()));

        let mut candidates = Vec::new();
        let mut push_candidate = |prov: String, mdl: String| {
            let key = (prov.to_lowercase(), mdl.to_lowercase());
            if !mdl.trim().is_empty() && seen.insert(key) {
                candidates.push((prov, mdl));
            }
        };

        // 1. Same-provider alternate models (prefer synced live models if user ran `/models fetch`, else static defaults)
        if !is_account_exhausted_error(err_msg) && self.config.has_configured_provider(&self.primary_name) {
            if let Some(synced) = self.config.get_synced_models(&self.primary_name) {
                for live_model in synced
                    .iter()
                    .filter(|m| is_likely_chat_model(m) && !m.eq_ignore_ascii_case(&primary_model))
                    .take(4)
                {
                    push_candidate(self.primary_name.clone(), live_model.clone());
                }
            } else {
                for alt_model in fallback_models_for_provider(&self.primary_name, &primary_model).into_iter().take(3) {
                    push_candidate(self.primary_name.clone(), alt_model);
                }
            }
        }

        // 2. Other user-configured custom providers (sorted for deterministic behavior)
        let mut custom_keys: Vec<String> = self.config.custom_providers.keys().cloned().collect();
        custom_keys.sort();
        for key in custom_keys {
            if key != self.primary_name.to_lowercase() {
                if let Some(custom) = self.config.custom_providers.get(&key) {
                    push_candidate(key, custom.model.clone());
                }
            }
        }

        // 3. Other built-in providers that have API keys configured
        let builtin_with_keys = [
            "gemini",
            "deepseek",
            "groq",
            "openrouter",
            "sambanova",
            "mistral",
            "github",
            "openai",
            "anthropic",
        ];
        for p in builtin_with_keys {
            if p != self.primary_name.to_lowercase() && self.config.get_api_key(p).is_some() {
                push_candidate(p.to_string(), self.config.get_model(p));
            }
        }

        // 4. Zero-config and local fallbacks
        if self.primary_name.to_lowercase() != "lontar-free" {
            push_candidate("lontar-free".to_string(), self.config.get_model("lontar-free"));
            push_candidate("lontar-free".to_string(), "deepseek".to_string());
        }
        if self.primary_name.to_lowercase() != "ollama" {
            push_candidate("ollama".to_string(), self.config.get_model("ollama"));
        }

        candidates
    }

    async fn try_dynamic_model_discovery(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        already_tried: &mut HashSet<(String, String)>,
        reason: &str,
    ) -> Option<ProviderResponse> {
        let base_url = self.config.get_base_url(&self.primary_name);
        let api_key = self.config.get_api_key(&self.primary_name);
        let remote_models = fetch_remote_models(&self.primary_name, base_url.as_deref(), api_key.as_deref())
            .await
            .ok()?;

        let ranked_chat = rank_chat_models(&remote_models);
        let discovered_list = if ranked_chat.is_empty() {
            None
        } else {
            Some(ranked_chat.clone())
        };

        for remote_mdl in ranked_chat.into_iter().take(3) {
            let key = (self.primary_name.to_lowercase(), remote_mdl.to_lowercase());
            if !already_tried.insert(key) {
                continue;
            }
            if let Ok(prov) = create_provider(&self.primary_name, Some(&remote_mdl), &self.config) {
                if let Ok(mut resp) = prov.chat(messages, tools).await {
                    resp.failover = Some(FailoverInfo {
                        from_provider: self.primary_name.clone(),
                        from_model: self.primary_provider.model().to_string(),
                        to_provider: self.primary_name.clone(),
                        to_model: remote_mdl,
                        reason: reason.to_string(),
                        discovered_models: discovered_list,
                    });
                    return Some(resp);
                }
            }
        }
        None
    }
}

#[async_trait]
impl LlmProvider for LlmRouter {
    fn name(&self) -> &'static str {
        "lontar-router"
    }

    fn model(&self) -> &str {
        self.primary_provider.model()
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ProviderResponse> {
        match self.primary_provider.chat(messages, tools).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                let err_msg = e.to_string();
                let Some(reason) = classify_failover_reason(&err_msg) else {
                    return Err(e);
                };

                if !self.quiet {
                    eprintln!(
                        "{}",
                        format!(
                            "⚠️ Model '{}' ({}) {}. Memulai auto-switch...",
                            self.primary_provider.model(),
                            self.primary_name,
                            reason
                        )
                        .yellow()
                        .bold()
                    );
                }

                let mut tried = HashSet::new();
                tried.insert((
                    self.primary_name.to_lowercase(),
                    self.primary_provider.model().to_lowercase(),
                ));

                // When the user's current provider has never been synced and returns 404/unavailable,
                // query its live /models endpoint first before falling back to stale static candidates.
                let already_synced = self.config.get_synced_models(&self.primary_name).is_some();
                if is_model_unavailable_error(&err_msg)
                    && !already_synced
                    && self.config.has_configured_provider(&self.primary_name)
                {
                    if let Some(resp) = self
                        .try_dynamic_model_discovery(messages, tools, &mut tried, reason)
                        .await
                    {
                        return Ok(resp);
                    }
                }

                // Separate same-provider static candidates from cross-provider candidates so dynamic
                // /models healing runs before giving up on the user's chosen provider.
                let all_candidates = self.build_candidates(&err_msg);
                let (same_prov, other_prov): (Vec<_>, Vec<_>) = all_candidates
                    .into_iter()
                    .partition(|(p, _)| p.eq_ignore_ascii_case(&self.primary_name));

                for (fb_prov, fb_mdl) in same_prov {
                    if !tried.insert((fb_prov.to_lowercase(), fb_mdl.to_lowercase())) {
                        continue;
                    }
                    if let Ok(provider) = create_provider(&fb_prov, Some(&fb_mdl), &self.config) {
                        if let Ok(mut resp) = provider.chat(messages, tools).await {
                            resp.failover = Some(FailoverInfo {
                                from_provider: self.primary_name.clone(),
                                from_model: self.primary_provider.model().to_string(),
                                to_provider: fb_prov,
                                to_model: fb_mdl,
                                reason: reason.to_string(),
                                discovered_models: None,
                            });
                            return Ok(resp);
                        }
                    }
                }

                if is_model_unavailable_error(&err_msg)
                    && already_synced
                    && self.config.has_configured_provider(&self.primary_name)
                {
                    if let Some(resp) = self
                        .try_dynamic_model_discovery(messages, tools, &mut tried, reason)
                        .await
                    {
                        return Ok(resp);
                    }
                }

                for (fb_prov, fb_mdl) in other_prov {
                    if !self.quiet {
                        eprintln!(
                            "{}",
                            format!("🔄 Auto-switch mencoba {}:{}...", fb_prov, fb_mdl).cyan()
                        );
                    }
                    if let Ok(provider) = create_provider(&fb_prov, Some(&fb_mdl), &self.config) {
                        match provider.chat(messages, tools).await {
                            Ok(mut resp) => {
                                if !self.quiet {
                                    eprintln!(
                                        "{}",
                                        format!("✅ Auto-switch berhasil ke {} ({})!", fb_prov, fb_mdl)
                                            .green()
                                            .bold()
                                    );
                                }
                                resp.failover = Some(FailoverInfo {
                                    from_provider: self.primary_name.clone(),
                                    from_model: self.primary_provider.model().to_string(),
                                    to_provider: fb_prov,
                                    to_model: fb_mdl,
                                    reason: reason.to_string(),
                                    discovered_models: None,
                                });
                                return Ok(resp);
                            }
                            Err(fb_err) => {
                                if !self.quiet {
                                    eprintln!(
                                        "{}",
                                        format!("❌ Candidate {}:{} gagal: {}", fb_prov, fb_mdl, fb_err)
                                            .dimmed()
                                    );
                                }
                            }
                        }
                    }
                }

                Err(e)
            }
        }
    }
}
