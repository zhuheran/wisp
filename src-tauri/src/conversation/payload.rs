use std::error::Error;
use std::fmt;

use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
    ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
    ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
    ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
    ChatCompletionRequestUserMessageContentPart, ChatCompletionToolType, FunctionCall, ImageDetail,
    ImageUrl,
};

use crate::db::types::{Message, MessageRole};

use super::types::{ConversationToolCall, ConversationToolContent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationPayloadError {
    InvalidToolCalls { message_id: String, reason: String },
    MissingToolCallId { message_id: String },
}

impl fmt::Display for ConversationPayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConversationPayloadError::InvalidToolCalls { message_id, reason } => {
                write!(f, "invalid tool calls for message {message_id}: {reason}")
            }
            ConversationPayloadError::MissingToolCallId { message_id } => {
                write!(f, "missing tool_call_id for tool message {message_id}")
            }
        }
    }
}

impl Error for ConversationPayloadError {}

pub fn build_openai_messages(
    messages: &[Message],
) -> Result<Vec<ChatCompletionRequestMessage>, ConversationPayloadError> {
    let mut converted = Vec::with_capacity(messages.len());
    let mut pending_tool_call_ids: Vec<String> = Vec::new();

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message(message)),
            MessageRole::Assistant => {
                let tool_calls = parse_tool_calls(message)?;
                pending_tool_call_ids = tool_calls.iter().map(|call| call.id.clone()).collect();
                converted.push(convert_assistant_message(message, tool_calls));
            }
            MessageRole::System => converted.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(message.text.clone()),
                    ..Default::default()
                },
            )),
            MessageRole::Tool => {
                let tool_call_id = pending_tool_call_ids.first().cloned().ok_or_else(|| {
                    ConversationPayloadError::MissingToolCallId {
                        message_id: message.id.clone(),
                    }
                })?;
                pending_tool_call_ids.remove(0);
                converted.push(ChatCompletionRequestMessage::Tool(
                    ChatCompletionRequestToolMessage {
                        content: ChatCompletionRequestToolMessageContent::Text(message.text.clone()),
                        tool_call_id,
                    },
                ));
            }
        }
    }

    Ok(converted)
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
    tool_calls: Vec<ConversationToolCall>,
) -> ChatCompletionRequestMessage {
    let openai_tool_calls = if tool_calls.is_empty() {
        None
    } else {
        Some(
            tool_calls
                .into_iter()
                .map(|call| ChatCompletionMessageToolCall {
                    id: call.id,
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionCall {
                        name: call.name,
                        arguments: serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect(),
        )
    };

    let content = if message.text.is_empty() && openai_tool_calls.is_some() {
        None
    } else {
        Some(ChatCompletionRequestAssistantMessageContent::Text(
            message.text.clone(),
        ))
    };

    ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
        content,
        tool_calls: openai_tool_calls,
        ..Default::default()
    })
}

fn parse_tool_calls(message: &Message) -> Result<Vec<ConversationToolCall>, ConversationPayloadError> {
    let Some(raw) = &message.tool_calls else {
        return Ok(Vec::new());
    };

    serde_json::from_str::<Vec<ConversationToolCall>>(raw).map_err(|error| {
        ConversationPayloadError::InvalidToolCalls {
            message_id: message.id.clone(),
            reason: error.to_string(),
        }
    })
}

pub fn tool_result_text(call: &ConversationToolCall) -> String {
    let Some(result) = &call.result else {
        return String::new();
    };

    result
        .content
        .iter()
        .map(|content| match content {
            ConversationToolContent::Text { text } => text.clone(),
            ConversationToolContent::Image { .. } => "[Image]".to_string(),
            ConversationToolContent::Resource { uri, text, .. } => {
                text.clone().unwrap_or_else(|| format!("[Resource: {uri}]"))
            }
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
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
    fn builds_native_tool_history_for_second_round() {
        let assistant_tool_calls = serde_json::json!([
            {
                "id": "call_1",
                "name": "server:search",
                "arguments": { "query": "rust tauri" },
                "result": {
                    "content": [{ "type": "text", "text": "result text" }],
                    "isError": false
                }
            }
        ])
        .to_string();

        let messages = vec![
            message(MessageRole::User, "Search docs", None),
            message(MessageRole::Assistant, "", Some(assistant_tool_calls)),
            message(MessageRole::Tool, "result text", None),
        ];

        let converted = build_openai_messages(&messages).expect("payload builds");
        assert_eq!(converted.len(), 3);
        assert!(matches!(converted[0], ChatCompletionRequestMessage::User(_)));
        match &converted[1] {
            ChatCompletionRequestMessage::Assistant(msg) => {
                assert!(msg.content.is_none());
                let calls = msg.tool_calls.as_ref().expect("tool calls present");
                assert_eq!(calls[0].id, "call_1");
                assert_eq!(calls[0].function.name, "server:search");
                assert_eq!(calls[0].function.arguments, r#"{"query":"rust tauri"}"#);
            }
            other => panic!("expected assistant, got {other:?}"),
        }
        match &converted[2] {
            ChatCompletionRequestMessage::Tool(msg) => {
                assert_eq!(msg.tool_call_id, "call_1");
            }
            other => panic!("expected tool, got {other:?}"),
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

        let converted = build_openai_messages(&[msg]).expect("payload builds");
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
        let converted = build_openai_messages(&[message(MessageRole::Assistant, "hello", None)])
            .expect("payload builds");
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

    #[test]
    fn rejects_tool_message_without_prior_assistant_tool_call() {
        let err = build_openai_messages(&[message(MessageRole::Tool, "orphan result", None)])
            .expect_err("orphan tool message should fail");
        assert!(matches!(err, ConversationPayloadError::MissingToolCallId { .. }));
    }
}
