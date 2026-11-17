use crootagent::config::Config;

#[test]
fn test_default_config() {
    let cfg = Config::default();
    assert_eq!(cfg.default_provider, "openai");
    assert_eq!(cfg.max_iterations, 25);
    assert!(!cfg.auto_confirm);
    assert_eq!(cfg.openai_model, "gpt-4o");
    assert_eq!(cfg.anthropic_model, "claude-3-5-sonnet-20241022");
    assert_eq!(cfg.gemini_model, "gemini-1.5-pro");
    assert_eq!(cfg.groq_model, "llama-3.3-70b-versatile");
    assert_eq!(cfg.ollama_model, "llama3.2");
}

#[test]
fn test_config_set_value() {
    let mut cfg = Config::default();
    assert!(cfg.set_value("default_provider", "gemini").is_ok());
    assert_eq!(cfg.default_provider, "gemini");

    assert!(cfg.set_value("auto_confirm", "true").is_ok());
    assert!(cfg.auto_confirm);

    assert!(cfg.set_value("max_iterations", "50").is_ok());
    assert_eq!(cfg.max_iterations, 50);

    assert!(cfg.set_value("groq_model", "llama-3.1-8b-instant").is_ok());
    assert_eq!(cfg.groq_model, "llama-3.1-8b-instant");

    assert!(cfg.set_value("invalid_key_xyz", "value").is_err());
}

#[test]
fn test_config_toml_roundtrip() {
    let cfg = Config::default();
    let toml_str = toml::to_string(&cfg).expect("Serialization to TOML failed");
    let parsed: Config = toml::from_str(&toml_str).expect("Deserialization from TOML failed");
    assert_eq!(parsed.default_provider, cfg.default_provider);
    assert_eq!(parsed.openai_model, cfg.openai_model);
}
