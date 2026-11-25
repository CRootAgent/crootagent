use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSession {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub provider: String,
    pub model: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbDocument {
    pub id: String,
    pub session_id: String,
    pub file_path: String,
    pub file_type: String,
    pub file_size: i64,
    pub chunk_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbChunk {
    pub id: i64,
    pub document_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub token_estimate: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbProvider {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub is_free: bool,
    pub base_url: String,
    pub default_model: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTestLog {
    pub id: i64,
    pub session_id: String,
    pub test_name: String,
    pub status: String,
    pub output: String,
    pub duration_ms: i64,
    pub timestamp: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open_default() -> Result<Self> {
        let dir = Config::config_dir();
        fs::create_dir_all(&dir)?;
        let db_path = dir.join("lontar.db");
        Self::open(db_path)
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        db.seed_free_providers()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        db.seed_free_providers()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                total_tokens INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                file_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS document_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                token_estimate INTEGER NOT NULL,
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                display_name TEXT NOT NULL,
                is_free INTEGER NOT NULL DEFAULT 1,
                base_url TEXT NOT NULL,
                default_model TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 10
            );

            CREATE TABLE IF NOT EXISTS test_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                test_name TEXT NOT NULL,
                status TEXT NOT NULL,
                output TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    fn seed_free_providers(&self) -> Result<()> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM providers",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            let defaults = [
                ("groq", "Groq Cloud", 1, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", 1),
                ("gemini", "Google Gemini", 1, "https://generativelanguage.googleapis.com", "gemini-1.5-flash", 2),
                ("openrouter", "OpenRouter Free", 1, "https://openrouter.ai/api/v1", "meta-llama/llama-3.3-70b-instruct:free", 3),
                ("deepseek", "DeepSeek", 1, "https://api.deepseek.com/v1", "deepseek-chat", 4),
                ("github", "GitHub Models (Free Tier)", 1, "https://models.inference.ai.azure.com", "gpt-4o-mini", 5),
                ("sambanova", "SambaNova Cloud", 1, "https://api.sambanova.ai/v1", "Meta-Llama-3.3-70B-Instruct", 6),
                ("mistral", "Mistral NeMo", 1, "https://api.mistral.ai/v1", "mistral-nemo", 7),
                ("ollama", "Ollama Local (Offline)", 1, "http://localhost:11434/v1", "llama3.2", 8),
            ];

            for (id, name, is_free, url, model, prio) in defaults {
                self.conn.execute(
                    "INSERT OR IGNORE INTO providers (id, name, display_name, is_free, base_url, default_model, priority)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, id, name, is_free, url, model, prio],
                )?;
            }
        }
        Ok(())
    }

    // --- Session CRUD ---
    pub fn insert_session(&self, session: &DbSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, name, created_at, updated_at, provider, model, total_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session.id,
                session.name,
                session.created_at,
                session.updated_at,
                session.provider,
                session.model,
                session.total_tokens
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<DbSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, provider, model, total_tokens
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(DbSession {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                total_tokens: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_sessions(&self) -> Result<Vec<DbSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, updated_at, provider, model, total_tokens
             FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbSession {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                total_tokens: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_session(&self, id: &str) -> Result<bool> {
        let count = self.conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    // --- Document CRUD ---
    pub fn insert_document(&self, doc: &DbDocument, chunks: &[DbChunk]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO documents (id, session_id, file_path, file_type, file_size, chunk_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                doc.id,
                doc.session_id,
                doc.file_path,
                doc.file_type,
                doc.file_size,
                doc.chunk_count,
                doc.created_at
            ],
        )?;

        for chunk in chunks {
            self.conn.execute(
                "INSERT INTO document_chunks (document_id, chunk_index, content, token_estimate)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    doc.id,
                    chunk.chunk_index,
                    chunk.content,
                    chunk.token_estimate
                ],
            )?;
        }
        Ok(())
    }

    pub fn list_documents(&self, session_id: &str) -> Result<Vec<DbDocument>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, file_path, file_type, file_size, chunk_count, created_at
             FROM documents WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(DbDocument {
                id: row.get(0)?,
                session_id: row.get(1)?,
                file_path: row.get(2)?,
                file_type: row.get(3)?,
                file_size: row.get(4)?,
                chunk_count: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_document_chunks(&self, doc_id: &str) -> Result<Vec<DbChunk>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, document_id, chunk_index, content, token_estimate
             FROM document_chunks WHERE document_id = ?1 ORDER BY chunk_index ASC",
        )?;
        let rows = stmt.query_map(params![doc_id], |row| {
            Ok(DbChunk {
                id: row.get(0)?,
                document_id: row.get(1)?,
                chunk_index: row.get(2)?,
                content: row.get(3)?,
                token_estimate: row.get(4)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn clear_session_documents(&self, session_id: &str) -> Result<()> {
        let docs = self.list_documents(session_id)?;
        for doc in docs {
            self.conn.execute("DELETE FROM document_chunks WHERE document_id = ?1", params![doc.id])?;
        }
        self.conn.execute("DELETE FROM documents WHERE session_id = ?1", params![session_id])?;
        Ok(())
    }

    // --- Provider CRUD ---
    pub fn list_providers(&self) -> Result<Vec<DbProvider>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, display_name, is_free, base_url, default_model, priority
             FROM providers ORDER BY priority ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_free_int: i64 = row.get(3)?;
            Ok(DbProvider {
                id: row.get(0)?,
                name: row.get(1)?,
                display_name: row.get(2)?,
                is_free: is_free_int == 1,
                base_url: row.get(4)?,
                default_model: row.get(5)?,
                priority: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Test Log CRUD (For Academic Code Coverage Proof) ---
    pub fn insert_test_log(&self, log: &DbTestLog) -> Result<()> {
        self.conn.execute(
            "INSERT INTO test_logs (session_id, test_name, status, output, duration_ms, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                log.session_id,
                log.test_name,
                log.status,
                log.output,
                log.duration_ms,
                log.timestamp
            ],
        )?;
        Ok(())
    }

    pub fn list_test_logs(&self) -> Result<Vec<DbTestLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, test_name, status, output, duration_ms, timestamp
             FROM test_logs ORDER BY id DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbTestLog {
                id: row.get(0)?,
                session_id: row.get(1)?,
                test_name: row.get(2)?,
                status: row.get(3)?,
                output: row.get(4)?,
                duration_ms: row.get(5)?,
                timestamp: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }
}
