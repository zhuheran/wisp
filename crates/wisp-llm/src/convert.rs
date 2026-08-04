//! Conversion between the OpenAI wire format (produced by
//! `wisp-conversation::payload`) and rig's `message::Message`, plus the
//! request parameter split (§5/§6 of the migration spec).

use std::collections::HashMap;

use rig_core::completion::ToolDefinition;
use rig_core::message::{AssistantContent, ImageDetail, Message, UserContent};
use rig_core::OneOrMany;
use serde_json::{json, Value};
use wisp_configs::provider::ApiType;

use crate::error::LlmError;

/// Convert OpenAI-wire role-tagged messages into rig `Message`s.
///
/// Handles the payload shapes produced by
/// `wisp-conversation::payload::build_openai_messages_with_reasoning`:
/// system / user (string or multimodal parts) / assistant (content,
/// `tool_calls`, `reasoning_content` or the `reasoning` alias) / tool
/// (`tool_call_id` + content).
pub fn convert_messages(messages: &[Value]) -> Result<Vec<Message>, LlmError> {
    let mut converted = Vec::with_capacity(messages.len());
    for raw in messages {
        let role = raw.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let message = match role {
            "system" => {
                let content =
                    raw.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                Message::system(content)
            },
            "user" => convert_user_message(raw)?,
            "assistant" => convert_assistant_message(raw)?,
            "tool" => {
                let call_id = raw
                    .get("tool_call_id")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| {
                        LlmError::Other("tool message missing tool_call_id".to_string())
                    })?;
                let content =
                    raw.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                Message::tool_result_with_call_id(call_id, Some(call_id.to_string()), content)
            },
            other => {
                return Err(LlmError::Other(format!("unknown message role '{other}'")))
            },
        };
        converted.push(message);
    }
    Ok(converted)
}

fn convert_user_message(raw: &Value) -> Result<Message, LlmError> {
    let content = raw.get("content");
    match content {
        Some(Value::String(text)) => Ok(Message::user(text.clone())),
        Some(Value::Array(parts)) => {
            let mut user_content = Vec::new();
            for part in parts {
                let kind = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match kind {
                    "text" => {
                        let text = part.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        user_content.push(UserContent::text(text));
                    },
                    "image_url" => {
                        let url = part
                            .get("image_url")
                            .and_then(|u| u.get("url"))
                            .and_then(|u| u.as_str())
                            .ok_or_else(|| {
                                LlmError::Other("image_url part missing url".to_string())
                            })?;
                        let detail = part.get("image_url").and_then(|u| u.get("detail"));
                        user_content.push(UserContent::image_url(url, None, image_detail(detail)));
                    },
                    other => {
                        return Err(LlmError::Other(format!(
                            "unknown user content part '{other}'"
                        )))
                    },
                }
            }
            let content = OneOrMany::many(user_content)
                .map_err(|_| LlmError::Other("user message has no content parts".to_string()))?;
            Ok(Message::User { content })
        },
        _ => Err(LlmError::Other("user message has no content".to_string())),
    }
}

fn convert_assistant_message(raw: &Value) -> Result<Message, LlmError> {
    let content_str = raw.get("content").and_then(|c| c.as_str()).unwrap_or("");
    let reasoning = raw
        .get("reasoning_content")
        .or_else(|| raw.get("reasoning"))
        .and_then(|r| r.as_str())
        .filter(|r| !r.is_empty());

    let mut parts: Vec<AssistantContent> = Vec::new();
    if !content_str.is_empty() {
        parts.push(AssistantContent::text(content_str));
    }
    if let Some(reasoning) = reasoning {
        parts.push(AssistantContent::reasoning(reasoning));
    }
    if let Some(tool_calls) = raw.get("tool_calls").and_then(|t| t.as_array()) {
        for call in tool_calls {
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let function = call.get("function");
            let name =
                function.and_then(|f| f.get("name")).and_then(|v| v.as_str()).unwrap_or("");
            let arguments = match function.and_then(|f| f.get("arguments")) {
                Some(Value::String(encoded)) => serde_json::from_str(encoded)
                    .unwrap_or(Value::Object(Default::default())),
                Some(value) => value.clone(),
                None => Value::Object(Default::default()),
            };
            parts.push(AssistantContent::tool_call(id, name, arguments));
        }
    }
    if parts.is_empty() {
        parts.push(AssistantContent::text(""));
    }
    let content = OneOrMany::many(parts)
        .map_err(|_| LlmError::Other("assistant message has no content".to_string()))?;
    Ok(Message::Assistant { id: None, content })
}

/// Split merged request parameters into rig `CompletionRequest` dedicated
/// fields and the `additional_params` blob.
///
/// `temperature` / `max_tokens` map onto dedicated fields and must never be
/// duplicated into `additional_params` (`#[serde(flatten)]` would emit
/// duplicate keys in the request body).
pub fn split_parameters(
    params: Option<&HashMap<String, Value>>,
) -> (Option<f64>, Option<u64>, Option<Value>) {
    let Some(params) = params else {
        return (None, None, None);
    };

    let mut temperature = None;
    let mut max_tokens = None;
    let mut additional = serde_json::Map::new();

    for (key, value) in params {
        match key.as_str() {
            "temperature" => {
                if let Some(t) = value.as_f64() {
                    temperature = Some(t);
                    continue;
                }
            },
            "max_tokens" => {
                if let Some(m) = value.as_u64() {
                    max_tokens = Some(m);
                    continue;
                }
            },
            _ => {},
        }
        additional.insert(key.clone(), value.clone());
    }

    let additional = if additional.is_empty() {
        None
    } else {
        Some(Value::Object(additional))
    };
    (temperature, max_tokens, additional)
}

/// DeepSeek always enables thinking today (`{"thinking": {"type": "enabled"}}`
/// is injected into the request body). Keep that behavior by merging the
/// marker into `additional_params` unless the caller already provided it.
pub fn ensure_deepseek_thinking(api_type: ApiType, additional: &mut Option<Value>) {
    if api_type != ApiType::DeepSeek {
        return;
    }
    let obj = additional.get_or_insert_with(|| json!({}));
    if !obj.is_object() {
        *obj = json!({});
    }
    if obj.get("thinking").is_none() {
        obj["thinking"] = json!({"type": "enabled"});
    }
}

/// Build a rig `CompletionRequest` from the OpenAI-wire messages and merged
/// parameters.
pub fn build_completion_request(
    messages: Vec<Value>,
    parameters: Option<HashMap<String, Value>>,
    tools: Vec<ToolDefinition>,
    tool_choice: Option<rig_core::message::ToolChoice>,
    api_type: ApiType,
) -> Result<rig_core::completion::CompletionRequest, LlmError> {
    let (temperature, max_tokens, mut additional) = split_parameters(parameters.as_ref());
    ensure_deepseek_thinking(api_type, &mut additional);
    Ok(rig_core::completion::CompletionRequest {
        model: None,
        preamble: None,
        chat_history: OneOrMany::many(convert_messages(&messages)?)
            .map_err(|_| LlmError::Other("no messages to send".to_string()))?,
        documents: Vec::new(),
        tools,
        temperature,
        max_tokens,
        tool_choice,
        additional_params: additional,
        output_schema: None,
        record_telemetry_content: false,
    })
}

fn image_detail(detail: Option<&Value>) -> Option<ImageDetail> {
    detail
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_core::message::{ToolResult, ToolResultContent};

    #[test]
    fn converts_system_message() {
        let msgs = vec![json!({"role": "system", "content": "You are helpful"})];
        let converted = convert_messages(&msgs).unwrap();
        assert_eq!(converted.len(), 1);
        assert!(matches!(
            &converted[0],
            Message::System { content } if content == "You are helpful"
        ));
    }

    #[test]
    fn converts_user_text_message() {
        let msgs = vec![json!({"role": "user", "content": "hello"})];
        let converted = convert_messages(&msgs).unwrap();
        assert!(matches!(
            &converted[0],
            Message::User { content } if content.first() == UserContent::text("hello")
        ));
    }

    #[test]
    fn converts_multimodal_user_message() {
        let msgs = vec![json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "describe"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc", "detail": "auto"}},
            ]
        })];
        let converted = convert_messages(&msgs).unwrap();
        let Message::User { content } = &converted[0] else {
            panic!("expected user message");
        };
        assert_eq!(content.len(), 2);
        assert_eq!(content.first(), UserContent::text("describe"));
        assert!(matches!(
            content.iter().nth(1),
            Some(UserContent::Image(image)) if image.detail == Some(ImageDetail::Auto)
        ));
    }

    #[test]
    fn converts_assistant_with_text_reasoning_and_tool_calls() {
        let msgs = vec![json!({
            "role": "assistant",
            "content": "let me check",
            "reasoning_content": "thinking...",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "get_weather", "arguments": "{\"location\":\"hz\"}"}
            }]
        })];
        let converted = convert_messages(&msgs).unwrap();
        let Message::Assistant { content, .. } = &converted[0] else {
            panic!("expected assistant message");
        };
        assert_eq!(content.len(), 3);
        assert_eq!(content.first(), AssistantContent::text("let me check"));
        assert!(matches!(
            content.iter().nth(1),
            Some(AssistantContent::Reasoning(r)) if r.display_text() == "thinking..."
        ));
        assert!(matches!(
            content.iter().nth(2),
            Some(AssistantContent::ToolCall(call))
                if call.function.name == "get_weather"
                    && call.function.arguments["location"] == "hz"
        ));
    }

    #[test]
    fn assistant_reasoning_alias_is_accepted() {
        let msgs = vec![json!({
            "role": "assistant",
            "content": "answer",
            "reasoning": "openrouter dialect"
        })];
        let converted = convert_messages(&msgs).unwrap();
        let Message::Assistant { content, .. } = &converted[0] else {
            panic!("expected assistant message");
        };
        assert!(matches!(
            content.iter().find(|c| matches!(c, AssistantContent::Reasoning(_))),
            Some(AssistantContent::Reasoning(r)) if r.display_text() == "openrouter dialect"
        ));
    }

    #[test]
    fn assistant_with_empty_content_and_no_tool_calls_emits_empty_text() {
        let msgs = vec![json!({"role": "assistant", "content": ""})];
        let converted = convert_messages(&msgs).unwrap();
        let Message::Assistant { content, .. } = &converted[0] else {
            panic!("expected assistant message");
        };
        assert_eq!(content.len(), 1);
        assert_eq!(content.first(), AssistantContent::text(""));
    }

    #[test]
    fn converts_tool_result_message() {
        let msgs = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "Sunny, 28C"
        })];
        let converted = convert_messages(&msgs).unwrap();
        let Message::User { content } = &converted[0] else {
            panic!("expected user message for tool result");
        };
        assert!(matches!(
            content.first(),
            UserContent::ToolResult(ToolResult { call_id, content, .. })
                if call_id.as_deref() == Some("call_1")
                    && content.first() == ToolResultContent::text("Sunny, 28C")
        ));
    }

    #[test]
    fn tool_message_without_call_id_is_rejected() {
        let msgs = vec![json!({"role": "tool", "content": "orphan result"})];
        assert!(convert_messages(&msgs).is_err());
    }

    #[test]
    fn temperature_and_max_tokens_go_to_dedicated_fields() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!(0.7));
        params.insert("max_tokens".to_string(), json!(2048));
        let (temperature, max_tokens, additional) = split_parameters(Some(&params));
        assert_eq!(temperature, Some(0.7));
        assert_eq!(max_tokens, Some(2048));
        // both keys are consumed by dedicated fields — never duplicated
        assert!(additional.is_none());
    }

    #[test]
    fn other_parameters_land_in_additional_params() {
        let mut params = HashMap::new();
        params.insert("top_p".to_string(), json!(0.9));
        params.insert("reasoning_effort".to_string(), json!("high"));
        let (temperature, max_tokens, additional) = split_parameters(Some(&params));
        assert_eq!(temperature, None);
        assert_eq!(max_tokens, None);
        let additional = additional.expect("params present");
        assert_eq!(additional["top_p"], json!(0.9));
        assert_eq!(additional["reasoning_effort"], "high");
    }

    #[test]
    fn empty_parameters_yield_no_fields() {
        let (temperature, max_tokens, additional) = split_parameters(None);
        assert_eq!((temperature, max_tokens, additional), (None, None, None));
    }

    #[test]
    fn non_numeric_temperature_is_ignored() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!("high"));
        let (temperature, _, _) = split_parameters(Some(&params));
        assert_eq!(temperature, None);
    }

    #[test]
    fn deepseek_injects_thinking_when_absent() {
        let mut additional = Some(json!({"reasoning_effort": "high"}));
        ensure_deepseek_thinking(ApiType::DeepSeek, &mut additional);
        assert_eq!(additional.unwrap()["thinking"], json!({"type": "enabled"}));
    }

    #[test]
    fn deepseek_keeps_explicit_thinking() {
        let mut additional = Some(json!({"thinking": {"type": "disabled"}}));
        ensure_deepseek_thinking(ApiType::DeepSeek, &mut additional);
        assert_eq!(additional.unwrap()["thinking"], json!({"type": "disabled"}));
    }

    #[test]
    fn non_deepseek_providers_do_not_get_thinking() {
        let mut additional = Some(json!({}));
        ensure_deepseek_thinking(ApiType::OpenAi, &mut additional);
        assert!(additional.unwrap().get("thinking").is_none());
    }

    #[test]
    fn build_request_never_duplicates_dedicated_fields() {
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!(0.3));
        params.insert("max_tokens".to_string(), json!(512));
        params.insert("thinking".to_string(), json!({"type": "enabled"}));
        let request = build_completion_request(
            vec![json!({"role": "user", "content": "hi"})],
            Some(params),
            Vec::new(),
            None,
            ApiType::DeepSeek,
        )
        .unwrap();
        assert_eq!(request.temperature, Some(0.3));
        assert_eq!(request.max_tokens, Some(512));
        let additional = request.additional_params.expect("thinking present");
        assert_eq!(additional["thinking"], json!({"type": "enabled"}));
        // dedicated fields are never re-emitted through additional_params
        assert!(additional.get("temperature").is_none());
        assert!(additional.get("max_tokens").is_none());
    }
}
