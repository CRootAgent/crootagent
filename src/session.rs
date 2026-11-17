use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_attribution: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            model_attribution: None,
            timestamp: Utc::now(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            model_attribution: None,
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCall>>, model: Option<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_calls,
            tool_call_id: None,
            model_attribution: model,
            timestamp: Utc::now(),
        }
    }

    pub fn tool_response(tool_call_id: String, content: String) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            model_attribution: None,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub message_count: usize,
    pub updated_at: DateTime<Utc>,
}

pub struct SessionStore;

impl SessionStore {
    pub fn sessions_dir() -> PathBuf {
        Config::config_dir().join("sessions")
    }

    pub fn init() -> Result<()> {
        let dir = Self::sessions_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("Gagal membuat direktori session {:?}", dir))?;
        Ok(())
    }

    pub fn create(name: Option<String>, provider: String, model: String) -> Result<Session> {
        Self::init()?;
        let id = Uuid::new_v4().to_string();
        let display_name = name.unwrap_or_else(|| format!("Session-{}", &id[..8]));
        let session = Session {
            id,
            name: display_name,
            provider,
            model,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            messages: Vec::new(),
        };
        Self::save(&session)?;
        Ok(session)
    }

    pub fn save(session: &Session) -> Result<()> {
        Self::init()?;
        let path = Self::sessions_dir().join(format!("{}.json", session.id));
        let json = serde_json::to_string_pretty(session)
            .with_context(|| "Gagal serialisasi session")?;
        fs::write(&path, json)
            .with_context(|| format!("Gagal menulis file session ke {:?}", path))?;
        Ok(())
    }

    pub fn load(id_or_prefix: &str) -> Result<Session> {
        Self::init()?;
        let dir = Self::sessions_dir();
        let direct_path = dir.join(format!("{}.json", id_or_prefix));
        if direct_path.exists() {
            let content = fs::read_to_string(&direct_path)?;
            return Ok(serde_json::from_str(&content)?);
        }

        // Search prefix or name match
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(sess) = serde_json::from_str::<Session>(&content) {
                    if sess.id.starts_with(id_or_prefix) || sess.name == id_or_prefix {
                        return Ok(sess);
                    }
                }
            }
        }

        anyhow::bail!("Session dengan ID/nama '{}' tidak ditemukan.", id_or_prefix)
    }

    pub fn list() -> Result<Vec<SessionHeader>> {
        Self::init()?;
        let dir = Self::sessions_dir();
        let mut headers = Vec::new();

        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<Session>(&content) {
                            headers.push(SessionHeader {
                                id: session.id,
                                name: session.name,
                                provider: session.provider,
                                model: session.model,
                                message_count: session.messages.len(),
                                updated_at: session.updated_at,
                            });
                        }
                    }
                }
            }
        }

        headers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(headers)
    }

    pub fn delete(id: &str) -> Result<()> {
        let session = Self::load(id)?;
        let path = Self::sessions_dir().join(format!("{}.json", session.id));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn export_markdown(session: &Session, output_path: Option<String>) -> Result<PathBuf> {
        let path = output_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("crootagent-session-{}.md", &session.id[..8])));

        let mut md = String::new();
        md.push_str(&format!("# Session: {}\n\n", session.name));
        md.push_str(&format!("- **ID:** `{}`\n", session.id));
        md.push_str(&format!("- **Provider:** {}\n", session.provider));
        md.push_str(&format!("- **Model:** {}\n", session.model));
        md.push_str(&format!("- **Created:** {}\n", session.created_at.to_rfc3339()));
        md.push_str(&format!("- **Updated:** {}\n\n---\n\n", session.updated_at.to_rfc3339()));

        for msg in &session.messages {
            let role_badge = match msg.role.as_str() {
                "user" => "👤 User",
                "assistant" => "🤖 Assistant",
                "system" => "⚙️ System",
                "tool" => "🔧 Tool Result",
                _ => &msg.role,
            };
            md.push_str(&format!("### {}\n", role_badge));
            if let Some(attr) = &msg.model_attribution {
                md.push_str(&format!("*Model: {}*\n\n", attr));
            }
            if let Some(content) = &msg.content {
                md.push_str(&format!("{}\n\n", content));
            }
            if let Some(calls) = &msg.tool_calls {
                for call in calls {
                    md.push_str(&format!("*Tool Call:* `{}` (`{}`)\n```json\n{}\n```\n\n", call.name, call.id, call.arguments));
                }
            }
        }

        fs::write(&path, md)?;
        Ok(path)
    }
}
