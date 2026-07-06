use serde_json::{json, Value};

use crate::types::{ConversationToolCall, ConversationToolContent};
use wisp_db::types::{Message, MessageRole};
use wisp_llm::{ReasoningConfig, ReasoningPassback};

pub fn build_openai_messages(messages: &[Message]) -> Vec<Value> {
    build_openai_messages_with_reasoning(messages, &ReasoningConfig::default(), false)
}

pub fn build_openai_messages_with_reasoning(
    messages: &[Message],
    config: &ReasoningConfig,
    native_tools: bool,
) -> Vec<Value> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message(message)),
            MessageRole::Assistant => {
                converted.push(convert_assistant_message_with_policy(
                    message,
                    config,
                    native_tools,
                ));
            },
            MessageRole::System => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
            MessageRole::Tool => {
                if let Some(tool_call_id) = &message.tool_call_id {
                    converted.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": message.text,
                    }));
                } else {
                    converted.push(json!({
                        "role": "system",
                        "content": message.text,
                    }));
                }
            },
        }
    }

    converted
}

fn convert_user_message(message: &Message) -> Value {
    if let Some(images) = &message.images {
        if !images.is_empty() {
            let mut parts = vec![json!({
                "type": "text",
                "text": message.text,
            })];
            for image in images {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image.image_url.url,
                        "detail": "auto",
                    },
                }));
            }
            return json!({
                "role": "user",
                "content": parts,
            });
        }
    }
    json!({
        "role": "user",
        "content": message.text,
    })
}

fn convert_assistant_message_with_policy(
    message: &Message,
    config: &ReasoningConfig,
    native_tools: bool,
) -> Value {
    let text = reconstruct_tool_call_text(message, native_tools);

    let mut msg = serde_json::Map::new();
    msg.insert("role".to_string(), json!("assistant"));
    msg.insert("content".to_string(), json!(text));

    if let Some(tc_json) = &message.tool_calls {
        if let Ok(calls) = serde_json::from_str::<Vec<crate::types::ConversationToolCall>>(tc_json)
        {
            let openai_tool_calls: Vec<Value> = calls
                .iter()
                .map(|call| {
                    let args = serde_json::to_string(&call.arguments).unwrap_or_default();
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": args,
                        }
                    })
                })
                .collect();
            if !openai_tool_calls.is_empty() {
                msg.insert("tool_calls".to_string(), json!(openai_tool_calls));
            }
        }
    }

    let include_reasoning = match config.policy {
        ReasoningPassback::Never => false,
        ReasoningPassback::Always => message
            .reasoning
            .as_deref()
            .map(|r| !r.is_empty())
            .unwrap_or(false),
        ReasoningPassback::ToolTurnsOnly => message.tool_calls.is_some(),
    };

    if include_reasoning {
        let reasoning = message.reasoning.as_deref().unwrap_or("");
        msg.insert(config.field_name.to_string(), json!(reasoning));
    }

    Value::Object(msg)
}

fn reconstruct_tool_call_text(message: &Message, native_tools: bool) -> String {
    if native_tools {
        return message.text.clone();
    }
    if let Some(raw_calls) = &message.tool_calls {
        let simplified: Vec<Value> = serde_json::from_str::<Vec<Value>>(raw_calls)
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                json!({
                    "name": call.get("name"),
                    "arguments": call.get("arguments"),
                })
            })
            .collect();

        let tag = serde_json::to_string(&simplified).unwrap_or_default();
        if message.text.is_empty() {
            format!("<|tool_calls|>{tag}<|/tool_calls|>")
        } else {
            format!("{}\n<|tool_calls|>{tag}<|/tool_calls|>", message.text)
        }
    } else {
        message.text.clone()
    }
}

pub fn format_tool_result(call: &ConversationToolCall) -> String {
    result_text(call).unwrap_or_else(|| format!("**{}**\n\n> _No result_", call.name))
}

/// Extract the concatenated text content of a tool call result. Used by the
/// conversation engine to persist a minimal tool message; full markdown/LLM
/// rendering is owned by the integration layer (see `wisp_software_tools`).
fn result_text(call: &ConversationToolCall) -> Option<String> {
    let result = call.result.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    for content in &result.content {
        match content {
            ConversationToolContent::Text { text } if !text.is_empty() => parts.push(text.clone()),
            ConversationToolContent::Image { .. } => parts.push("[image]".to_string()),
            ConversationToolContent::Resource { uri, text, .. } => {
                parts.push(text.clone().unwrap_or_else(|| format!("[resource: {uri}]")));
            },
            _ => {},
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ConversationToolResult;
    use wisp_db::types::{ImageContent, ImageUrl};
    use wisp_llm::{ReasoningConfig, ReasoningPassback};

    fn message(role: MessageRole, text: &str, tool_calls: Option<String>) -> Message {
        Message {
            id: format!("{}_id", text.replace(' ', "_")),
            text: text.to_string(),
            reasoning: None,
            sender: role,
            timestamp: 1,
            tokens: None,
            embedding: None,
            images: None,
            tool_calls,
            tool_call_id: None,
            source: Default::default(),
            pal_id: None,
            pal_name: None,
        }
    }

    fn message_with_reasoning_and_tools(
        text: &str,
        reasoning: Option<&str>,
        tool_calls: Option<&str>,
    ) -> Message {
        let mut msg = message(MessageRole::Assistant, text, tool_calls.map(|s| s.to_string()));
        msg.reasoning = reasoning.map(|s| s.to_string());
        msg
    }

    #[test]
    fn assistant_message_is_sent_as_plain_text() {
        let messages = vec![
            message(MessageRole::User, "hello", None),
            message(MessageRole::Assistant, "hi there", None),
        ];
        let converted = build_openai_messages(&messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0]["role"], "user");
        assert_eq!(converted[1]["role"], "assistant");
        assert_eq!(converted[1]["content"], "hi there");
    }

    #[test]
    fn tool_message_becomes_system_message() {
        let messages = vec![message(
            MessageRole::Tool,
            "[Tool: search]\n[Result]\nfound",
            None,
        )];
        let converted = build_openai_messages(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "system");
        assert!(converted[0]["content"]
            .as_str()
            .unwrap()
            .contains("[Tool: search]"));
    }

    fn tool_call(
        name: &str,
        args: serde_json::Value,
        text: Option<&str>,
        is_error: bool,
    ) -> ConversationToolCall {
        ConversationToolCall {
            id: format!("call_{}", name),
            name: name.to_string(),
            arguments: args,
            qualified_name: None,
            result: Some(ConversationToolResult {
                content: text
                    .map(|t| vec![ConversationToolContent::Text { text: t.to_string() }])
                    .unwrap_or_default(),
                is_error,
            }),
        }
    }

    #[test]
    fn format_tool_result_extracts_result_text() {
        let call = tool_call(
            "get_weather",
            serde_json::json!({"location": "Hangzhou"}),
            Some("Sunny, 28°C"),
            false,
        );
        let out = format_tool_result(&call);
        assert!(out.contains("Sunny, 28°C"));
    }

    #[test]
    fn format_tool_result_missing_result_shows_placeholder() {
        let call = ConversationToolCall {
            id: "c".to_string(),
            name: "noop".to_string(),
            arguments: serde_json::json!({}),
            qualified_name: None,
            result: None,
        };
        let out = format_tool_result(&call);
        assert!(out.contains("_No result_"));
    }

    #[test]
    fn builds_multimodal_user_message_for_images() {
        let mut msg = message(MessageRole::User, "describe", None);
        msg.images = Some(vec![ImageContent {
            content_type: "image_url".to_string(),
            image_url: ImageUrl { url: "data:image/png;base64,abc".to_string() },
        }]);
        let converted = build_openai_messages(&[msg]);
        let content = converted[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
    }

    #[test]
    fn default_config_never_includes_reasoning() {
        let msg = message_with_reasoning_and_tools("answer", Some("thinking..."), None);
        let converted = build_openai_messages(&[msg]);
        assert!(converted[0].get("reasoning_content").is_none());
    }

    #[test]
    fn deepseek_includes_reasoning_on_tool_turns() {
        let msg = message_with_reasoning_and_tools(
            "answer",
            Some("thinking..."),
            Some(r#"[{"name":"x","arguments":{}}]"#),
        );
        let config = ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        assert_eq!(converted[0]["reasoning_content"], "thinking...");
    }

    #[test]
    fn deepseek_omits_reasoning_on_plain_turns() {
        let msg = message_with_reasoning_and_tools("answer", Some("thinking..."), None);
        let config = ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        assert!(converted[0].get("reasoning_content").is_none());
    }

    #[test]
    fn deepseek_empty_reasoning_on_tool_turns_when_none_stored() {
        let msg = message_with_reasoning_and_tools(
            "answer",
            None,
            Some(r#"[{"name":"x","arguments":{}}]"#),
        );
        let config = ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        assert_eq!(converted[0]["reasoning_content"], "");
    }

    #[test]
    fn compat_includes_reasoning_when_present() {
        let msg = message_with_reasoning_and_tools(
            "answer",
            Some("thinking..."),
            Some(r#"[{"name":"x","arguments":{}}]"#),
        );
        let config =
            ReasoningConfig { field_name: "reasoning_content", policy: ReasoningPassback::Always };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        assert_eq!(converted[0]["reasoning_content"], "thinking...");
    }

    #[test]
    fn compat_omits_reasoning_when_empty() {
        let msg = message_with_reasoning_and_tools("answer", Some(""), None);
        let config =
            ReasoningConfig { field_name: "reasoning_content", policy: ReasoningPassback::Always };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        assert!(converted[0].get("reasoning_content").is_none());
    }

    #[test]
    fn assistant_with_tool_calls_includes_openai_format() {
        let msg = message_with_reasoning_and_tools(
            "",
            Some("thinking..."),
            Some(r#"[{"id":"call_1","name":"get_weather","arguments":{"location":"Hangzhou"}}]"#),
        );
        let config = ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        };
        let converted = build_openai_messages_with_reasoning(&[msg], &config, false);
        let tool_calls = converted[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[0]["type"], "function");
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(tool_calls[0]["function"]["arguments"], r#"{"location":"Hangzhou"}"#);
    }

    #[test]
    fn native_tools_skips_tag_reconstruction_in_content() {
        let msg = message_with_reasoning_and_tools(
            "Let me check the weather",
            None,
            Some(r#"[{"id":"call_1","name":"get_weather","arguments":{"location":"Hangzhou"}}]"#),
        );
        let converted =
            build_openai_messages_with_reasoning(&[msg], &ReasoningConfig::default(), true);
        let content = converted[0]["content"].as_str().unwrap();
        assert_eq!(content, "Let me check the weather");
        assert!(!content.contains("<|tool_calls|>"));
        let tool_calls = converted[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn native_tools_with_empty_text_keeps_empty_content() {
        let msg = message_with_reasoning_and_tools(
            "",
            None,
            Some(r#"[{"id":"call_1","name":"search","arguments":{"q":"weather"}}]"#),
        );
        let converted =
            build_openai_messages_with_reasoning(&[msg], &ReasoningConfig::default(), true);
        let content = converted[0]["content"].as_str().unwrap();
        assert_eq!(content, "");
        assert!(!content.contains("<|tool_calls|>"));
        let tool_calls = converted[0]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "search");
    }

    #[test]
    fn custom_tools_still_reconstructs_tags() {
        let msg = message_with_reasoning_and_tools(
            "some text",
            None,
            Some(r#"[{"id":"call_1","name":"search","arguments":{"q":"weather"}}]"#),
        );
        let converted =
            build_openai_messages_with_reasoning(&[msg], &ReasoningConfig::default(), false);
        let content = converted[0]["content"].as_str().unwrap();
        assert!(content.contains("<|tool_calls|>"));
    }
}
