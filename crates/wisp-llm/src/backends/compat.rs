use std::collections::HashMap;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value};
use wisp_keyring::KeyManager;

use crate::backend::{LlmBackend, StreamOutcome, StreamRequest, ToolChoice};
use crate::error::LlmError;
use crate::sse;

pub struct OpenAiCompatBackend;

#[async_trait]
impl LlmBackend for OpenAiCompatBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        let base_url = req.provider.base_url.trim_end_matches('/').to_string();
        let url = format!("{base_url}/chat/completions");

        let key_manager = KeyManager::new("wisp".to_string());
        let api_key = key_manager
            .get_api_key(&req.provider.name)
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map_err(|e| LlmError::Other(format!("API key not found: {e}")))?;

        let mut body = json!({
            "model": req.model,
            "messages": req.messages,
            "stream": true,
        });

        if let Some(params) = &req.parameters {
            apply_parameters(&mut body, params);
        } else {
            body["max_tokens"] = json!(1024);
        }

        if !req.tools.is_empty() {
            let tools_json: Vec<Value> = req
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
            body["tool_choice"] = match &req.tool_choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("required"),
                ToolChoice::Specific(name) => {
                    json!({"type": "function", "function": {"name": name}})
                }
            };
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, body });
        }

        let mut byte_stream = response.bytes_stream();
        let mut outcome = StreamOutcome::default();
        let mut buf: Vec<u8> = Vec::new();
        let mut data_acc: String = String::new();
        let mut done = false;

        while !done {
            if req.cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }

            loop {
                if buf.iter().any(|&b| b == b'\n') {
                    break;
                }
                match byte_stream.next().await {
                    Some(Ok(chunk)) => buf.extend_from_slice(&chunk),
                    Some(Err(e)) => return Err(LlmError::Http(e)),
                    None => break,
                }
            }

            let nl = match buf.iter().position(|&b| b == b'\n') {
                Some(i) => i,
                None => {
                    if buf.is_empty() && data_acc.is_empty() {
                        break;
                    }
                    buf.push(b'\n');
                    buf.len() - 1
                }
            };
            let line = {
                let bytes = buf.drain(..=nl).collect::<Vec<u8>>();
                let s = std::str::from_utf8(&bytes).unwrap_or("");
                s.strip_suffix('\r').unwrap_or(s).to_string()
            };

            if line.is_empty() {
                if data_acc.is_empty() {
                    continue;
                }
                let event = reqwest_sse::Event {
                    event_type: "message".to_string(),
                    data: std::mem::take(&mut data_acc),
                    last_event_id: None,
                    retry: None,
                };
                if sse::is_done(&event) {
                    done = true;
                    continue;
                }
                let parsed = sse::parse_data_json(&event)?;
                if !parsed.is_null() {
                    if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) =
                                    delta.get("content").and_then(|c| c.as_str())
                                {
                                    outcome.text.push_str(content);
                                    (req.callbacks.on_content)(content);
                                }
                                if let Some(reasoning) = delta
                                    .get("reasoning_content")
                                    .and_then(|c| c.as_str())
                                {
                                    outcome.reasoning.push_str(reasoning);
                                    (req.callbacks.on_reasoning)(reasoning);
                                }
                                if let Some(reasoning_details) =
                                    delta.get("reasoning_details").and_then(|c| c.as_array())
                                {
                                    for detail in reasoning_details {
                                        if let Some(text) =
                                            detail.get("text").and_then(|t| t.as_str())
                                        {
                                            outcome.reasoning.push_str(text);
                                            (req.callbacks.on_reasoning)(text);
                                        }
                                    }
                                }
                                if let Some(tool_calls) =
                                    delta.get("tool_calls").and_then(|c| c.as_array())
                                {
                                    for tc in tool_calls {
                                        outcome.tool_call_deltas.push(tc.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                if !data_acc.is_empty() {
                    data_acc.push('\n');
                }
                data_acc.push_str(data);
            }
        }

        Ok(outcome)
    }
}

fn apply_parameters(body: &mut Value, params: &HashMap<String, Value>) {
    if let Some(temp) = params.get("temperature").and_then(|v| v.as_f64()) {
        body["temperature"] = json!(temp as f32);
    }
    if let Some(top_p) = params.get("top_p").and_then(|v| v.as_f64()) {
        body["top_p"] = json!(top_p as f32);
    }
    if let Some(max_tokens) = params.get("max_tokens").and_then(|v| v.as_i64()) {
        body["max_tokens"] = json!(max_tokens as u32);
    } else {
        body["max_tokens"] = json!(1024u32);
    }
    if let Some(penalty) = params.get("presence_penalty").and_then(|v| v.as_f64()) {
        body["presence_penalty"] = json!(penalty as f32);
    }
    if let Some(penalty) = params.get("frequency_penalty").and_then(|v| v.as_f64()) {
        body["frequency_penalty"] = json!(penalty as f32);
    }
    if let Some(seed) = params.get("seed").and_then(|v| v.as_i64()) {
        body["seed"] = json!(seed as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_parameters_sets_max_tokens_default() {
        let mut body = json!({"model": "test", "messages": [], "stream": true});
        let params = HashMap::new();
        apply_parameters(&mut body, &params);
        assert_eq!(body["max_tokens"], json!(1024u32));
    }

    #[test]
    fn apply_parameters_respects_explicit_max_tokens() {
        let mut body = json!({});
        let mut params = HashMap::new();
        params.insert("max_tokens".to_string(), json!(4096));
        apply_parameters(&mut body, &params);
        assert_eq!(body["max_tokens"], json!(4096u32));
    }

    #[test]
    fn apply_parameters_sets_temperature() {
        let mut body = json!({});
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!(0.5));
        apply_parameters(&mut body, &params);
        assert_eq!(body["temperature"], json!(0.5f32));
    }
}
