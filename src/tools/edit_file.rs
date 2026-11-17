use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn description(&self) -> &'static str {
        "Melakukan surgical edit pada file dengan mencari teks target yang persis (exact match) dan menggantinya dengan replacement."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relatif atau absolut ke file yang ingin diedit"
                },
                "target": {
                    "type": "string",
                    "description": "Teks persis yang ingin diganti (harus ada di dalam file)"
                },
                "replacement": {
                    "type": "string",
                    "description": "Teks pengganti baru"
                }
            },
            "required": ["path", "target", "replacement"]
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

        let target = args
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'target' wajib diisi"))?;

        let replacement = args
            .get("replacement")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'replacement' wajib diisi"))?;

        let path = Path::new(path_str);
        if !path.exists() {
            anyhow::bail!("File '{}' tidak ditemukan.", path_str);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Gagal membaca file '{}'", path_str))?;

        if !content.contains(target) {
            anyhow::bail!(
                "Teks target tidak ditemukan di dalam '{}'. Pastikan whitespace dan karakter cocok persis.",
                path_str
            );
        }

        let occurrences = content.matches(target).count();
        let new_content = content.replace(target, replacement);

        fs::write(path, new_content)
            .with_context(|| format!("Gagal menyimpan perubahan ke '{}'", path_str))?;

        Ok(format!(
            "Berhasil mengganti {} kemunculan target pada '{}'",
            occurrences, path_str
        ))
    }
}
