use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "Melihat daftar file dan folder dalam sebuah direktori, termasuk informasi ukuran file dan status direktori."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relatif atau absolut ke direktori yang ingin dilihat (default: '.')"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Kedalaman rekursi direktori (default: 1)"
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;

        let root_path = Path::new(path_str);
        if !root_path.exists() {
            anyhow::bail!("Direktori '{}' tidak ditemukan.", path_str);
        }

        let mut output = String::new();
        output.push_str(&format!("Direktori: {}\n", path_str));
        output.push_str("----------------------------------------\n");

        fn visit_dir(dir: &Path, depth: usize, max_depth: usize, out: &mut String) -> Result<()> {
            if depth > max_depth {
                return Ok(());
            }

            let mut entries = Vec::new();
            for entry in fs::read_dir(dir).with_context(|| format!("Gagal membaca {:?}", dir))? {
                entries.push(entry?);
            }

            // Sort dirs first, then files
            entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));

            let indent = "  ".repeat(depth);
            for entry in entries {
                let path = entry.path();
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();

                // Skip common noise
                if name_str.starts_with(".git") || name_str == "target" || name_str == "node_modules" {
                    continue;
                }

                if path.is_dir() {
                    out.push_str(&format!("{}📁 {}/\n", indent, name_str));
                    visit_dir(&path, depth + 1, max_depth, out)?;
                } else {
                    let metadata = entry.metadata().ok();
                    let size_str = metadata
                        .map(|m| format_size(m.len()))
                        .unwrap_or_else(|| "?? B".to_string());
                    out.push_str(&format!("{}📄 {} ({})\n", indent, name_str, size_str));
                }
            }

            Ok(())
        }

        visit_dir(root_path, 0, max_depth, &mut output)?;
        Ok(output)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
