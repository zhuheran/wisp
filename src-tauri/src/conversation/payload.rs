use async_openai::types::{
    ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
    ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    ChatCompletionRequestUserMessageContentPart, ImageDetail, ImageUrl,
};

use crate::db::types::{Message, MessageRole};
use super::types::{ConversationToolCall, ConversationToolContent};

pub fn build_openai_messages(
    messages: &[Message],
) -> Vec<ChatCompletionRequestMessage> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message(message)),
            MessageRole::Assistant => {
                converted.push(convert_assistant_message(message));
            }
            MessageRole::System => converted.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(message.text.clone()),
                    ..Default::default()
                },
            )),
            MessageRole::Tool => {
                // 工具结果以 system role 发送，内含 AI 可理解的结构化文本
                converted.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(
                            message.text.clone(),
                        ),
                        ..Default::default()
                    },
                ));
            }
        }
    }

    converted
}

fn convert_user_message(message: &Message) -> ChatCompletionRequestMessage {
    if let Some(images) = &message.images {
        if !images.is_empty() {
            let mut parts = vec![ChatCompletionRequestUserMessageContentPart::Text(
                ChatCompletionRequestMessageContentPartText {
                    text: message.text.clone(),
                },
            )];

            for image in images {
                parts.push(ChatCompletionRequestUserMessageContentPart::ImageUrl(
                    ChatCompletionRequestMessageContentPartImage {
                        image_url: ImageUrl {
                            url: image.image_url.url.clone(),
                            detail: Some(ImageDetail::Auto),
                        },
                    },
                ));
            }

            return ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Array(parts),
                ..Default::default()
            });
        }
    }

    ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
        content: ChatCompletionRequestUserMessageContent::Text(message.text.clone()),
        ..Default::default()
    })
}

fn convert_assistant_message(
    message: &Message,
) -> ChatCompletionRequestMessage {
    // Reconstruct the full assistant text including <|tool_calls|> tags.
    // This is critical so the model sees what tool calls it made in previous rounds
    // and doesn't repeat them.
    let text = if let Some(raw_calls) = &message.tool_calls {
        // Parse stored tool calls and reconstruct minimal name/arguments format
        let simplified: Vec<serde_json::Value> = serde_json::from_str::<Vec<serde_json::Value>>(raw_calls)
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                serde_json::json!({
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
    };

    let mut msg = ChatCompletionRequestAssistantMessage {
        content: Some(ChatCompletionRequestAssistantMessageContent::Text(text)),
        ..Default::default()
    };

    ChatCompletionRequestMessage::Assistant(msg)
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
    use crate::db::types::{ImageContent, ImageUrl};

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
        }
    }

    #[test]
    fn assistant_message_is_sent_as_plain_text() {
        let messages = vec![
            message(MessageRole::User, "hello", None),
            message(MessageRole::Assistant, "hi there", None),
        ];

        let converted = build_openai_messages(&messages);
        assert_eq!(converted.len(), 2);
        assert!(matches!(converted[0], ChatCompletionRequestMessage::User(_)));
        match &converted[1] {
            ChatCompletionRequestMessage::Assistant(msg) => {
                assert!(msg.tool_calls.is_none());
                assert_eq!(
                    msg.content,
                    Some(ChatCompletionRequestAssistantMessageContent::Text(
                        "hi there".to_string()
                    ))
                );
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }

    #[test]
    fn tool_message_becomes_system_message() {
        let messages = vec![
            message(MessageRole::Tool, "[Tool: search]\n[Result]\nfound", None),
        ];

        let converted = build_openai_messages(&messages);
        assert_eq!(converted.len(), 1);
        match &converted[0] {
            ChatCompletionRequestMessage::System(msg) => {
                let text = match &msg.content {
                    ChatCompletionRequestSystemMessageContent::Text(t) => t.as_str(),
                    _ => panic!("expected text content"),
                };
                assert!(text.contains("[Tool: search]"));
                assert!(text.contains("found"));
            }
            other => panic!("expected system, got {other:?}"),
        }
    }

    #[test]
    fn builds_multimodal_user_message_for_images() {
        let mut msg = message(MessageRole::User, "describe", None);
        msg.images = Some(vec![ImageContent {
            content_type: "image_url".to_string(),
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
            },
        }]);

        let converted = build_openai_messages(&[msg]);
        match &converted[0] {
            ChatCompletionRequestMessage::User(msg) => match &msg.content {
                ChatCompletionRequestUserMessageContent::Array(parts) => {
                    assert_eq!(parts.len(), 2);
                }
                other => panic!("expected multimodal array, got {other:?}"),
            },
            other => panic!("expected user, got {other:?}"),
        }
    }

    #[test]
    fn keeps_normal_assistant_text_as_text_content() {
        let converted = build_openai_messages(&[message(MessageRole::Assistant, "hello", None)]);
        match &converted[0] {
            ChatCompletionRequestMessage::Assistant(msg) => {
                assert!(msg.tool_calls.is_none());
                assert_eq!(
                    msg.content,
                    Some(ChatCompletionRequestAssistantMessageContent::Text(
                        "hello".to_string()
                    ))
                );
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }
}
