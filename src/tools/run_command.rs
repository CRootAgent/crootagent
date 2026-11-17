use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub struct RunCommandTool;

#[async_trait]
impl Tool for RunCommandTool {
    fn name(&self) -> &'static str {
        "run_command"
    }

    fn description(&self) -> &'static str {
        "Mengeksekusi perintah shell/terminal di sistem operasi lokal dan menangkap output stdout, stderr, serta exit code."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Perintah shell lengkap yang ingin dijalankan (mis. 'git status', 'cargo test')"
                },
                "cwd": {
                    "type": "string",
                    "description": "Direktori kerja tempat perintah dijalankan (opsional, default: direktori aktif saat ini)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Batas waktu eksekusi dalam detik (opsional, default: 60)"
                }
            },
            "required": ["command"]
        })
    }

    fn is_destructive(&self) -> bool {
        true
    }

    async fn execute(&self, args: &Value) -> Result<String> {
        let command_str = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Argumen 'command' wajib diisi"))?;

        let cwd = args.get("cwd").and_then(|v| v.as_str());
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", command_str]);
            c
        };

        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", command_str]);
            c
        };

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let run_future = async {
            let child = cmd.spawn().with_context(|| format!("Gagal spawn proses: {}", command_str))?;
            let output = child.wait_with_output().await.with_context(|| "Gagal menunggu proses selesai")?;
            Ok::<std::process::Output, anyhow::Error>(output)
        };

        let result = match timeout(Duration::from_secs(timeout_secs), run_future).await {
            Ok(output_res) => output_res?,
            Err(_) => {
                anyhow::bail!("Perintah melampaui batas waktu timeout ({} detik): {}", timeout_secs, command_str);
            }
        };

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let status = result.status.code().unwrap_or(-1);

        let mut output = String::new();
        output.push_str(&format!("Exit Code: {}\n", status));
        if !stdout.is_empty() {
            output.push_str("--- STDOUT ---\n");
            output.push_str(&stdout);
            output.push('\n');
        }
        if !stderr.is_empty() {
            output.push_str("--- STDERR ---\n");
            output.push_str(&stderr);
            output.push('\n');
        }

        Ok(output)
    }
}
