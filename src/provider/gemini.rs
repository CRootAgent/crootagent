use super::{LlmProvider, ProviderResponse};
use crate::session::{ChatMessage, ToolCall};
use crate::tools::ToolDefinition;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

pub struct GeminiProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self {
            api_key,
            model,
            base_url,
            client: Client::builder().build().unwrap_or_default(),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ProviderResponse> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            self.model,
            self.api_key
        );

        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                if let Some(c) = &msg.content {
                    system_instruction = Some(json!({
                        "parts": [{ "text": c }]
                    }));
                }
                continue;
            }

            if msg.role == "tool" {
                let content_str = msg.content.clone().unwrap_or_default();
                let parsed_resp: Value = serde_json::from_str(&content_str)
                    .unwrap_or_else(|_| json!({ "output": content_str }));
                let function_name = msg.tool_call_id.clone().unwrap_or_else(|| "tool".to_string());
                contents.push(json!({
                    "role": "function",
                    "parts": [{
                        "functionResponse": {
                            "name": function_name,
                            "response": parsed_resp
                        }
                    }]
                }));
                continue;
            }

            let role = if msg.role == "assistant" { "model" } else { "user" };
            let mut parts = Vec::new();

            if let Some(c) = &msg.content {
                if !c.is_empty() {
                    parts.push(json!({ "text": c }));
                }
            }

            if let Some(calls) = &msg.tool_calls {
                for call in calls {
                    let parsed_args: Value = serde_json::from_str(&call.arguments)
                        .unwrap_or_else(|_| json!({}));
                    parts.push(json!({
                        "functionCall": {
                            "name": call.name,
                            "args": parsed_args
                        }
                    }));
                }
            }

            contents.push(json!({
                "role": role,
                "parts": parts
            }));
        }

        let mut body = json!({
            "contents": contents,
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = sys;
        }

        if !tools.is_empty() {
            let decls: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    })
                })
                .collect();
            body["tools"] = json!([{
                "functionDeclarations": decls
            }]);
        }

        let res = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Gagal memanggil API Google Gemini di {}", self.base_url))?;

        let status = res.status();
        let res_text = res.text().await.unwrap_or_default();

        if !status.is_success() {
            anyhow::bail!("Error dari Google Gemini API (HTTP {}): {}", status, res_text);
        }

        let res_json: Value = serde_json::from_str(&res_text)
            .with_context(|| "Gagal parse respons JSON dari Gemini")?;

        let candidate = res_json
            .get("candidates")
            .and_then(|c| c.get(0))
            .ok_or_else(|| anyhow::anyhow!("Gemini tidak mengembalikan kandidat"))?;

        let parts = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array());

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        if let Some(parts_arr) = parts {
            for part in parts_arr {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(t.to_string());
                }
                if let Some(fcall) = part.get("functionCall") {
                    let name = fcall.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let args = fcall.get("args").unwrap_or(&json!({})).to_string();
                    let id = format!("call_{}", &Uuid::new_v4().to_string()[..8]);
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: args,
                    });
                }
            }
        }

        let content = if !text_parts.is_empty() {
            Some(text_parts.join("\n"))
        } else {
            None
        };

        let finish_reason = candidate
            .get("finishReason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        Ok(ProviderResponse {
            content,
            tool_calls: if !tool_calls.is_empty() { Some(tool_calls) } else { None },
            model_name: format!("gemini:{}", self.model),
            finish_reason,
        })
    }
}
