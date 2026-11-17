use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Membuat atau menimpa file dengan konten yang diberikan. Otomatis membuat direktori induk jika belum ada."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relatif atau absolut ke file yang ingin ditulis"
                },
                "content": {
                    "type": "string",
                    "description": "Konten teks lengkap yang akan ditulis ke file"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Boleh menimpa jika file sudah ada (default: true)"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn is_destructive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &Value) -> Result<String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'path' wajib diisi"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'content' wajib diisi"))?;

        let overwrite = args
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let path = Path::new(path_str);

        if path.exists() && !overwrite {
            anyhow::bail!("File '{}' sudah ada dan overwrite diset ke false.", path_str);
        }

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Gagal membuat direktori induk {:?}", parent))?;
            }
        }

        fs::write(path, content)
            .with_context(|| format!("Gagal menulis file '{}'", path_str))?;

        Ok(format!("Berhasil menulis {} byte ke '{}'", content.len(), path_str))
    }
}
