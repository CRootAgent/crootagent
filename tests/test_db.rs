use lontar::db::{Database, DbChunk, DbDocument, DbSession, DbTestLog};

#[test]
fn test_db_initialization_and_seeding() {
    let db = Database::open_in_memory().expect("Failed to open in-memory database");
    let providers = db.list_providers().expect("Failed to list providers");

    assert_eq!(providers.len(), 8);
    let groq = providers.iter().find(|p| p.id == "groq").expect("Groq provider missing");
    assert!(groq.is_free);
    assert_eq!(groq.default_model, "llama-3.3-70b-versatile");

    let gemini = providers.iter().find(|p| p.id == "gemini").expect("Gemini provider missing");
    assert!(gemini.is_free);
}

#[test]
fn test_db_session_crud() {
    let db = Database::open_in_memory().expect("Failed to open in-memory database");
    let session = DbSession {
        id: "sess-abc-123".to_string(),
        name: "Proyek 2 Sprint".to_string(),
        created_at: "2026-11-23T10:00:00Z".to_string(),
        updated_at: "2026-11-23T10:30:00Z".to_string(),
        provider: "groq".to_string(),
        model: "llama-3.3-70b-versatile".to_string(),
        total_tokens: 1540,
    };

    // Insert
    db.insert_session(&session).expect("Failed to insert session");

    // Read / Get
    let fetched = db.get_session("sess-abc-123").expect("Query failed");
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.name, "Proyek 2 Sprint");
    assert_eq!(fetched.provider, "groq");
    assert_eq!(fetched.total_tokens, 1540);

    // List
    let list = db.list_sessions().expect("Failed to list sessions");
    assert_eq!(list.len(), 1);

    // Delete
    let deleted = db.delete_session("sess-abc-123").expect("Failed to delete session");
    assert!(deleted);
    let after_delete = db.get_session("sess-abc-123").expect("Query failed");
    assert!(after_delete.is_none());
}

#[test]
fn test_db_document_and_chunks() {
    let db = Database::open_in_memory().expect("Failed to open in-memory database");

    // Create session parent
    let session = DbSession {
        id: "sess-doc-1".to_string(),
        name: "Doc Session".to_string(),
        created_at: "2026-11-23T10:00:00Z".to_string(),
        updated_at: "2026-11-23T10:00:00Z".to_string(),
        provider: "gemini".to_string(),
        model: "gemini-1.5-flash".to_string(),
        total_tokens: 0,
    };
    db.insert_session(&session).unwrap();

    let doc = DbDocument {
        id: "doc-uuid-1".to_string(),
        session_id: "sess-doc-1".to_string(),
        file_path: "/docs/arsitektur.md".to_string(),
        file_type: "md".to_string(),
        file_size: 4096,
        chunk_count: 2,
        created_at: "2026-11-23T10:00:00Z".to_string(),
    };

    let chunks = vec![
        DbChunk {
            id: 0,
            document_id: "doc-uuid-1".to_string(),
            chunk_index: 0,
            content: "Arsitektur Lontar CLI berbasis Rust.".to_string(),
            token_estimate: 6,
        },
        DbChunk {
            id: 0,
            document_id: "doc-uuid-1".to_string(),
            chunk_index: 1,
            content: "Integrasi dokumen gaya NotebookLM dan failover provider gratis.".to_string(),
            token_estimate: 8,
        },
    ];

    db.insert_document(&doc, &chunks).expect("Insert document failed");

    let docs = db.list_documents("sess-doc-1").expect("List docs failed");
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].file_path, "/docs/arsitektur.md");

    let fetched_chunks = db.get_document_chunks("doc-uuid-1").expect("Get chunks failed");
    assert_eq!(fetched_chunks.len(), 2);
    assert_eq!(fetched_chunks[0].chunk_index, 0);
    assert!(fetched_chunks[1].content.contains("NotebookLM"));

    // Clear documents
    db.clear_session_documents("sess-doc-1").expect("Clear docs failed");
    let after_clear = db.list_documents("sess-doc-1").unwrap();
    assert_eq!(after_clear.len(), 0);
}

#[test]
fn test_db_test_logs() {
    let db = Database::open_in_memory().expect("Failed to open in-memory database");

    let log = DbTestLog {
        id: 0,
        session_id: "sess-test".to_string(),
        test_name: "test_cargo_coverage".to_string(),
        status: "PASS".to_string(),
        output: "All 15 tests passed with 100% success rate".to_string(),
        duration_ms: 125,
        timestamp: "2026-11-23T11:00:00Z".to_string(),
    };

    db.insert_test_log(&log).expect("Failed to insert test log");

    let logs = db.list_test_logs().expect("Failed to list test logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].test_name, "test_cargo_coverage");
    assert_eq!(logs[0].status, "PASS");
}
