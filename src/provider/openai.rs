use super::{LlmProvider, ProviderResponse};
use crate::session::{ChatMessage, ToolCall};
use crate::tools::ToolDefinition;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct OpenAiProvider {
    name: &'static str,
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(name: &'static str, api_key: String, model: String, base_url: String) -> Self {
        Self {
            name,
            api_key,
            model,
            base_url,
            client: Client::builder().build().unwrap_or_default(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ProviderResponse> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut formatted_messages = Vec::new();
        for msg in messages {
            let mut obj = json!({
                "role": msg.role,
            });

            if let Some(content) = &msg.content {
                obj["content"] = json!(content);
            } else if msg.role == "assistant" && msg.tool_calls.is_some() {
                obj["content"] = Value::Null;
            }

            if let Some(calls) = &msg.tool_calls {
                let tool_calls_json: Vec<Value> = calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": c.arguments
                            }
                        })
                    })
                    .collect();
                obj["tool_calls"] = json!(tool_calls_json);
            }

            if let Some(t_id) = &msg.tool_call_id {
                obj["tool_call_id"] = json!(t_id);
            }

            formatted_messages.push(obj);
        }

        let mut body = json!({
            "model": self.model,
            "messages": formatted_messages,
        });

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
            body["tool_choice"] = json!("auto");
        }

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        if !self.api_key.is_empty() && self.api_key != "ollama" {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let res = req
            .send()
            .await
            .with_context(|| format!("Gagal memanggil API provider {} di {}", self.name, url))?;

        let status = res.status();
        let res_text = res.text().await.unwrap_or_default();

        if !status.is_success() {
            anyhow::bail!(
                "Error dari provider {} (HTTP {}): {}",
                self.name,
                status,
                res_text
            );
        }

        let res_json: Value = serde_json::from_str(&res_text)
            .with_context(|| format!("Gagal parse respons JSON dari provider {}", self.name))?;

        let choice = res_json
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| anyhow::anyhow!("Respons provider tidak memuat choice"))?;

        let message = choice
            .get("message")
            .ok_or_else(|| anyhow::anyhow!("Choice tidak memuat message"))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        let tool_calls = message.get("tool_calls").and_then(|calls| {
            calls.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let id = item.get("id").and_then(|v| v.as_str())?.to_string();
                        let func = item.get("function")?;
                        let name = func.get("name").and_then(|v| v.as_str())?.to_string();
                        let arguments = func.get("arguments").and_then(|v| v.as_str())?.to_string();
                        Some(ToolCall { id, name, arguments })
                    })
                    .collect()
            })
        });

        let returned_model = res_json
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or(&self.model)
            .to_string();

        Ok(ProviderResponse {
            content,
            tool_calls,
            model_name: format!("{}:{}", self.name, returned_model),
            finish_reason,
        })
    }
}
