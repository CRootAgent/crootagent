use lontar::config::Config;
use tempfile::tempdir;

#[test]
fn test_default_config() {
    let cfg = Config::default();
    assert_eq!(cfg.default_provider, "lontar-free");
    assert_eq!(cfg.max_iterations, 25);
    assert!(!cfg.auto_confirm);
    assert_eq!(cfg.groq_model, "llama-3.3-70b-versatile");
    assert_eq!(cfg.gemini_model, "gemini-3.5-flash-lite");
    assert_eq!(cfg.openrouter_model, "meta-llama/llama-3.3-70b-instruct:free");
    assert_eq!(cfg.deepseek_model, "deepseek-flash");
    assert_eq!(cfg.ollama_model, "llama3.2");
}

#[test]
fn test_config_set_value() {
    let mut cfg = Config::default();
    assert!(cfg.set_value_in_memory("default_provider", "gemini").is_ok());
    assert_eq!(cfg.default_provider, "gemini");

    assert!(cfg.set_value_in_memory("auto_confirm", "true").is_ok());
    assert!(cfg.auto_confirm);

    assert!(cfg.set_value_in_memory("max_iterations", "50").is_ok());
    assert_eq!(cfg.max_iterations, 50);

    assert!(cfg.set_value_in_memory("groq_model", "llama-3.1-8b-instant").is_ok());
    assert_eq!(cfg.groq_model, "llama-3.1-8b-instant");

    assert!(cfg.set_value_in_memory("invalid_key_xyz", "value").is_err());

    // Custom provider config updates & persistence helpers (tested against isolated tempdir)
    cfg.set_custom_provider_in_memory("z.ai", "https://api.z.ai/api/paas/v4", Some("key-123"), "glm-4.5-flash");
    assert!(cfg.has_configured_provider("z.ai"));
    assert!(cfg.set_value_in_memory("z.ai_model", "glm-4.7-flash").is_ok());
    assert_eq!(cfg.get_model("z.ai"), "glm-4.7-flash");

    // Activating provider updates default_provider + model and persists across save_to/load_from
    cfg.activate_provider_in_memory("z.ai", Some("glm-4.7-flash"));
    assert_eq!(cfg.default_provider, "z.ai");

    let dir = tempdir().expect("tempdir failed");
    let cfg_file = dir.path().join("config.toml");
    cfg.save_to(&cfg_file).expect("save_to failed");
    let reloaded = Config::load_from(&cfg_file).expect("load_from failed");
    assert_eq!(reloaded.default_provider, "z.ai");
    assert_eq!(reloaded.get_model("z.ai"), "glm-4.7-flash");
    assert_eq!(reloaded.get_api_key("z.ai").as_deref(), Some("key-123"));

    // Model sync persistence: adding new detected models & removing stale ones survives TOML roundtrip
    let mut cfg_sync = Config::default();
    let live_groq = vec![
        "openai/gpt-oss-120b".to_string(),
        "qwen/qwen3.8-27b".to_string(),
    ];
    let sync_out = cfg_sync.sync_provider_models_in_memory(
        "groq",
        &live_groq,
        &["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
    );
    assert_eq!(sync_out.added.len(), 2);
    assert_eq!(sync_out.removed.len(), 2);
    assert_eq!(sync_out.switched_default_to.as_deref(), Some("openai/gpt-oss-120b"));
    assert_eq!(cfg_sync.get_model("groq"), "openai/gpt-oss-120b");

    let sync_file = dir.path().join("sync_config.toml");
    cfg_sync.save_to(&sync_file).expect("save sync_config failed");
    let reloaded_sync = Config::load_from(&sync_file).expect("load sync_config failed");
    assert_eq!(
        reloaded_sync.get_synced_models("groq").unwrap(),
        &live_groq
    );
    assert_eq!(reloaded_sync.get_model("groq"), "openai/gpt-oss-120b");
}

#[test]
fn test_config_toml_roundtrip() {
    let mut cfg = Config::default();
    cfg.default_provider = "deepseek".to_string();
    cfg.deepseek_api_key = Some("sk-secret-test".to_string());
    cfg.set_custom_provider_in_memory("z.ai", "https://api.z.ai/api/paas/v4", Some("key-zai"), "glm-4.5-flash");
    let toml_str = toml::to_string(&cfg).expect("Serialization to TOML failed");
    let parsed: Config = toml::from_str(&toml_str).expect("Deserialization from TOML failed");
    assert_eq!(parsed.default_provider, "deepseek");
    assert_eq!(parsed.deepseek_api_key.as_deref(), Some("sk-secret-test"));
    assert_eq!(parsed.groq_model, cfg.groq_model);
    assert_eq!(parsed.get_api_key("z.ai").as_deref(), Some("key-zai"));

    // Forward-compatibility: minimal TOML missing new fields still parses cleanly using defaults
    let legacy_toml = r#"
        default_provider = "gemini"
        gemini_api_key = "AIzaTest"
    "#;
    let legacy_parsed: Config = toml::from_str(legacy_toml).expect("Legacy TOML deserialization failed");
    assert_eq!(legacy_parsed.default_provider, "gemini");
    assert_eq!(legacy_parsed.gemini_api_key.as_deref(), Some("AIzaTest"));
    assert_eq!(legacy_parsed.lontar_free_model, "openai-fast");
}
