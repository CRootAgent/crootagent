use lontar::db::Database;
use lontar::document::DocumentEngine;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_document_chunk_text() {
    let sample = "Satu dua tiga empat lima enam tujuh delapan sembilan sepuluh";
    let chunks = DocumentEngine::chunk_text(sample, 4, 1);

    // chunk 1: "Satu dua tiga empat" (0..4)
    // chunk 2: "empat lima enam tujuh" (3..7)
    // chunk 3: "tujuh delapan sembilan sepuluh" (6..10)
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0], "Satu dua tiga empat");
    assert_eq!(chunks[1], "empat lima enam tujuh");
    assert_eq!(chunks[2], "tujuh delapan sembilan sepuluh");
}

#[test]
fn test_document_ingest_and_retrieve_context() {
    let db = Database::open_in_memory().expect("Failed to open DB");
    db.insert_session(&lontar::db::DbSession {
        id: "test-sess".to_string(),
        name: "Test Session".to_string(),
        created_at: "2026-11-23T10:00:00Z".to_string(),
        updated_at: "2026-11-23T10:00:00Z".to_string(),
        provider: "groq".to_string(),
        model: "llama-3.3-70b-versatile".to_string(),
        total_tokens: 0,
    }).unwrap();

    let dir = tempdir().expect("Failed to create tempdir");
    let file_path = dir.path().join("panduan.md");

    fs::write(
        &file_path,
        "# Panduan Belajar Rust\nRust memiliki sistem kepemilikan (ownership) dan borrow checker untuk keamanan memori tanpa garbage collector.\nLontar CLI dibangun dengan Rust murni.",
    )
    .unwrap();

    let doc = DocumentEngine::ingest_file(&db, "test-sess", file_path.to_str().unwrap())
        .expect("Ingest failed");
    assert_eq!(doc.file_type, "md");
    assert!(doc.chunk_count >= 1);

    // Query matching terms
    let context = DocumentEngine::retrieve_context(&db, "test-sess", "ownership borrow checker", 2)
        .expect("Retrieve failed");
    assert!(context.contains("GROUNDED CONTEXT"));
    assert!(context.contains("panduan.md"));
    assert!(context.contains("ownership"));
}

#[test]
fn test_build_study_guide_prompt() {
    let prompt = DocumentEngine::build_study_guide_prompt("Kompilasi Kernel Linux", "Konteks kernel linux...");
    assert!(prompt.contains("STUDY GUIDE: Kompilasi Kernel Linux"));
    assert!(prompt.contains("Ikhtisar Eksekutif"));
    assert!(prompt.contains("Glosarium"));
    assert!(prompt.contains("Pertanyaan Evaluasi Diri"));
}

#[test]
fn test_build_prd_prompt() {
    let prompt = DocumentEngine::build_prd_prompt("Lontar CLI Agent", "Konteks requirements...");
    assert!(prompt.contains("PRODUCT REQUIREMENT DOCUMENT (PRD): Lontar CLI Agent"));
    assert!(prompt.contains("Problem Statement"));
    assert!(prompt.contains("User Personas"));
    assert!(prompt.contains("Acceptance Criteria"));
}

#[test]
fn test_build_tasklist_prompt() {
    let prompt = DocumentEngine::build_tasklist_prompt("PRD text here");
    assert!(prompt.contains("IMPLEMENTATION TASKLIST & ROADMAP"));
    assert!(prompt.contains("Phase 1"));
    assert!(prompt.contains("Phase 2"));
    assert!(prompt.contains("Phase 3"));
}

#[test]
fn test_parse_path_and_prompt() {
    let dir = tempdir().expect("Failed to create tempdir");
    let file_path = dir.path().join("soal_uts.pdf");
    fs::write(&file_path, b"test content").unwrap();

    let raw_input = format!("{} jelaskan ini sacsa ascas", file_path.to_str().unwrap());
    let (parsed_path, prompt) = DocumentEngine::parse_path_and_prompt(&raw_input);
    assert_eq!(parsed_path, file_path);
    assert_eq!(prompt, Some("jelaskan ini sacsa ascas".to_string()));

    // Escaped folder test
    let subfolder = dir.path().join("Pemograman II");
    fs::create_dir_all(&subfolder).unwrap();
    let nested_file = subfolder.join("bab10.pdf");
    fs::write(&nested_file, b"bab 10").unwrap();

    let escaped_input = format!("{}/bab10.pdf baca ini", subfolder.to_str().unwrap().replace(" ", "\\ "));
    let (parsed_nested, nested_prompt) = DocumentEngine::parse_path_and_prompt(&escaped_input);
    assert_eq!(parsed_nested, nested_file);
    assert_eq!(nested_prompt, Some("baca ini".to_string()));
}
