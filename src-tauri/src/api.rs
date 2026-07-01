use std::collections::HashMap;
use std::error::Error;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionTool, CreateChatCompletionRequestArgs,
    },
    Client,
};
use futures::StreamExt;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use crate::{configs::provider::Provider};

use super::conversation::types::ConversationToolCall;
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

#[derive(Debug, Clone)]
pub struct OpenAiStreamEvents {
    pub content_chunk: &'static str,
    pub reasoning_chunk: &'static str,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenAiStreamChunkEvent {
    pub message_id: Option<String>,
    pub chunk: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiStreamOutcome {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ConversationToolCall>,
    pub saw_tool_call_delta: bool,
    pending_arguments: std::collections::HashMap<u32, String>,
    pending_names: std::collections::HashMap<u32, String>,
    pending_ids: std::collections::HashMap<u32, String>,
}

impl Default for OpenAiStreamOutcome {
    fn default() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
            saw_tool_call_delta: false,
            pending_arguments: std::collections::HashMap::new(),
            pending_names: std::collections::HashMap::new(),
            pending_ids: std::collections::HashMap::new(),
        }
    }
}

impl OpenAiStreamEvents {
    pub const CONVERSATION: Self = Self {
        content_chunk: "conversation_stream_chunk",
        reasoning_chunk: "conversation_stream_reasoning",
        message_id: None,
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

fn summarize_messages_for_debug(messages: &[ChatCompletionRequestMessage]) -> String {
    let preview = serde_json::to_string(messages).unwrap_or_else(|_| "<failed to serialize messages>".to_string());
    const MAX_CHARS: usize = 4000;
    let char_count = preview.chars().count();
    if char_count > MAX_CHARS {
        let truncated = preview.chars().take(MAX_CHARS).collect::<String>();
        format!("{}... [truncated {} chars]", truncated, char_count - MAX_CHARS)
    } else {
        preview
    }
}

pub async fn stream_openai_messages(
    app_handle: AppHandle,
    messages: Vec<ChatCompletionRequestMessage>,
    tools: Option<Vec<ChatCompletionTool>>,
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
    args.model(model.clone()).messages(messages.clone()).stream(true);
    if let Some(tools) = tools {
        args.tools(tools);
    }
    apply_chat_parameters(&mut args, parameters);

    let request = args.build()?;
    let mut stream = client.chat().create_stream(request).await.map_err(|error| {
        let message_summary = summarize_messages_for_debug(&messages);
        format!(
            "stream create failed for model '{}' (provider '{}'): {}\nMessages:\n{}",
            model,
            provider.name,
            error,
            message_summary,
        )
    })?;
    let mut outcome = OpenAiStreamOutcome::default();

    while let Some(response) = stream.next().await {
        match response {
            Ok(ccr) => {
                for choice in ccr.choices {
                    if let Some(tool_calls) = choice.delta.tool_calls {
                        outcome.saw_tool_call_delta = true;
                        for tool_call in tool_calls {
                            let index = tool_call.index;
                            let function = tool_call.function;
                            let name = function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default();
                            let arguments = function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default();

                            if let Some(id) = tool_call.id {
                                outcome.pending_ids.insert(index, id);
                            }
                            if !name.is_empty() {
                                outcome.pending_names.insert(index, name);
                            }
                            if !arguments.is_empty() {
                                outcome.pending_arguments.entry(index).or_default().push_str(&arguments);
                            }

                            let id = outcome.pending_ids.get(&index).cloned().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            let name = outcome.pending_names.get(&index).cloned().unwrap_or_default();
                            let pending_args = outcome.pending_arguments.get(&index).map(|s| s.as_str()).unwrap_or("");
                            let parsed_arguments = serde_json::from_str::<Value>(pending_args).unwrap_or(Value::Null);

                            if let Some(existing) = outcome.tool_calls.iter_mut().find(|call| call.id == id) {
                                existing.name = name;
                                if parsed_arguments.is_object() {
                                    existing.arguments = parsed_arguments;
                                }
                            } else {
                                let args = if parsed_arguments.is_object() { parsed_arguments } else { Value::Null };
                                outcome.tool_calls.push(ConversationToolCall {
                                    id,
                                    name,
                                    arguments: args,
                                    result: None,
                                    qualified_name: None,
                                });
                            }
                        }
                    }
                    if let Some(content) = choice.delta.content {
                        outcome.text.push_str(&content);
                        if events.message_id.is_some() {
                            app_handle
                                .emit(
                                    events.content_chunk,
                                    OpenAiStreamChunkEvent {
                                        message_id: events.message_id.clone(),
                                        chunk: content,
                                    },
                                )
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    if let Some(reasoning_content) = choice.delta.reasoning_content {
                        outcome.reasoning.push_str(&reasoning_content);
                        if events.message_id.is_some() {
                            app_handle
                                .emit(
                                    events.reasoning_chunk,
                                    OpenAiStreamChunkEvent {
                                        message_id: events.message_id.clone(),
                                        chunk: reasoning_content,
                                    },
                                )
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            Err(e) => {
                let message_summary = summarize_messages_for_debug(&messages);
                let mut error_text = format!(
                    "stream failed for model '{}' (provider '{}'): {}\nMessages:\n{}",
                    model,
                    provider.name,
                    e,
                    message_summary,
                );
                if let Some(raw_error) = e.source() {
                    error_text.push_str(&format!("\nRaw error: {}", raw_error));
                }
                return Err(error_text.into());
            }
        }
    }

    println!("[API] Stream completed for model: {}, tool_calls: {}", model, outcome.tool_calls.len());
    Ok(outcome)
}
