use lontar::agent::Agent;
use lontar::diff::DiffViewer;
use lontar::router::SUPPORTED_PROVIDERS;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_supported_providers_list() {
    assert_eq!(SUPPORTED_PROVIDERS.len(), 11);

    let free_providers: Vec<_> = SUPPORTED_PROVIDERS.iter().filter(|p| p.is_free).collect();
    assert_eq!(free_providers.len(), 9);

    let ids: Vec<&str> = free_providers.iter().map(|p| p.id).collect();
    assert!(ids.contains(&"lontar-free"));
    assert!(ids.contains(&"groq"));
    assert!(ids.contains(&"gemini"));
    assert!(ids.contains(&"openrouter"));
    assert!(ids.contains(&"deepseek"));
    assert!(ids.contains(&"github"));
    assert!(ids.contains(&"sambanova"));
    assert!(ids.contains(&"mistral"));
    assert!(ids.contains(&"ollama"));
}

#[test]
fn test_diff_viewer_render() {
    let old = "fn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}";
    let new = "fn main() {\n    let a = 42;\n    println!(\"{}\", a);\n}";

    let diff_output = DiffViewer::render_diff(Path::new("main.rs"), old, new);
    assert!(diff_output.contains("DIFF PREVIEW: main.rs"));
    assert!(diff_output.contains("let a = 1;"));
    assert!(diff_output.contains("let a = 42;"));
}

#[test]
fn test_process_inline_mentions() {
    let dir = tempdir().expect("tempdir failed");
    let file_path = dir.path().join("catatan.txt");
    fs::write(&file_path, "Catatan rahasia algoritma").unwrap();

    let mention_str = format!("Tolong analisa file @{}", file_path.to_str().unwrap());
    let processed = Agent::process_inline_mentions(&mention_str);

    assert!(processed.contains("Konten File"));
    assert!(processed.contains("Catatan rahasia algoritma"));
}

#[test]
fn test_format_api_error_simplification() {
    use lontar::tui::format_api_error;

    // 401 Unauthorized with raw JSON
    let raw_gemini_401 = r#"Error dari Google Gemini API (HTTP 401 Unauthorized): { "error": { "code": 401, "message": "API key not valid. Please pass a valid API key.", "status": "UNAUTHENTICATED" } }"#;
    let formatted = format_api_error(raw_gemini_401);
    assert_eq!(formatted, "✖ Gemini: API key tidak valid atau salah (HTTP 401)");

    // 429 Quota
    let raw_groq_429 = "Groq API error HTTP 429: Resource exhausted / rate limit reached for model llama-3.3-70b";
    let formatted_groq = format_api_error(raw_groq_429);
    assert_eq!(formatted_groq, "✖ Groq: Kuota habis atau batas rate limit tercapai (HTTP 429)");

    // 404 Model not found
    let raw_404 = "OpenRouter error: model_not_found 'xyz' does not exist";
    let formatted_404 = format_api_error(raw_404);
    assert_eq!(formatted_404, "✖ OpenRouter: Model tidak ditemukan / tidak tersedia di tier ini (HTTP 404)");

    // 403 Forbidden
    let raw_403 = "HTTP 403: permission_denied country, region, or territory not supported";
    let formatted_403 = format_api_error(raw_403);
    assert!(formatted_403.contains("Wilayah/negara tidak didukung"));
}

#[test]
fn test_resolve_char_with_shift_and_symbols() {
    use crossterm::event::{KeyCode, KeyModifiers};
    use lontar::tui::resolve_char_with_shift;

    // Shift + '/' -> '?'
    assert_eq!(resolve_char_with_shift(KeyCode::Char('/'), KeyModifiers::SHIFT), Some('?'));

    // Shift + '1' -> '!'
    assert_eq!(resolve_char_with_shift(KeyCode::Char('1'), KeyModifiers::SHIFT), Some('!'));

    // Shift + 'a' -> 'A'
    assert_eq!(resolve_char_with_shift(KeyCode::Char('a'), KeyModifiers::SHIFT), Some('A'));

    // Normal '/' -> '/'
    assert_eq!(resolve_char_with_shift(KeyCode::Char('/'), KeyModifiers::NONE), Some('/'));

    // Already shifted '?' -> '?'
    assert_eq!(resolve_char_with_shift(KeyCode::Char('?'), KeyModifiers::NONE), Some('?'));
    assert_eq!(resolve_char_with_shift(KeyCode::Char('?'), KeyModifiers::SHIFT), Some('?'));

    // Ctrl+C -> None (ignored for normal typing)
    assert_eq!(resolve_char_with_shift(KeyCode::Char('c'), KeyModifiers::CONTROL), None);
}

#[test]
fn test_provider_guide_content() {
    use lontar::tui::get_provider_guide;

    let full_guide = get_provider_guide(None);
    assert!(full_guide.contains("Google Gemini"));
    assert!(full_guide.contains("Groq Cloud"));
    assert!(full_guide.contains("OpenRouter"));
    assert!(full_guide.contains("awesome-free-llm-apis"));
    assert!(full_guide.contains("free-ai-apis"));
    assert!(full_guide.contains("/custom"));

    let gemini_guide = get_provider_guide(Some("gemini"));
    assert!(gemini_guide.contains("gemini-3.5-flash-lite"));
    assert!(gemini_guide.contains("aistudio.google.com"));

    let deepseek_guide = get_provider_guide(Some("deepseek"));
    assert!(deepseek_guide.contains("deepseek-flash"));
    assert!(deepseek_guide.contains("deepseek-v4-pro"));
    assert!(deepseek_guide.contains("deepseek-reasoner"));
    assert!(deepseek_guide.contains("platform.deepseek.com"));
}

#[test]
fn test_custom_provider_registration() {
    use lontar::config::Config;
    use lontar::provider::create_provider;

    let mut cfg = Config::default();
    cfg.set_custom_provider_in_memory("cerebras", "https://api.cerebras.ai/v1", Some("csk-test"), "llama3.3-70b");

    assert!(cfg.custom_providers.contains_key("cerebras"));
    assert_eq!(cfg.get_api_key("cerebras"), Some("csk-test".to_string()));
    assert_eq!(cfg.get_model("cerebras"), "llama3.3-70b");
    assert_eq!(cfg.get_base_url("cerebras"), Some("https://api.cerebras.ai/v1".to_string()));

    let prov = create_provider("cerebras", None, &cfg);
    assert!(prov.is_ok());
    let prov = prov.unwrap();
    assert_eq!(prov.name(), "cerebras");
    assert_eq!(prov.model(), "llama3.3-70b");
}

#[test]
fn test_failover_classification_and_candidate_order() {
    use lontar::config::Config;
    use lontar::router::{
        classify_failover_reason, fallback_models_for_provider, is_account_exhausted_error,
        is_model_unavailable_error, LlmRouter,
    };

    // Auth / 401 should NOT auto-failover (must report invalid key)
    assert!(classify_failover_reason("Error HTTP 401 Unauthorized: API key not valid").is_none());
    assert!(classify_failover_reason("API key untuk 'gemini' belum diset").is_none());

    // Quota / tokens exhausted / rate limits SHOULD failover
    assert!(classify_failover_reason("HTTP 429 Too Many Requests: rate limit reached").is_some());
    assert!(classify_failover_reason("Error HTTP 402 Insufficient Balance").is_some());
    assert!(classify_failover_reason("RESOURCE_EXHAUSTED: quota exceeded for tokens per minute").is_some());

    // Model unavailable / deprecated / 404 / 503 SHOULD failover
    assert!(classify_failover_reason("HTTP 404 model_not_found").is_some());
    assert!(classify_failover_reason("Model is deprecated and does not exist").is_some());
    assert!(classify_failover_reason("HTTP 503 service unavailable / overloaded").is_some());
    assert!(is_model_unavailable_error("HTTP 404 Not Found: model does not exist"));
    assert!(is_account_exhausted_error("HTTP 402 Insufficient Balance"));

    // Alternate models on same provider exclude active model
    let gemini_alts = fallback_models_for_provider("gemini", "gemini-3.5-flash-lite");
    assert!(!gemini_alts.contains(&"gemini-3.5-flash-lite".to_string()));
    assert!(gemini_alts.contains(&"gemini-3.5-flash".to_string()));

    // Candidate order:
    // 1. Same-provider alternate models (unless account 402 exhausted)
    // 2. Custom providers (cerebras, z.ai)
    // 3. Configured built-in providers (gemini, deepseek, ...)
    // 4. lontar-free & ollama last resort
    let mut cfg = Config::default();
    cfg.deepseek_api_key = Some("sk-ds".to_string());
    cfg.gemini_api_key = Some("AIza-gem".to_string());
    cfg.set_custom_provider_in_memory("z.ai", "https://api.z.ai/api/paas/v4", Some("key-zai"), "glm-4.5-flash");

    let router = LlmRouter::new("deepseek", Some("deepseek-flash"), &cfg).unwrap();
    let rate_limit_candidates = router.build_candidates("HTTP 429 rate limit");
    // First candidates should be alternate deepseek models
    assert_eq!(rate_limit_candidates[0].0, "deepseek");
    assert_ne!(rate_limit_candidates[0].1, "deepseek-flash");
    // Custom & built-in providers with keys must come BEFORE lontar-free
    let pos_zai = rate_limit_candidates.iter().position(|(p, _)| p == "z.ai").unwrap();
    let pos_gem = rate_limit_candidates.iter().position(|(p, _)| p == "gemini").unwrap();
    let pos_free = rate_limit_candidates.iter().position(|(p, _)| p == "lontar-free").unwrap();
    assert!(pos_zai < pos_free);
    assert!(pos_gem < pos_free);

    // On 402 insufficient balance, skip same-provider deepseek models and jump straight to other providers
    let balance_candidates = router.build_candidates("HTTP 402 Insufficient Balance");
    assert!(!balance_candidates.iter().any(|(p, _)| p == "deepseek"));
    assert_eq!(balance_candidates[0].0, "z.ai");
}

#[tokio::test]
async fn test_router_live_http_failover_on_quota_and_unavailable() {
    use lontar::config::Config;
    use lontar::provider::LlmProvider;
    use lontar::router::LlmRouter;
    use lontar::session::ChatMessage;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Spin up a local HTTP mock server that simulates:
    // - port A ("alpha"): returns 404 model_not_found on "stale-model", and 429 on everything else
    // - port B ("beta"): returns 200 valid OpenAI JSON response on "beta-live"
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { break };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);

                let resp = if req.contains("GET /models") {
                    let body = r#"{"data":[{"id":"auto-discovered-chat"}]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if req.contains("\"model\":\"alpha-404\"") {
                    let body = r#"{"error":{"message":"model_not_found"}}"#;
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if req.contains("\"model\":\"auto-discovered-chat\"") {
                    let body = r#"{"model":"auto-discovered-chat","choices":[{"message":{"role":"assistant","content":"Healed via /models!"},"finish_reason":"stop"}]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if req.contains("\"model\":\"alpha-429\"") {
                    let body = r#"{"error":{"message":"Rate limit / quota exceeded"}}"#;
                    format!(
                        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else {
                    let body = r#"{"model":"beta-live","choices":[{"message":{"role":"assistant","content":"Answered by fallback beta!"},"finish_reason":"stop"}]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                let _ = socket.write_all(resp.as_bytes()).await;
            });
        }
    });

    let base_url = format!("http://{}", addr);

    // Case 1: 404 Model Not Found -> auto-heals via /models discovery on same provider!
    let mut cfg1 = Config::default();
    cfg1.set_custom_provider_in_memory("alpha", &base_url, Some("k1"), "alpha-404");
    cfg1.set_custom_provider_in_memory("beta", &base_url, Some("k2"), "beta-live");

    let mut router1 = LlmRouter::new("alpha", Some("alpha-404"), &cfg1).unwrap();
    router1.quiet = true;
    let res1 = router1.chat(&[ChatMessage::user("hi")], &[]).await.unwrap();
    assert_eq!(res1.content.as_deref(), Some("Healed via /models!"));
    let fb1 = res1.failover.expect("failover metadata expected on 404 heal");
    assert_eq!(fb1.from_model, "alpha-404");
    assert_eq!(fb1.to_provider, "alpha");
    assert_eq!(fb1.to_model, "auto-discovered-chat");

    // Case 2: 429 Quota Exhausted -> auto-switches across provider to beta!
    let mut cfg2 = Config::default();
    cfg2.set_custom_provider_in_memory("alpha", &base_url, Some("k1"), "alpha-429");
    cfg2.set_custom_provider_in_memory("beta", &base_url, Some("k2"), "beta-live");

    let mut router2 = LlmRouter::new("alpha", Some("alpha-429"), &cfg2).unwrap();
    router2.quiet = true;
    let res2 = router2.chat(&[ChatMessage::user("hi")], &[]).await.unwrap();
    assert_eq!(res2.content.as_deref(), Some("Answered by fallback beta!"));
    let fb2 = res2.failover.expect("failover metadata expected on 429 failover");
    assert_eq!(fb2.from_provider, "alpha");
    assert_eq!(fb2.to_provider, "beta");
    assert_eq!(fb2.to_model, "beta-live");
}

#[test]
fn test_filter_non_chat_and_pick_preferred_model() {
    use lontar::provider::{is_likely_chat_model, pick_preferred_chat_model};

    assert!(!is_likely_chat_model("whisper-large-v3"));
    assert!(!is_likely_chat_model("canopylabs/orpheus-v1-english"));
    assert!(!is_likely_chat_model("meta-llama/llama-prompt-guard-2-86m"));
    assert!(!is_likely_chat_model("openai/gpt-oss-safeguard-20b"));

    assert!(is_likely_chat_model("openai/gpt-oss-120b"));
    assert!(is_likely_chat_model("qwen/qwen3.8-27b"));
    assert!(is_likely_chat_model("allam-2-7b"));

    let preferred = pick_preferred_chat_model(&[
        "allam-2-7b".to_string(),
        "openai/gpt-oss-20b".to_string(),
        "openai/gpt-oss-120b".to_string(),
    ]);
    assert_eq!(preferred.as_deref(), Some("openai/gpt-oss-120b"));
}
