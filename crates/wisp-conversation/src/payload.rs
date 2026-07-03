use serde_json::{json, Value};

use wisp_db::types::{Message, MessageRole};
use crate::types::{ConversationToolCall, ConversationToolContent};

pub fn build_openai_messages_value(messages: &[Message]) -> Vec<Value> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message_value(message)),
            MessageRole::Assistant => converted.push(convert_assistant_message_value(message)),
            MessageRole::System => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
            MessageRole::Tool => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
        }
    }

    converted
}

fn convert_user_message_value(message: &Message) -> Value {
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

fn convert_assistant_message_value(message: &Message) -> Value {
    let text = reconstruct_tool_call_text(message);

    json!({
        "role": "assistant",
        "content": text,
    })
}

fn reconstruct_tool_call_text(message: &Message) -> String {
    if let Some(raw_calls) = &message.tool_calls {
        let simplified: Vec<Value> = serde_json::from_str::<Vec<Value>>(raw_calls)
            .unwrap_or_default()
            .into_iter()
            .map(|call| json!({
                "name": call.get("name"),
                "arguments": call.get("arguments"),
            }))
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

pub fn build_openai_messages_with_reasoning(
    messages: &[Message],
    include_reasoning_on_tool_turns: bool,
) -> Vec<Value> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message_value(message)),
            MessageRole::Assistant => {
                if include_reasoning_on_tool_turns {
                    converted.push(convert_assistant_message_deepseek(message));
                } else {
                    converted.push(convert_assistant_message_value(message));
                }
            }
            MessageRole::System => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
            MessageRole::Tool => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
        }
    }

    converted
}

fn convert_assistant_message_deepseek(message: &Message) -> Value {
    let mut msg = serde_json::Map::new();
    msg.insert("role".to_string(), json!("assistant"));

    let text = reconstruct_tool_call_text(message);
    msg.insert("content".to_string(), json!(text));

    if message.tool_calls.is_some() {
        let reasoning = message.reasoning.as_deref().unwrap_or("");
        msg.insert("reasoning_content".to_string(), json!(reasoning));
    }

    Value::Object(msg)
}

/// 格式化 tool call 的结果成 AI 可读的结构化文本（存储到 DB + 构建 system message 用）
pub fn format_tool_result(call: &ConversationToolCall) -> String {
    let result = match &call.result {
        Some(r) => r,
        None => return format!("[Tool: {}]\n[No result]", call.name),
    };

    let status = if result.is_error { "error" } else { "success" };
    let args_str = serde_json::to_string(&call.arguments).unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(format!("[Tool: {}]", call.name));
    lines.push(format!("Arguments: {}", args_str));
    lines.push(format!("Status: {}", status));

    let has_content = result.content.iter().any(|c| matches!(c, ConversationToolContent::Text { text } if !text.is_empty()));
    if has_content {
        lines.push(String::new());
        if result.is_error {
            lines.push("[Error]".to_string());
        } else {
            lines.push("[Result]".to_string());
        }
        for content in &result.content {
            match content {
                ConversationToolContent::Text { text } if !text.is_empty() => {
                    lines.push(text.clone());
                }
                ConversationToolContent::Image { .. } => {
                    lines.push("[Image]".to_string());
                }
                ConversationToolContent::Resource { uri, text, .. } => {
                    lines.push(text.clone().unwrap_or_else(|| format!("[Resource: {uri}]")));
                }
                _ => {}
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_db::types::{ImageContent, ImageUrl};

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
    fn value_assistant_message_is_sent_as_plain_text() {
        let messages = vec![
            message(MessageRole::User, "hello", None),
            message(MessageRole::Assistant, "hi there", None),
        ];
        let converted = build_openai_messages_value(&messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0]["role"], "user");
        assert_eq!(converted[1]["role"], "assistant");
        assert_eq!(converted[1]["content"], "hi there");
    }

    #[test]
    fn value_tool_message_becomes_system_message() {
        let messages = vec![
            message(MessageRole::Tool, "[Tool: search]\n[Result]\nfound", None),
        ];
        let converted = build_openai_messages_value(&messages);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["role"], "system");
        assert!(converted[0]["content"].as_str().unwrap().contains("[Tool: search]"));
    }

    #[test]
    fn value_builds_multimodal_user_message_for_images() {
        let mut msg = message(MessageRole::User, "describe", None);
        msg.images = Some(vec![ImageContent {
            content_type: "image_url".to_string(),
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
            },
        }]);
        let converted = build_openai_messages_value(&[msg]);
        let content = converted[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
    }

    #[test]
    fn value_keeps_normal_assistant_text_as_text_content() {
        let converted = build_openai_messages_value(&[message(MessageRole::Assistant, "hello", None)]);
        assert_eq!(converted[0]["role"], "assistant");
        assert_eq!(converted[0]["content"], "hello");
    }

    #[test]
    fn deepseek_includes_reasoning_on_tool_turns() {
        let msg = message_with_reasoning_and_tools(
            "answer",
            Some("thinking..."),
            Some(r#"[{"name":"x","arguments":{}}]"#),
        );
        let converted = build_openai_messages_with_reasoning(&[msg], true);
        assert_eq!(converted[0]["role"], "assistant");
        assert_eq!(converted[0]["reasoning_content"], "thinking...");
    }

    #[test]
    fn deepseek_omits_reasoning_on_plain_turns() {
        let msg = message_with_reasoning_and_tools("answer", Some("thinking..."), None);
        let converted = build_openai_messages_with_reasoning(&[msg], true);
        assert!(converted[0].get("reasoning_content").is_none());
    }

    #[test]
    fn deepseek_empty_reasoning_on_tool_turns_when_none_stored() {
        let msg = message_with_reasoning_and_tools(
            "answer",
            None,
            Some(r#"[{"name":"x","arguments":{}}]"#),
        );
        let converted = build_openai_messages_with_reasoning(&[msg], true);
        assert_eq!(converted[0]["reasoning_content"], "");
    }
}
