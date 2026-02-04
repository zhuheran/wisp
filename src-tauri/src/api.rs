use std::collections::HashMap;
use std::error::Error;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
		ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
		ChatCompletionRequestMessage,
        ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
        CreateChatCompletionRequestArgs,
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
fn convert_messages(
    messages: Vec<Value>,
) -> Result<Vec<ChatCompletionRequestMessage>, Box<dyn Error>> {
    messages
        .into_iter()
        .map(|msg| {
            let role = msg["role"].as_str().ok_or("Missing role")?;
            let content = msg["content"].as_str().ok_or("Missing content")?;

            match role {
                "user" => Ok(ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(content.to_string()),
                        ..Default::default()
                    },
                )),
                "assistant" => Ok(ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                            content.to_string(),
                        )),
                        ..Default::default()
                    },
                )),
                "system" => Ok(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(
                            content.to_string(),
                        ),
                        ..Default::default()
                    },
                )),
                // Add other roles (assistant, system) as needed
                _ => Err("Unsupported role".into()),
            }
        })
        .collect()
}

/// Streams chat completions from OpenAI-compatible API and emits chunks via Tauri
pub async fn ask_openai_stream(
    app_handle: AppHandle,
    messages: Vec<Value>,
    model: String,
	provider: Provider,
    parameters: Option<HashMap<String, Value>>,
) -> Result<(), Box<dyn Error>> {
	let base_url = provider.base_url;
	let key_manager_local = KeyManager::new("wisp".to_string());
	let api_key = key_manager_local.get_api_key(&provider.name).or_else(|_| std::env::var("OPENAI_API_KEY"))?;
	let client = get_openai_client(base_url, api_key)?;
    let converted_messages = convert_messages(messages)?;

    let mut args = CreateChatCompletionRequestArgs::default();
    args.model(model.clone())
        .messages(converted_messages)
        .stream(true);
    let request_builder = &mut args;

    // Apply custom parameters if provided
    if let Some(params) = parameters {
        // Apply temperature
        if let Some(temp) = params.get("temperature").and_then(|v| v.as_f64()) {
            request_builder.temperature(temp as f32);
        }
        
        // Apply top_p
        if let Some(top_p) = params.get("top_p").and_then(|v| v.as_f64()) {
            request_builder.top_p(top_p as f32);
        }
        
        // Apply max_tokens
        if let Some(max_tokens) = params.get("max_tokens").and_then(|v| v.as_i64()) {
            request_builder.max_tokens(max_tokens as u32);
        } else {
            // Default max_tokens
            request_builder.max_tokens(1024u32);
        }
        
        // Apply presence_penalty
        if let Some(penalty) = params.get("presence_penalty").and_then(|v| v.as_f64()) {
            request_builder.presence_penalty(penalty as f32);
        }
        
        // Apply frequency_penalty
        if let Some(penalty) = params.get("frequency_penalty").and_then(|v| v.as_f64()) {
            request_builder.frequency_penalty(penalty as f32);
        }
        
        // Apply seed if supported
        if let Some(seed) = params.get("seed").and_then(|v| v.as_i64()) {
            request_builder.seed(seed as i32);
        }
    } else {
        // Default max_tokens
        request_builder.max_tokens(1024u32);
    }

    let request = args.build()?;

    let mut stream = client.chat().create_stream(request).await?;

    while let Some(response) = stream.next().await {
		match response {
            Ok(ccr) => {
                for choice in ccr.choices {
                    if let Some(content) = choice.delta.content {
                        app_handle
                            .emit("openai_stream_chunk", content)
                            .map_err(|e| e.to_string())?;
                    }
					if let Some(reasoning_content) = choice.delta.reasoning_content {
                        app_handle
                            .emit("openai_stream_chunk_reasoning", reasoning_content)
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error in stream chunk: {}", e);
                // Log the raw error message for debugging
                if let Some(raw_error) = e.source() {
                    eprintln!("Raw error: {}", raw_error);
                }
            }
        }
    }

    Ok(())
}
