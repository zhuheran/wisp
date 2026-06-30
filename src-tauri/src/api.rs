use std::collections::HashMap;
use std::error::Error;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
        ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
        ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionCall, ImageDetail,
        ImageUrl,
    },
    Client,
};
use futures::StreamExt;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use crate::{configs::provider::Provider};

use super::key_manager::KeyManager;


/// Creates a configured OpenAI client with custom parameters
fn get_openai_client(
    base_url: String,
    api_key: String,
) -> Result<Client<OpenAIConfig>, Box<dyn Error>> {
    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);
    Ok(Client::with_config(config))
}

/// Converts generic message values to OpenAI-compatible message types
/// Supports multimodal content (text + images)
fn convert_messages(
    messages: Vec<Value>,
) -> Result<Vec<ChatCompletionRequestMessage>, Box<dyn Error>> {
    messages
        .into_iter()
        .map(|msg| {
            let role = msg["role"].as_str().ok_or("Missing role")?;
            let content = &msg["content"];

            match role {
                "user" => {
                    // Check if content is an array (multimodal) or string (text only)
                    if let Some(content_array) = content.as_array() {
                        // Multimodal content: array of text and image parts
                        let parts: Result<Vec<ChatCompletionRequestUserMessageContentPart>, Box<dyn Error>> = content_array
                            .iter()
                            .map(|part| -> Result<ChatCompletionRequestUserMessageContentPart, Box<dyn Error>> {
                                let part_type = part["type"].as_str().ok_or("Missing part type")?;
                                match part_type {
                                    "text" => {
                                        let text = part["text"].as_str().ok_or("Missing text content")?;
                                        Ok(ChatCompletionRequestUserMessageContentPart::Text(
                                            ChatCompletionRequestMessageContentPartText {
                                                text: text.to_string(),
                                            }
                                        ))
                                    }
                                    "image_url" => {
                                        let image_url = part["image_url"]["url"].as_str().ok_or("Missing image URL")?;
                                        Ok(ChatCompletionRequestUserMessageContentPart::ImageUrl(
                                            ChatCompletionRequestMessageContentPartImage {
                                                image_url: ImageUrl {
                                                    url: image_url.to_string(),
                                                    detail: Some(ImageDetail::Auto),
                                                },
                                            }
                                        ))
                                    }
                                    _ => Err("Unsupported content part type".into()),
                                }
                            })
                            .collect();
                        
                        Ok(ChatCompletionRequestMessage::User(
                            ChatCompletionRequestUserMessage {
                                content: ChatCompletionRequestUserMessageContent::Array(parts?),
                                ..Default::default()
                            },
                        ))
                    } else {
                        // Simple text content
                        let text = content.as_str().ok_or("Missing content")?;
                        Ok(ChatCompletionRequestMessage::User(
                            ChatCompletionRequestUserMessage {
                                content: ChatCompletionRequestUserMessageContent::Text(text.to_string()),
                                ..Default::default()
                            },
                        ))
                    }
                }
                "assistant" => {
                    let text = content.as_str().unwrap_or("");
                    let tool_calls = msg["toolCalls"].as_array().map(|calls| {
                        calls
                            .iter()
                            .map(|call| {
                                let arguments = call["arguments"].clone();
                                ChatCompletionMessageToolCall {
                                    id: call["id"].as_str().unwrap_or_default().to_string(),
                                    r#type: ChatCompletionToolType::Function,
                                    function: FunctionCall {
                                        name: call["name"].as_str().unwrap_or_default().to_string(),
                                        arguments: serde_json::to_string(&arguments)
                                            .unwrap_or_else(|_| "{}".to_string()),
                                    },
                                }
                            })
                            .collect::<Vec<_>>()
                    });

                    Ok(ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessage {
                            content: if text.is_empty() && tool_calls.is_some() {
                                None
                            } else {
                                Some(ChatCompletionRequestAssistantMessageContent::Text(
                                    text.to_string(),
                                ))
                            },
                            tool_calls,
                            ..Default::default()
                        },
                    ))
                }
                "system" => {
                    let text = content.as_str().ok_or("Missing content")?;
                    Ok(ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessage {
                            content: ChatCompletionRequestSystemMessageContent::Text(
                                text.to_string(),
                            ),
                            ..Default::default()
                        },
                    ))
                }
                "tool" => {
                    let text = if let Some(text) = content.as_str() {
                        text.to_string()
                    } else {
                        serde_json::to_string(content)?
                    };
                    let tool_call_id = msg["toolCallId"]
                        .as_str()
                        .ok_or("Missing toolCallId")?
                        .to_string();

                    Ok(ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessage {
                            content: ChatCompletionRequestToolMessageContent::Text(text),
                            tool_call_id,
                        },
                    ))
                }
                _ => Err("Unsupported role".into()),
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct OpenAiStreamEvents {
    pub content_chunk: &'static str,
    pub reasoning_chunk: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenAiStreamOutcome {
    pub text: String,
    pub reasoning: String,
}

impl OpenAiStreamEvents {
    pub const LEGACY: Self = Self {
        content_chunk: "openai_stream_chunk",
        reasoning_chunk: "openai_stream_chunk_reasoning",
    };

    pub const CONVERSATION: Self = Self {
        content_chunk: "conversation_stream_chunk",
        reasoning_chunk: "conversation_stream_reasoning",
    };
}

fn apply_chat_parameters(
    request_builder: &mut CreateChatCompletionRequestArgs,
    parameters: Option<HashMap<String, Value>>,
) {
    if let Some(params) = parameters {
        if let Some(temp) = params.get("temperature").and_then(|v| v.as_f64()) {
            request_builder.temperature(temp as f32);
        }

        if let Some(top_p) = params.get("top_p").and_then(|v| v.as_f64()) {
            request_builder.top_p(top_p as f32);
        }

        if let Some(max_tokens) = params.get("max_tokens").and_then(|v| v.as_i64()) {
            request_builder.max_tokens(max_tokens as u32);
        } else {
            request_builder.max_tokens(1024u32);
        }

        if let Some(penalty) = params.get("presence_penalty").and_then(|v| v.as_f64()) {
            request_builder.presence_penalty(penalty as f32);
        }

        if let Some(penalty) = params.get("frequency_penalty").and_then(|v| v.as_f64()) {
            request_builder.frequency_penalty(penalty as f32);
        }

        if let Some(seed) = params.get("seed").and_then(|v| v.as_i64()) {
            request_builder.seed(seed as i32);
        }
    } else {
        request_builder.max_tokens(1024u32);
    }
}

pub async fn stream_openai_messages(
    app_handle: AppHandle,
    messages: Vec<ChatCompletionRequestMessage>,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, Value>>,
    events: OpenAiStreamEvents,
) -> Result<OpenAiStreamOutcome, Box<dyn Error>> {
    let base_url = provider.base_url;
    let key_manager_local = KeyManager::new("wisp".to_string());
    let api_key = key_manager_local
        .get_api_key(&provider.name)
        .or_else(|_| std::env::var("OPENAI_API_KEY"))?;
    let client = get_openai_client(base_url, api_key)?;

    let mut args = CreateChatCompletionRequestArgs::default();
    args.model(model.clone()).messages(messages).stream(true);
    apply_chat_parameters(&mut args, parameters);

    let request = args.build()?;
    let mut stream = client.chat().create_stream(request).await?;
    let mut outcome = OpenAiStreamOutcome::default();

    while let Some(response) = stream.next().await {
        match response {
            Ok(ccr) => {
                for choice in ccr.choices {
                    if let Some(content) = choice.delta.content {
                        outcome.text.push_str(&content);
                        app_handle
                            .emit(events.content_chunk, content)
                            .map_err(|e| e.to_string())?;
                    }
                    if let Some(reasoning_content) = choice.delta.reasoning_content {
                        outcome.reasoning.push_str(&reasoning_content);
                        app_handle
                            .emit(events.reasoning_chunk, reasoning_content)
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error in stream chunk: {}", e);
                if let Some(raw_error) = e.source() {
                    eprintln!("Raw error: {}", raw_error);
                }
            }
        }
    }

    println!("[API] Stream completed for model: {}", model);
    Ok(outcome)
}

/// Streams chat completions from OpenAI-compatible API and emits chunks via Tauri.
/// This legacy entry point accepts frontend JSON messages. New Rust-owned
/// conversation flows should call `stream_openai_messages` with typed messages.
pub async fn ask_openai_stream(
    app_handle: AppHandle,
    messages: Vec<Value>,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, Value>>,
) -> Result<(), Box<dyn Error>> {
    let converted_messages = convert_messages(messages)?;
    stream_openai_messages(
        app_handle,
        converted_messages,
        model,
        provider,
        parameters,
        OpenAiStreamEvents::LEGACY,
    )
    .await
    .map(|_| ())
}
