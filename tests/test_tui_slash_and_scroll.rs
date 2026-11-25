use std::path::PathBuf;
use chrono::Utc;
use lontar::document::DocumentEngine;
use lontar::tui::{TuiApp, SLASH_COMMANDS};
use lontar::session::Session;
use lontar::config::Config;

fn make_in_memory_session(provider: &str, model: &str) -> Session {
    Session {
        id: "test-in-memory-session".to_string(),
        name: "Test Session".to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        messages: Vec::new(),
    }
}

#[test]
fn test_slash_command_matching_and_dismissal() {
    let session = make_in_memory_session("openai", "gpt-4o");
    let config = Config::default();
    let mut app = TuiApp::new(session, config, false).unwrap();

    // 1. When typing '/', all commands should match
    app.input = "/".to_string();
    let matches = app.get_matching_slash_commands();
    assert_eq!(matches.len(), SLASH_COMMANDS.len());

    // 2. When typing '/co', should match /context add, /context list, /config set
    app.input = "/co".to_string();
    let matches = app.get_matching_slash_commands();
    let names: Vec<&str> = matches.iter().map(|m| m.name).collect();
    assert!(names.contains(&"/context add"));
    assert!(names.contains(&"/context list"));
    assert!(names.contains(&"/config set"));

    // 3. When typing '/context ', should match /context add and /context list
    app.input = "/context ".to_string();
    let matches = app.get_matching_slash_commands();
    let names: Vec<&str> = matches.iter().map(|m| m.name).collect();
    assert!(names.contains(&"/context add"));
    assert!(names.contains(&"/context list"));
    assert_eq!(names.len(), 2);

    // 4. When typing '/context a', should only match /context add
    app.input = "/context a".to_string();
    let matches = app.get_matching_slash_commands();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "/context add");

    // 5. When typing arguments like '/context add /path/to/file', suggestions should dismiss (empty)
    app.input = "/context add /Users/demo/file.pdf".to_string();
    let matches = app.get_matching_slash_commands();
    assert!(matches.is_empty(), "Slash suggestions should be dismissed while typing arguments!");
}

#[test]
fn test_user_screenshot_failing_strings_resolution() {
    // String case 1:
    let raw1 = "/Users/demo/Downloads/Soal_UTS_Network_Programming_TipeB1.pdf jelaskan ini sacsa ascas";
    let (path1, prompt1) = DocumentEngine::parse_path_and_prompt(raw1);
    assert_eq!(
        path1,
        PathBuf::from("/Users/demo/Downloads/Soal_UTS_Network_Programming_TipeB1.pdf")
    );
    assert_eq!(prompt1, Some("jelaskan ini sacsa ascas".to_string()));

    // String case 2 (macOS escaped spaces):
    let raw2 = "/Users/demo/Documents/Workspace/Perkuliahan/Pemograman/Pemograman\\ II/bab10.pdf baca ini";
    let (path2, prompt2) = DocumentEngine::parse_path_and_prompt(raw2);
    assert_eq!(
        path2,
        PathBuf::from("/Users/demo/Documents/Workspace/Perkuliahan/Pemograman/Pemograman II/bab10.pdf")
    );
    assert_eq!(prompt2, Some("baca ini".to_string()));
}

#[test]
fn test_horizontal_scrolling_viewport_calculations() {
    // Verify viewport calculations when input is long
    let total_chars: usize = 120;
    let avail_width: usize = 40;
    let margin = 4.min(avail_width / 4);

    // Test cursor at beginning:
    let cur_pos: usize = 0;
    let start = if cur_pos < avail_width.saturating_sub(margin) {
        0
    } else {
        (cur_pos + margin + 1)
            .saturating_sub(avail_width)
            .min(total_chars - avail_width)
    };
    let end = (start + avail_width).min(total_chars);
    assert_eq!(start, 0);
    assert_eq!(end, 40);

    // Test cursor at end:
    let cur_pos: usize = 120;
    let start = if cur_pos < avail_width.saturating_sub(margin) {
        0
    } else {
        (cur_pos + margin + 1)
            .saturating_sub(avail_width)
            .min(total_chars - avail_width)
    };
    let end = (start + avail_width).min(total_chars);
    assert_eq!(start, 80);
    assert_eq!(end, 120);

    // Visible slice length is never greater than avail_width
    assert!(end - start <= avail_width);
    // Relative cursor position is within [0, end - start]
    let slice_cur_pos = cur_pos - start;
    assert_eq!(slice_cur_pos, 40);
    assert!(slice_cur_pos <= avail_width);
}

#[test]
fn test_tui_apply_failover_and_configured_custom_provider_preserve() {
    use lontar::provider::FailoverInfo;

    let mut cfg = Config::default();
    // Keyless custom provider must still count as configured (not reset to lontar-free on restart)
    cfg.custom_providers.insert(
        "local-vllm".to_string(),
        lontar::config::CustomProviderConfig {
            name: "local-vllm".to_string(),
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: None,
            model: "qwen2.5".to_string(),
        },
    );
    assert!(cfg.has_configured_provider("local-vllm"));

    let session = make_in_memory_session("local-vllm", "qwen2.5");
    let mut app = TuiApp::new(session, cfg, false).unwrap();

    assert_eq!(app.active_provider, "local-vllm");
    assert_eq!(app.active_model, "qwen2.5");

    // Simulate auto-switch notification + live model update (including discovered_models auto-sync!)
    let badge = app.apply_failover_with_save(
        Some(FailoverInfo {
            from_provider: "local-vllm".to_string(),
            from_model: "qwen2.5".to_string(),
            to_provider: "gemini".to_string(),
            to_model: "gemini-3.5-flash-lite".to_string(),
            reason: "model tidak tersedia / server sibuk".to_string(),
            discovered_models: Some(vec![
                "gemini-3.5-flash-lite".to_string(),
                "gemini-3.8-flash".to_string(),
            ]),
        }),
        false,
    );

    assert_eq!(app.active_provider, "gemini");
    assert_eq!(app.active_model, "gemini-3.5-flash-lite");
    assert_eq!(badge, "gemini-3.5-flash-lite gemini");
    assert!(app.messages.iter().any(|m| m.sender == "System" && m.text.contains("Auto-switch")));

    // Synced models from auto-switch must immediately show in Select Model modal & remove old static Gemini entries
    let gemini_in_modal: Vec<_> = app
        .get_filtered_models()
        .into_iter()
        .filter(|m| m.provider_id == "gemini")
        .collect();
    assert!(gemini_in_modal.iter().any(|m| m.model_id == "gemini-3.5-flash-lite" && m.tag == "● Active"));
    assert!(gemini_in_modal.iter().any(|m| m.model_id == "gemini-3.8-flash"));
    assert!(!gemini_in_modal.iter().any(|m| m.model_id == "gemini-3.5-flash"));
}

#[test]
fn test_fetch_models_sync_adds_detected_and_removes_missing() {
    use lontar::router::LlmRouter;

    let mut cfg = Config::default();
    cfg.groq_api_key = Some("gsk-test".to_string());
    let session = make_in_memory_session("groq", "llama-3.3-70b-versatile");
    let mut app = TuiApp::new(session, cfg, false).unwrap();

    // Before sync: static Groq models appear in Select Model modal
    let before_groq: Vec<String> = app
        .get_filtered_models()
        .into_iter()
        .filter(|m| m.provider_id == "groq")
        .map(|m| m.model_id)
        .collect();
    assert!(before_groq.contains(&"llama-3.3-70b-versatile".to_string()));
    assert!(before_groq.contains(&"llama-3.1-8b-instant".to_string()));

    // Exact 11 models returned by Groq API in user's screenshot
    let groq_live_raw = vec![
        "openai/gpt-oss-safeguard-20b".to_string(),
        "meta-llama/llama-prompt-guard-2-86m".to_string(),
        "qwen/qwen3.8-27b".to_string(),
        "canopylabs/orpheus-arabic-saudi".to_string(),
        "canopylabs/orpheus-v1-english".to_string(),
        "openai/gpt-oss-20b".to_string(),
        "allam-2-7b".to_string(),
        "openai/gpt-oss-120b".to_string(),
        "whisper-large-v3-turbo".to_string(),
        "whisper-large-v3".to_string(),
        "meta-llama/llama-prompt-guard-2-22m".to_string(),
    ];

    let summary = app.apply_fetched_models_with_save("groq", &groq_live_raw, false);
    assert!(summary.contains("4 model chat aktif"));
    assert!(summary.contains("+ Ditambahkan"));
    assert!(summary.contains("- Dihapus"));

    // Stale static models MUST be removed from Select Model modal (including "Recent")
    let after_groq: Vec<String> = app
        .get_filtered_models()
        .into_iter()
        .filter(|m| m.provider_id == "groq")
        .map(|m| m.model_id)
        .collect();
    assert!(!after_groq.contains(&"llama-3.3-70b-versatile".to_string()));
    assert!(!after_groq.contains(&"llama-3.1-8b-instant".to_string()));

    // Non-chat audio/guard models should NOT clutter chat selector
    assert!(!after_groq.contains(&"whisper-large-v3".to_string()));
    assert!(!after_groq.contains(&"canopylabs/orpheus-v1-english".to_string()));
    assert!(!after_groq.contains(&"meta-llama/llama-prompt-guard-2-86m".to_string()));

    // Live chat models MUST be added
    assert!(after_groq.contains(&"qwen/qwen3.8-27b".to_string()));
    assert!(after_groq.contains(&"openai/gpt-oss-120b".to_string()));
    assert!(after_groq.contains(&"openai/gpt-oss-20b".to_string()));
    assert!(after_groq.contains(&"allam-2-7b".to_string()));

    // Stale active model should have been switched automatically to flagship detected chat model
    assert_eq!(app.active_model, "openai/gpt-oss-120b");

    // Router failover should use synced live models and avoid removed static models
    let router = LlmRouter::new("groq", Some("openai/gpt-oss-120b"), &app.agent.config).unwrap();
    let fb_candidates = router.build_candidates("HTTP 429 rate limit");
    let same_prov_models: Vec<String> = fb_candidates
        .into_iter()
        .filter(|(p, _)| p == "groq")
        .map(|(_, m)| m)
        .collect();
    assert!(!same_prov_models.contains(&"llama-3.1-8b-instant".to_string()));
    assert!(same_prov_models.contains(&"qwen/qwen3.8-27b".to_string()));
}
