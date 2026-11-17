use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Membaca isi file teks dengan nomor baris. Mendukung pembacaan seluruh file atau rentang baris tertentu (start_line, end_line)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path relatif atau absolut ke file yang ingin dibaca"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Nomor baris awal (1-indexed, opsional)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "Nomor baris akhir (inklusif, opsional)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: &Value) -> Result<String> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'path' wajib diisi"))?;

        let path = Path::new(path_str);
        if !path.exists() {
            anyhow::bail!("File '{}' tidak ditemukan.", path_str);
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Gagal membaca file '{}'", path_str))?;

        let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let end_line = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize);

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = if start_line > 0 { start_line - 1 } else { 0 };
        let end = end_line.unwrap_or(total_lines).min(total_lines);

        if start >= total_lines && total_lines > 0 {
            anyhow::bail!(
                "start_line {} melebihi total baris file ({})",
                start_line,
                total_lines
            );
        }

        let mut output = String::new();
        output.push_str(&format!("--- Menampilkan baris {}-{} dari total {} baris di '{}' ---\n", start + 1, end, total_lines, path_str));

        for (idx, line) in lines[start..end].iter().enumerate() {
            output.push_str(&format!("{:4}: {}\n", start + idx + 1, line));
        }

        Ok(output)
    }
}
