use crootagent::session::{ChatMessage, Session, ToolCall};

#[test]
fn test_chat_message_creation() {
    let msg_user = ChatMessage::user("Halo dunia");
    assert_eq!(msg_user.role, "user");
    assert_eq!(msg_user.content.as_deref(), Some("Halo dunia"));

    let msg_sys = ChatMessage::system("System instructions");
    assert_eq!(msg_sys.role, "system");
    assert_eq!(msg_sys.content.as_deref(), Some("System instructions"));

    let tool_call = ToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: r#"{"path":"test.txt"}"#.to_string(),
    };
    let msg_asst = ChatMessage::assistant(
        Some("Memeriksa file...".to_string()),
        Some(vec![tool_call]),
        Some("openai:gpt-4o".to_string()),
    );
    assert_eq!(msg_asst.role, "assistant");
    assert_eq!(msg_asst.tool_calls.as_ref().unwrap().len(), 1);
    assert_eq!(msg_asst.model_attribution.as_deref(), Some("openai:gpt-4o"));

    let msg_tool = ChatMessage::tool_response("call_1".to_string(), "File content here".to_string());
    assert_eq!(msg_tool.role, "tool");
    assert_eq!(msg_tool.tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn test_session_serialization() {
    let mut sess = Session {
        id: "test-sess-123".to_string(),
        name: "Test Session".to_string(),
        provider: "groq".to_string(),
        model: "llama-3.3-70b-versatile".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
    };

    sess.messages.push(ChatMessage::user("Test prompt"));
    sess.messages.push(ChatMessage::assistant(Some("Test response".to_string()), None, None));

    let json_str = serde_json::to_string(&sess).expect("Failed to serialize session");
    let deserialized: Session = serde_json::from_str(&json_str).expect("Failed to deserialize session");

    assert_eq!(deserialized.id, "test-sess-123");
    assert_eq!(deserialized.name, "Test Session");
    assert_eq!(deserialized.messages.len(), 2);
}
