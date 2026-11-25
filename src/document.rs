use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::db::{Database, DbChunk, DbDocument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedChunk {
    pub file_path: String,
    pub chunk_index: usize,
    pub content: String,
    pub token_estimate: usize,
}

pub struct DocumentEngine;

impl DocumentEngine {
    /// Ingest a file (markdown, text, code, or pdf) into the database for a session.
    pub fn ingest_file(db: &Database, session_id: &str, path_str: &str) -> Result<DbDocument> {
        let path = PathBuf::from(path_str);
        if !path.exists() {
            anyhow::bail!("File {:?} tidak ditemukan", path);
        }

        let metadata = fs::metadata(&path)
            .with_context(|| format!("Gagal membaca metadata file {:?}", path))?;
        let file_size = metadata.len() as i64;
        let file_type = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("txt")
            .to_lowercase();

        let raw_text = Self::extract_text(&path, &file_type)?;
        let chunks_text = Self::chunk_text(&raw_text, 800, 100);

        let doc_id = Uuid::new_v4().to_string();
        let db_doc = DbDocument {
            id: doc_id.clone(),
            session_id: session_id.to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_type,
            file_size,
            chunk_count: chunks_text.len() as i64,
            created_at: Utc::now().to_rfc3339(),
        };

        let mut db_chunks = Vec::new();
        for (i, c) in chunks_text.iter().enumerate() {
            let tokens = c.split_whitespace().count();
            db_chunks.push(DbChunk {
                id: 0,
                document_id: doc_id.clone(),
                chunk_index: i as i64,
                content: c.clone(),
                token_estimate: tokens as i64,
            });
        }

        db.insert_document(&db_doc, &db_chunks)?;
        Ok(db_doc)
    }

    /// Extract text from file based on type
    pub fn extract_text(path: &Path, file_type: &str) -> Result<String> {
        match file_type {
            "pdf" => {
                // Read PDF using lopdf for high quality text extraction
                if let Ok(doc) = lopdf::Document::load(path) {
                    let mut text = String::new();
                    let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
                    for page in pages {
                        if let Ok(page_text) = doc.extract_text(&[page]) {
                            text.push_str(&page_text);
                            text.push('\n');
                        }
                    }
                    if !text.trim().is_empty() {
                        return Ok(text);
                    }
                }
                // Fallback if lopdf fails or yields empty
                let bytes = fs::read(path)?;
                Ok(Self::extract_pdf_simple(&bytes))
            }
            _ => {
                // Plain text / markdown / code
                let content = fs::read_to_string(path)
                    .with_context(|| format!("Gagal membaca isi file {:?}", path))?;
                Ok(content)
            }
        }
    }

    /// Extract text streams from raw PDF bytes (no heavy native C dependencies required)
    fn extract_pdf_simple(bytes: &[u8]) -> String {
        let mut text = String::new();
        let content_str = String::from_utf8_lossy(bytes);

        // Scan for standard PDF stream blocks containing text or literal strings
        for line in content_str.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() > 2 {
                text.push_str(&trimmed[1..trimmed.len() - 1]);
                text.push('\n');
            } else if trimmed.contains("BT") && trimmed.contains("ET") {
                // Text object in PDF
                text.push_str(trimmed);
                text.push('\n');
            }
        }

        if text.trim().is_empty() {
            // Fallback: extract all ASCII printable sequences >= 4 chars
            let mut current = String::new();
            for &b in bytes {
                if b.is_ascii_graphic() || b == b' ' || b == b'\n' {
                    current.push(b as char);
                } else {
                    if current.trim().len() >= 4 {
                        text.push_str(current.trim());
                        text.push('\n');
                    }
                    current.clear();
                }
            }
        }
        text
    }

    /// Chunk text into overlapping segments
    pub fn chunk_text(text: &str, chunk_words: usize, overlap_words: usize) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < words.len() {
            let end = (start + chunk_words).min(words.len());
            let chunk_slice = &words[start..end];
            chunks.push(chunk_slice.join(" "));

            if end >= words.len() {
                break;
            }
            start += chunk_words - overlap_words;
        }

        chunks
    }

    /// Retrieve relevant grounded context chunks for a prompt query
    pub fn retrieve_context(db: &Database, session_id: &str, query: &str, max_chunks: usize) -> Result<String> {
        let docs = db.list_documents(session_id)?;
        if docs.is_empty() {
            return Ok(String::new());
        }

        let query_terms: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|s| s.to_string())
            .collect();

        let mut scored_chunks: Vec<(usize, String, String)> = Vec::new(); // (score, file_path, content)

        for doc in &docs {
            let chunks = db.get_document_chunks(&doc.id)?;
            for chunk in chunks {
                let content_lower = chunk.content.to_lowercase();
                let mut score = 0;
                for term in &query_terms {
                    if content_lower.contains(term) {
                        score += 1;
                    }
                }
                scored_chunks.push((score, doc.file_path.clone(), chunk.content));
            }
        }

        // Sort descending by score
        scored_chunks.sort_by(|a, b| b.0.cmp(&a.0));

        let selected = scored_chunks.into_iter().take(max_chunks);
        let mut context_block = String::from("\n=== GROUNDED CONTEXT FROM NOTEBOOKLM DOCUMENTS ===\n");
        for (idx, (score, path, content)) in selected.enumerate() {
            let file_name = Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or(path.clone());
            context_block.push_str(&format!(
                "\n--- [Source #{}: {} | Relevance Score: {}] ---\n{}\n",
                idx + 1,
                file_name,
                score,
                content
            ));
        }
        context_block.push_str("\n=== END GROUNDED CONTEXT ===\n");

        Ok(context_block)
    }

    /// Generate NotebookLM Study Guide artifact
    pub fn build_study_guide_prompt(topic: &str, grounded_context: &str) -> String {
        format!(
            r#"Tolong buatkan Study Guide (Panduan Belajar Lengkap) yang terstruktur dan komprehensif mengenai: '{}' berdasarkan dokumen berikut.

{}

Format output WAJIB mengikuti struktur ini:
# 📚 STUDY GUIDE: {}

## 1. Ikhtisar Eksekutif & Tujuan Pembelajaran
[Jelaskan rangkuman inti dalam 2-3 paragraf terstruktur]

## 2. Poin-Poin Konsep Kunci
- **Konsep A**: [Penjelasan mendalam]
- **Konsep B**: [Penjelasan mendalam]

## 3. Glosarium Istilah Penting
| Istilah | Definisi Konseptual |
|---|---|
| ... | ... |

## 4. Analisis Komparatif & Studi Kasus
[Sajikan perbandingan atau penerapan praktis]

## 5. Pertanyaan Evaluasi Diri (Kuis Pemahaman)
1. [Pertanyaan 1]?
2. [Pertanyaan 2]?
3. [Pertanyaan 3]?
"#,
            topic, grounded_context, topic
        )
    }

    /// Generate Product Requirements Document (PRD) artifact
    pub fn build_prd_prompt(feature_name: &str, context: &str) -> String {
        format!(
            r#"Tolong buatkan Product Requirement Document (PRD) standar industri software engineering untuk: '{}' berdasarkan konteks berikut:

{}

Format output WAJIB mengikuti struktur ini:
# 📋 PRODUCT REQUIREMENT DOCUMENT (PRD): {}

## 1. Latar Belakang & Masalah (Problem Statement)
[Jelaskan problem apa yang diselesaikan dan dampaknya]

## 2. Tujuan & Metrik Keberhasilan (Success Metrics)
- Metrik 1: ...
- Metrik 2: ...

## 3. Profil Pengguna (User Personas) & User Stories
- Sebagai [Persona], saya ingin [Aksi], sehingga [Manfaat].

## 4. Ruang Lingkup Fitur (Feature Scope)
### In-Scope (MVP)
- [ ] Fitur A
- [ ] Fitur B
### Out-of-Scope (Future Phasing)
- Fitur C

## 5. Persyaratan Non-Fungsional (Performance, Security, Reliability)
- Kinerja: ...
- Keamanan: ...

## 6. Kriteria Penerimaan (Acceptance Criteria)
- GIVEN ... WHEN ... THEN ...
"#,
            feature_name, context, feature_name
        )
    }

    /// Generate Tasklist artifact from PRD or user goal
    pub fn build_tasklist_prompt(prd_text: &str) -> String {
        format!(
            r#"Tolong buatkan daftar tugas implementasi teknis (Actionable Tasklist) dengan prioritas dan gate pengujian berdasarkan PRD berikut:

{}

Format output WAJIB mengikuti struktur ini:
# 📝 IMPLEMENTATION TASKLIST & ROADMAP

## Phase 1: Setup & Data Layer
- [ ] Task 1.1: ... -> Verifikasi: ...
- [ ] Task 1.2: ... -> Verifikasi: ...

## Phase 2: Core Business Logic & Integration
- [ ] Task 2.1: ... -> Verifikasi: ...
- [ ] Task 2.2: ... -> Verifikasi: ...

## Phase 3: Testing & Code Coverage Gate
- [ ] Task 3.1: Unit & Integration Tests (Target >=80% Code Coverage)
- [ ] Task 3.2: Multi-OS release verification
"#,
            prd_text
        )
    }

    /// Parse raw input from `/context add` or drag & drop.
    /// Handles unescaping shell backslashes (e.g. `\ ` -> ` `), quotes (`"..."` / `'...'`),
    /// home directory expansion (`~`), and separates the valid file path from trailing user prompt.
    pub fn parse_path_and_prompt(raw: &str) -> (PathBuf, Option<String>) {
        let raw = raw.trim();
        if raw.is_empty() {
            return (PathBuf::new(), None);
        }

        fn clean_str(s: &str) -> PathBuf {
            let mut unescaped = String::with_capacity(s.len());
            let mut chars = s.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(&next_c) = chars.peek() {
                        if next_c == ' ' || next_c == '\\' || next_c == '"' || next_c == '\'' {
                            unescaped.push(next_c);
                            chars.next();
                            continue;
                        }
                    }
                }
                unescaped.push(c);
            }

            let trimmed = unescaped.trim_matches(|c| c == '"' || c == '\'');
            if trimmed.starts_with("~/") || trimmed == "~" {
                if let Ok(home) = std::env::var("HOME") {
                    return PathBuf::from(trimmed.replacen('~', &home, 1));
                }
            }
            PathBuf::from(trimmed)
        }

        // 1. Quoted path: "path with spaces" [prompt]
        if raw.starts_with('"') {
            if let Some(end_quote) = raw[1..].find('"') {
                let path_part = &raw[1..=end_quote];
                let rest_part = raw[end_quote + 2..].trim();
                let prompt = if rest_part.is_empty() { None } else { Some(rest_part.to_string()) };
                return (clean_str(path_part), prompt);
            }
        } else if raw.starts_with('\'') {
            if let Some(end_quote) = raw[1..].find('\'') {
                let path_part = &raw[1..=end_quote];
                let rest_part = raw[end_quote + 2..].trim();
                let prompt = if rest_part.is_empty() { None } else { Some(rest_part.to_string()) };
                return (clean_str(path_part), prompt);
            }
        }

        // 2. Test if entire input (cleaned) exists
        let full_cleaned = clean_str(raw);
        if full_cleaned.exists() {
            return (full_cleaned, None);
        }

        // 3. Tokenize respecting backslash-escaped spaces (`\ `)
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next_c) = chars.peek() {
                    if next_c == ' ' {
                        current_token.push('\\');
                        current_token.push(' ');
                        chars.next();
                        continue;
                    }
                }
                current_token.push('\\');
            } else if c == ' ' {
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
            } else {
                current_token.push(c);
            }
        }
        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        // Scan from right to left to find the longest token sequence that exists on disk
        for k in (1..=tokens.len()).rev() {
            let candidate_str = tokens[..k].join(" ");
            let candidate_path = clean_str(&candidate_str);
            if candidate_path.exists() {
                let prompt_str = tokens[k..].join(" ").trim().to_string();
                let prompt = if prompt_str.is_empty() { None } else { Some(prompt_str) };
                return (candidate_path, prompt);
            }
        }

        // Fallback: first token is path, remainder is prompt
        if !tokens.is_empty() {
            let first_path = clean_str(&tokens[0]);
            let prompt_str = tokens[1..].join(" ").trim().to_string();
            let prompt = if prompt_str.is_empty() { None } else { Some(prompt_str) };
            (first_path, prompt)
        } else {
            (clean_str(raw), None)
        }
    }
}
