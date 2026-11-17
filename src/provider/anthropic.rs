use super::{LlmProvider, ProviderResponse};
use crate::session::{ChatMessage, ToolCall};
use crate::tools::ToolDefinition;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
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
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ProviderResponse> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let mut system_prompt = String::new();
        let mut formatted_messages = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                if let Some(c) = &msg.content {
                    if !system_prompt.is_empty() {
                        system_prompt.push_str("\n\n");
                    }
                    system_prompt.push_str(c);
                }
                continue;
            }

            if msg.role == "tool" {
                let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
                let content = msg.content.clone().unwrap_or_default();
                formatted_messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content
                    }]
                }));
                continue;
            }

            if msg.role == "assistant" && msg.tool_calls.is_some() {
                let mut blocks = Vec::new();
                if let Some(c) = &msg.content {
                    if !c.is_empty() {
                        blocks.push(json!({
                            "type": "text",
                            "text": c
                        }));
                    }
                }
                if let Some(calls) = &msg.tool_calls {
                    for call in calls {
                        let parsed_input: Value = serde_json::from_str(&call.arguments)
                            .unwrap_or_else(|_| json!({}));
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": call.id,
                            "name": call.name,
                            "input": parsed_input
                        }));
                    }
                }
                formatted_messages.push(json!({
                    "role": "assistant",
                    "content": blocks
                }));
                continue;
            }

            let role = if msg.role == "assistant" { "assistant" } else { "user" };
            formatted_messages.push(json!({
                "role": role,
                "content": msg.content.clone().unwrap_or_default()
            }));
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": formatted_messages,
        });

        if !system_prompt.is_empty() {
            body["system"] = json!(system_prompt);
        }

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
        }

        let res = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Gagal memanggil API Anthropic di {}", url))?;

        let status = res.status();
        let res_text = res.text().await.unwrap_or_default();

        if !status.is_success() {
            anyhow::bail!("Error dari Anthropic API (HTTP {}): {}", status, res_text);
        }

        let res_json: Value = serde_json::from_str(&res_text)
            .with_context(|| "Gagal parse respons JSON dari Anthropic")?;

        let content_arr = res_json.get("content").and_then(|c| c.as_array());

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        if let Some(blocks) = content_arr {
            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if block_type == "text" {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(t.to_string());
                    }
                } else if block_type == "tool_use" {
                    let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let input = block.get("input").unwrap_or(&json!({})).to_string();
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
            }
        }

        let content = if !text_parts.is_empty() {
            Some(text_parts.join("\n"))
        } else {
            None
        };

        let stop_reason = res_json
            .get("stop_reason")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());

        Ok(ProviderResponse {
            content,
            tool_calls: if !tool_calls.is_empty() { Some(tool_calls) } else { None },
            model_name: format!("anthropic:{}", self.model),
            finish_reason: stop_reason,
        })
    }
}
