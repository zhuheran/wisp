use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use wisp_llm::{backend_for, StreamCallbacks, StreamRequest, ToolChoice};
use wisp_llm::ToolDefinition as LlmToolDefinition;
use wisp_configs::character::Character;
use wisp_configs::provider::Provider;
use wisp_configs::model::{ModelInfo, TextModelCapability};
use crate::abort::AbortRegistry;
use crate::orchestrator;
use wisp_conversation::payload::{
    build_openai_messages, build_openai_messages_with_reasoning, format_tool_result,
};
use wisp_conversation::tool_parser::parse_tool_calls;
use wisp_conversation::{ConversationToolCall, ConversationToolContent, ConversationToolResult};
use wisp_common::MessageSource;
use wisp_db::types::{ImageContent, Message, MessageRole};
use wisp_mcp::{ToolContent, ToolDefinition};
use crate::types::AppData;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationSendRequest {
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub text: String,
    pub images: Option<Vec<ImageContent>>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub character: Option<Character>,
    #[serde(default)]
    pub target_pal_ids: Option<Vec<String>>,
    #[serde(default)]
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationRegenerateRequest {
    pub conversation_id: String,
    pub message_id: String,
    pub insert_guidance: bool,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub character: Option<Character>,
    #[serde(default)]
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationDeriveRequest {
    pub conversation_id: String,
    pub replaced_message_id: String,
    pub text: String,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub character: Option<Character>,
    #[serde(default)]
    pub target_pal_ids: Option<Vec<String>>,
    #[serde(default)]
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEventPayload {
    MessageCreated { message: Message, parent_id: Option<String> },
    MessageUpdated { message_id: String, text: String, reasoning: Option<String>, tool_calls: Option<String> },
    Completed { leaf_message_id: String },
    Failed { error: String },
}

fn emit_event<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>, payload: ConversationEventPayload) -> Result<(), String> {
    app_handle
        .emit("conversation_event", payload)
        .map_err(|error| error.to_string())
}

fn insert_message_and_emit<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &mut AppData,
    conversation_id: &str,
    message: Message,
    parent_id: Option<&str>,
) -> Result<(), String> {
    let images_json = message
        .images
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;

    state
        .chat
        .add_message(
            conversation_id,
            &message.id,
            &message.text,
            message.reasoning.as_deref(),
            &message.sender.to_string(),
            parent_id,
            images_json.as_deref(),
            message.tool_calls.as_deref(),
        )
        .map_err(|error| error.to_string())?;

    emit_event(
        app_handle,
        ConversationEventPayload::MessageCreated {
            message,
            parent_id: parent_id.map(ToString::to_string),
        },
    )
}

async fn execute_tool_call<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    call: ConversationToolCall,
) -> Result<ConversationToolCall, String> {
    let registry = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state
            .lock()
            .map_err(|error| format!("Failed to acquire app state for tool {}: {}", call.name, error))?;
        std::sync::Arc::clone(&state.tool_registry)
    };

    let result = registry
        .execute(&call.name, call.arguments.clone())
        .await
        .map_err(|error| format!("Tool '{}' failed: {}", call.name, error))?;

    let tool_result = wisp_conversation::ConversationToolResult {
        content: result
            .content
            .into_iter()
            .map(|c| match c {
                ToolContent::Text { text } => {
                    wisp_conversation::ConversationToolContent::Text { text }
                }
                ToolContent::Image { data, mime_type } => {
                    wisp_conversation::ConversationToolContent::Image {
                        data,
                        mime_type,
                    }
                }
                ToolContent::Resource {
                    uri,
                    mime_type,
                    text,
                    blob,
                } => wisp_conversation::ConversationToolContent::Resource {
                    uri,
                    mime_type,
                    text,
                    blob,
                },
            })
            .collect(),
        is_error: result.is_error,
    };

    Ok(ConversationToolCall {
        result: Some(tool_result),
        ..call
    })
}

fn format_tool_parameter_line(name: &str, property: &serde_json::Value) -> String {
    let mut detail = property
        .get("description")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            property
                .get("type")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string());

    if let Some(enum_values) = property.get("enum").and_then(|value| value.as_array()) {
        let enum_values = enum_values
            .iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        if !enum_values.is_empty() {
            detail.push_str(&format!(" (enum: {})", enum_values.join(", ")));
        }
    }

    format!("      - {}: {}", name, detail)
}

fn build_enabled_tools_prompt(enabled_tools: &[ToolDefinition]) -> String {
    if enabled_tools.is_empty() {
        return String::new();
    }

    let mut tool_info: Vec<&ToolDefinition> = enabled_tools.iter().collect();
    tool_info.sort_by(|a, b| a.name.cmp(&b.name));

    let tool_lines: Vec<String> = tool_info
        .into_iter()
        .map(|tool| {
            let desc = tool
                .description
                .as_deref()
                .unwrap_or("No description");
            let mut lines = vec![format!("  - **{}**: {desc}", tool.name)];

            if let Some(props) = tool
                .input_schema
                .get("properties")
                .and_then(|v| v.as_object())
            {
                let mut prop_names: Vec<&String> = props.keys().collect();
                prop_names.sort();
                for prop_name in prop_names {
                    let prop = &props[prop_name];
                    let desc = prop
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let type_str = prop
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    lines.push(format!("    - `{prop_name}` ({type_str}): {desc}"));
                }
            }

            lines.join("\n")
        })
        .collect();

    format!(
        r#"## Available Tools

You have access to the following tools. Use them via <|tool_calls|> when appropriate.

### Tool List

{}

### How to Call

Wrap a JSON array of tool calls in `<|tool_calls|>` tags:

<|tool_calls|>
[{{"name":"tool_name","arguments":{{"param":"value"}}}}]
<|/tool_calls|>
"#,
        tool_lines.join("\n\n")
    )
}

async fn resolve_enabled_mcp_tools<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Vec<ToolDefinition>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|error| error.to_string())?;
    Ok(state.tool_registry.list_enabled_tools())
}

async fn run_conversation_rounds<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    conversation_id: String,
    current_leaf_id: String,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, serde_json::Value>>,
    character: Option<Character>,
    stream_id: String,
) -> Result<String, String> {
    let result = run_conversation_rounds_inner(
        &app_handle,
        conversation_id,
        current_leaf_id,
        model,
        provider,
        parameters,
        character,
        &stream_id,
    )
    .await;
    app_handle.state::<AbortRegistry>().unregister(&stream_id);
    result
}

async fn run_conversation_rounds_inner<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    conversation_id: String,
    mut current_leaf_id: String,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, serde_json::Value>>,
    character: Option<Character>,
    stream_id: &str,
) -> Result<String, String> {
    let pal_id = character.as_ref().map(|c| c.id.clone());
    let pal_name = character.as_ref().map(|c| c.name.clone());
    let registry = app_handle.state::<AbortRegistry>();
    let cancel = registry.register(stream_id);
    for round in 0..10 {
        let path = {
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex
                .lock()
                .map_err(|error| format!("Failed to acquire app state for conversation '{}': {}", conversation_id, error))?;
            state
                .chat
                .get_message_path_to(&conversation_id, &current_leaf_id)
                .map_err(|error| format!("Failed to build message path for conversation '{}' from leaf '{}': {}", conversation_id, current_leaf_id, error))?
        };

        let backend = backend_for(&provider);
        let reasoning_config = backend.reasoning_config();
        let mut openai_messages =
            build_openai_messages_with_reasoning(&path, &reasoning_config);

        let enabled_tools = resolve_enabled_mcp_tools(app_handle).await?;
        let supports_native_tools = provider
            .get_model(&model)
            .map(|m| match &m.model_info {
                ModelInfo::TextGeneration { capabilities, .. } => {
                    capabilities.contains(&TextModelCapability::ToolUse)
                }
                _ => false,
            })
            .unwrap_or(false);

        let tool_defs: Vec<LlmToolDefinition> = if supports_native_tools {
            enabled_tools
                .iter()
                .map(|t| LlmToolDefinition {
                    name: t.name.clone(),
                    description: t.description.clone().unwrap_or_default(),
                    parameters: t.input_schema.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };

        let tools_prompt = if supports_native_tools {
            String::new()
        } else {
            build_enabled_tools_prompt(&enabled_tools)
        };

        let mut system_prompt_sections = Vec::new();
        if let Some(character) = &character {
            if !character.system_prompt.trim().is_empty() {
                system_prompt_sections.push(character.system_prompt.trim().to_string());
            }
        }
        if !tools_prompt.is_empty() {
            system_prompt_sections.push(tools_prompt);
        }
        if !system_prompt_sections.is_empty() {
            openai_messages.insert(0, serde_json::json!({
                "role": "system",
                "content": system_prompt_sections.join("\n\n"),
            }));
        }

        let assistant_message_id = Uuid::new_v4().to_string();
        {
            let draft_message = Message {
                id: assistant_message_id.clone(),
                text: String::new(),
                reasoning: None,
                sender: MessageRole::Assistant,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                tokens: None,
                embedding: None,
                images: None,
                tool_calls: None,
                source: MessageSource::UserPrompted,
                pal_id: pal_id.clone(),
                pal_name: pal_name.clone(),
            };
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
            insert_message_and_emit(
                app_handle,
                &mut state,
                &conversation_id,
                draft_message,
                Some(&current_leaf_id),
            )?;
        }

        let assistant_msg_id = assistant_message_id.clone();
        let sid = stream_id.to_string();
        let ah = app_handle.clone();
        let callbacks = StreamCallbacks {
            on_content: Arc::new(move |chunk: &str| {
                let _ = ah.emit(
                    "conversation_stream_chunk",
                    serde_json::json!({
                        "stream_id": &sid,
                        "message_id": &assistant_msg_id,
                        "chunk": chunk,
                    }),
                );
            }),
            on_reasoning: Arc::new({
                let assistant_msg_id = assistant_message_id.clone();
                let sid = stream_id.to_string();
                let ah = app_handle.clone();
                move |chunk: &str| {
                    let _ = ah.emit(
                        "conversation_stream_reasoning",
                        serde_json::json!({
                            "stream_id": &sid,
                            "message_id": &assistant_msg_id,
                            "chunk": chunk,
                        }),
                    );
                }
            }),
        };

        let outcome = backend
            .stream(StreamRequest {
                messages: openai_messages,
                model: model.clone(),
                provider: provider.clone(),
                parameters: parameters.clone(),
                callbacks,
                cancel: cancel.clone(),
                tools: tool_defs,
                tool_choice: ToolChoice::Auto,
            })
            .await
            .map_err(|error| format!(
                "Model '{}' failed while streaming conversation '{}': {}",
                model, conversation_id, error
            ))?;

        let parsed = parse_tool_calls(&outcome.text);
        let native_calls = wisp_conversation::merge_tool_call_deltas(&outcome.tool_call_deltas);
        let mut calls = parsed
            .calls
            .into_iter()
            .filter(|call| !call.name.trim().is_empty())
            .filter(|call| call.arguments.is_object())
            .collect::<Vec<_>>();
        if !native_calls.is_empty() {
            calls = native_calls;
        }
        let assistant_message = Message {
            id: assistant_message_id.clone(),
            text: parsed.clean_text.clone(),
            reasoning: if outcome.reasoning.is_empty() {
                None
            } else {
                Some(outcome.reasoning.clone())
            },
            sender: MessageRole::Assistant,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            tokens: None,
            embedding: None,
            images: None,
            tool_calls: if calls.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&calls).map_err(|error| error.to_string())?)
            },
            source: MessageSource::UserPrompted,
            pal_id: pal_id.clone(),
            pal_name: pal_name.clone(),
        };

        {
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
            state
                .chat
                .update_message(&assistant_message_id, &assistant_message.text)
                .map_err(|error| error.to_string())?;
            if let Some(reasoning) = &assistant_message.reasoning {
                state
                    .chat
                    .messages_manager
                    .update_reasoning(&assistant_message_id, reasoning)
                    .map_err(|error| error.to_string())?;
            }
        }
        emit_event(
            app_handle,
            ConversationEventPayload::MessageUpdated {
                message_id: assistant_message_id.clone(),
                text: assistant_message.text.clone(),
                reasoning: assistant_message.reasoning.clone(),
                tool_calls: assistant_message.tool_calls.clone(),
            },
        )?;

        current_leaf_id = assistant_message_id.clone();

        if calls.is_empty() {
            emit_event(
                app_handle,
                ConversationEventPayload::Completed {
                    leaf_message_id: current_leaf_id.clone(),
                },
            )?;
            return Ok(current_leaf_id);
        }

        if round == 9 {
            emit_event(
                app_handle,
                ConversationEventPayload::Failed {
                    error: "Max tool rounds reached".to_string(),
                },
            )?;
            return Err(format!("Max tool rounds reached for conversation '{}'", conversation_id));
        }

        let mut completed_calls = Vec::new();
        for call in calls {
            completed_calls.push(execute_tool_call(app_handle, call).await?);
        }
        let completed_calls_json = serde_json::to_string(&completed_calls)
            .map_err(|error| format!("Failed to serialize completed tool calls for conversation '{}': {}", conversation_id, error))?;

        {
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
            state
                .chat
                .messages_manager
                .update_tool_calls(&assistant_message_id, &completed_calls_json)
                .map_err(|error| error.to_string())?;
        }
        emit_event(
            app_handle,
            ConversationEventPayload::MessageUpdated {
                message_id: assistant_message_id.clone(),
                text: parsed.clean_text,
                reasoning: assistant_message.reasoning.clone(),
                tool_calls: Some(completed_calls_json.clone()),
            },
        )?;

        for call in &completed_calls {
            let tool_message = Message {
                id: Uuid::new_v4().to_string(),
                text: format_tool_result(call),
                reasoning: None,
                sender: MessageRole::Tool,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                tokens: None,
                embedding: None,
                images: None,
                tool_calls: None,
                source: Default::default(),
                pal_id: None,
                pal_name: None,
            };

            {
                let state_mutex = app_handle.state::<Mutex<AppData>>();
                let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
                insert_message_and_emit(
                    app_handle,
                    &mut state,
                    &conversation_id,
                    tool_message.clone(),
                    Some(&assistant_message_id),
                )?;
            }
            current_leaf_id = tool_message.id;
        }
    }

    Err(format!("Max tool rounds reached for conversation '{}'", conversation_id))
}

/// Inner logic of conversation_send_message, generic over Runtime for
/// testability. Callers outside tests should use the
/// [`conversation_send_message`] command wrapper.
pub async fn conversation_send_message_inner<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    request: ConversationSendRequest,
) -> Result<String, String> {
    let stream_id = request.stream_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let user_message_id = Uuid::new_v4().to_string();
    let user_message = Message {
        id: user_message_id.clone(),
        text: request.text.clone(),
        reasoning: None,
        sender: MessageRole::User,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        tokens: None,
        embedding: None,
        images: request.images.clone(),
        tool_calls: None,
        source: MessageSource::UserPrompted,
        pal_id: None,
        pal_name: None,
    };

    {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
        insert_message_and_emit(
            app_handle,
            &mut state,
            &request.conversation_id,
            user_message,
            request.parent_message_id.as_deref(),
        )?;
    }

    // Update unlocked_pals map from target_pal_ids (if any)
    if let Some(ref target_pal_ids) = request.target_pal_ids {
        if !target_pal_ids.is_empty() {
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
            state
                .unlocked_pals
                .entry(request.conversation_id.clone())
                .or_default()
                .extend(target_pal_ids.iter().cloned());
        }
    }

    if let Some(target_pal_ids) = request.target_pal_ids {
        if !target_pal_ids.is_empty() {
            // Multi-pal orchestration path
            let characters = {
                let state_mutex = app_handle.state::<Mutex<AppData>>();
                let state = state_mutex.lock().map_err(|error| error.to_string())?;
                state.config_manager.get_characters()
            };

            let replies = orchestrator::orchestrate_multi_pal_round(
                &app_handle,
                &request.conversation_id,
                &user_message_id,
                target_pal_ids,
                &characters,
                &request.provider,
                request.parameters.as_ref(),
            )
            .await?;

            for reply in &replies {
                let message = Message {
                    id: reply.message_id.clone(),
                    text: reply.text.clone(),
                    reasoning: None,
                    sender: MessageRole::Assistant,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    tokens: None,
                    embedding: None,
                    images: None,
                    tool_calls: None,
                    source: reply.source.clone(),
                    pal_id: Some(reply.pal_id.clone()),
                    pal_name: Some(reply.pal_name.clone()),
                };

                {
                    let state_mutex = app_handle.state::<Mutex<AppData>>();
                        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
                        insert_message_and_emit(
                            app_handle,
                            &mut state,
                            &request.conversation_id,
                            message,
                            Some(&user_message_id),
                        )?;
                }
            }

            emit_event(
                app_handle,
                ConversationEventPayload::Completed {
                    leaf_message_id: replies
                        .last()
                        .map(|r| r.message_id.clone())
                        .unwrap_or(user_message_id.clone()),
                },
            )?;

            return Ok(replies
                .last()
                .map(|r| r.message_id.clone())
                .unwrap_or(user_message_id));
        }
    }

    // Fallback: single default responder path
    let assistant_message_id = run_conversation_rounds(
        app_handle.clone(),
        request.conversation_id.clone(),
        user_message_id.clone(),
        request.model.clone(),
        request.provider.clone(),
        request.parameters.clone(),
        request.character.clone(),
        stream_id.clone(),
    )
    .await?;

    // ── Director check after single-pal path ───────────────────────
    // Check if there are any previously unlocked pals that the
    // director might invite to join the conversation.
    let unlocked_pal_ids: HashSet<String> = {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let state = state_mutex.lock().map_err(|error| error.to_string())?;
        state
            .unlocked_pals
            .get(&request.conversation_id)
            .cloned()
            .unwrap_or_default()
    };

    if !unlocked_pal_ids.is_empty() {
        let characters = {
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let state = state_mutex.lock().map_err(|error| error.to_string())?;
            state.config_manager.get_characters()
        };

        let director_reply = orchestrator::run_director_check(
            app_handle,
            &request.conversation_id,
            &user_message_id,
            &[],
            &characters,
            &unlocked_pal_ids,
            &request.provider,
            request.parameters.as_ref(),
        )
        .await?;

        if let Some(reply) = director_reply {
            let message = Message {
                id: reply.message_id.clone(),
                text: reply.text.clone(),
                reasoning: None,
                sender: MessageRole::Assistant,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                tokens: None,
                embedding: None,
                images: None,
                tool_calls: None,
                source: reply.source.clone(),
                pal_id: Some(reply.pal_id.clone()),
                pal_name: Some(reply.pal_name.clone()),
            };

            {
                let state_mutex = app_handle.state::<Mutex<AppData>>();
                let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
                insert_message_and_emit(
                    app_handle,
                    &mut state,
                    &request.conversation_id,
                    message,
                    Some(&assistant_message_id),
                )?;
            }

            emit_event(
                app_handle,
                ConversationEventPayload::Completed {
                    leaf_message_id: reply.message_id.clone(),
                },
            )?;

            return Ok(reply.message_id);
        }
    }

    Ok(assistant_message_id)
}

/// Tauri command wrapper for [`conversation_send_message_inner`].
#[tauri::command]
pub async fn conversation_send_message(
    app_handle: AppHandle,
    request: ConversationSendRequest,
) -> Result<String, String> {
    conversation_send_message_inner(&app_handle, request).await
}

#[tauri::command]
pub async fn conversation_regenerate_message(
    app_handle: AppHandle,
    request: ConversationRegenerateRequest,
) -> Result<String, String> {
    let parent_id = {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
        state
            .chat
            .thread_manager
            .get_parent(&request.message_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Cannot regenerate the root message".to_string())?
    };

    let _ = request.insert_guidance;
    let stream_id = request.stream_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    run_conversation_rounds(
        app_handle,
        request.conversation_id,
        parent_id,
        request.model,
        request.provider,
        request.parameters,
        request.character,
        stream_id,
    )
    .await
}

#[tauri::command]
pub async fn conversation_derive_message(
    app_handle: AppHandle,
    request: ConversationDeriveRequest,
) -> Result<String, String> {
    let parent_id = {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
        state
            .chat
            .thread_manager
            .get_parent(&request.replaced_message_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Root message cannot be derived".to_string())?
    };

    conversation_send_message(
        app_handle,
        ConversationSendRequest {
            conversation_id: request.conversation_id,
            parent_message_id: Some(parent_id),
            text: request.text,
            images: None,
            model: request.model,
            provider: request.provider,
            parameters: request.parameters,
            character: request.character,
            target_pal_ids: request.target_pal_ids,
            stream_id: request.stream_id,
        },
    )
    .await
}

#[tauri::command]
pub async fn conversation_edit_and_regenerate(
    app_handle: AppHandle,
    request: ConversationDeriveRequest,
) -> Result<String, String> {
    {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
        state
            .chat
            .update_message(&request.replaced_message_id, &request.text)
            .map_err(|error| error.to_string())?;
        emit_event(
            &app_handle,
            ConversationEventPayload::MessageUpdated {
                message_id: request.replaced_message_id.clone(),
                text: request.text.clone(),
                reasoning: None,
                tool_calls: None,
            },
        )?;
    }

    conversation_regenerate_message(
        app_handle,
        ConversationRegenerateRequest {
            conversation_id: request.conversation_id,
            message_id: request.replaced_message_id,
            insert_guidance: false,
            model: request.model,
            provider: request.provider,
            parameters: request.parameters,
            character: request.character,
            stream_id: request.stream_id,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::cache::DiagramCache;
    use wisp_configs::ConfigManager;
    use wisp_db::create_memory_pool;
    use wisp_db::chat::Chat;
    use wisp_keyring::KeyManager;
    use wisp_mcp::McpConfigManager;
    use wisp_mcp::McpHttpManager;
    use wisp_mcp::McpStdioManager;
    use wisp_mcp::ToolRegistry;

    /// Create a mock Tauri AppHandle with a managed AppData containing a
    /// conversation ready for use. Returns (handle, conversation_id).
    fn setup_app() -> (tauri::AppHandle<tauri::test::MockRuntime>, String) {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        let conversation_id = "test-conv".to_string();
        let mut chat = Chat::new_with_pool(create_memory_pool()).expect("chat");
        chat.create_conversation(&conversation_id, "Test", "desc")
            .expect("conversation created");

        let diagram_cache = DiagramCache::new().expect("diagram cache");
        let key_manager = KeyManager::new("test-wisp".to_string());
        let config_manager =
            ConfigManager::new(&handle).expect("config manager");
        let mcp_config_manager =
            McpConfigManager::new(&handle).expect("mcp config");
        let stdio_manager = Arc::new(McpStdioManager::new());
        let http_manager = Arc::new(McpHttpManager::new());
        let tool_registry = Arc::new(ToolRegistry::new(
            Arc::clone(&stdio_manager),
            Arc::clone(&http_manager),
        ));

        let app_data = AppData {
            chat,
            diagram_cache,
            key_manager,
            config_manager,
            mcp_config_manager,
            mcp_stdio_manager: stdio_manager,
            mcp_http_manager: http_manager,
            tool_registry,
            unlocked_pals: HashMap::new(),
        };

        handle.manage(Mutex::new(app_data));
        handle.manage(crate::abort::AbortRegistry::new());

        (handle, conversation_id)
    }

    fn test_provider() -> Provider {
        use wisp_configs::model::{Model as ProviderModel, ModelInfo, ModelMetadata};

        Provider {
            name: "test-provider".to_string(),
            display_name: "Test Provider".to_string(),
            base_url: "http://localhost:9999".to_string(),
            api_type: Default::default(),
            models: vec![ProviderModel {
                metadata: ModelMetadata {
                    name: "gpt-4".to_string(),
                    display_name: "GPT-4".to_string(),
                    creator: None,
                    version: None,
                    description: None,
                },
                model_info: ModelInfo::TextGeneration {
                    parameters: Default::default(),
                    capabilities: vec![],
                    multimodal: None,
                },
                tokenizer: None,
                max_input_size: 8192,
                api_endpoint: None,
            }],
        }
    }

    #[tokio::test]
    async fn send_message_with_none_target_pal_ids_uses_single_pal_path() {
        let (handle, conversation_id) = setup_app();

        let request = ConversationSendRequest {
            conversation_id,
            parent_message_id: None,
            text: "Hello".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: None,
            stream_id: None,
        };

        let result = conversation_send_message_inner(&handle, request).await;

        // Falls through to single-pal run_conversation_rounds, which will
        // fail at the LLM call (no real API). The error should NOT contain
        // any orchestrator-specific phrasing like "Pal not found".
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            !err.contains("Pal not found"),
            "expected single-pal path error, got orchestrator error: {}",
            err
        );
    }

    #[tokio::test]
    async fn send_message_with_empty_target_pal_ids_uses_single_pal_path() {
        let (handle, conversation_id) = setup_app();

        let request = ConversationSendRequest {
            conversation_id,
            parent_message_id: None,
            text: "Hello".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: Some(vec![]),
            stream_id: None,
        };

        let result = conversation_send_message_inner(&handle, request).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            !err.contains("Pal not found"),
            "expected single-pal path error, got orchestrator error: {}",
            err
        );
    }

    #[tokio::test]
    async fn send_message_with_non_empty_target_pal_ids_triggers_orchestrator() {
        let (handle, conversation_id) = setup_app();

        let request = ConversationSendRequest {
            conversation_id: conversation_id.clone(),
            parent_message_id: None,
            text: "Hello everyone".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: Some(vec!["nonexistent-pal".to_string()]),
            stream_id: None,
        };

        let result = conversation_send_message_inner(&handle, request).await;

        // Orchestrator tries to find "nonexistent-pal" in characters (empty
        // list from the fresh ConfigManager) and fails immediately.
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Pal not found"),
            "expected orchestrator 'Pal not found' error, got: {}",
            err
        );

        // Verify unlocked_pals map was updated despite the error
        let state_mutex = handle.state::<Mutex<AppData>>();
        let state = state_mutex.lock().unwrap();
        let stored = state.unlocked_pals.get(&conversation_id);
        assert!(stored.is_some(), "unlocked_pals should contain an entry for the conversation");
        assert!(stored.unwrap().contains("nonexistent-pal"), "unlocked_pals should contain the target_pal_id");
    }

    #[tokio::test]
    async fn unlocked_pals_stores_target_pal_ids_from_request() {
        let (handle, conversation_id) = setup_app();

        // Send a message with target_pal_ids (will fail at LLM call, but
        // unlocked_pals should be stored before that)
        let request = ConversationSendRequest {
            conversation_id: conversation_id.clone(),
            parent_message_id: None,
            text: "@coder @designer help".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: Some(vec!["coder".to_string(), "designer".to_string()]),
            stream_id: None,
        };

        let _ = conversation_send_message_inner(&handle, request).await;

        // Verify unlocked_pals has the correct entries
        let state_mutex = handle.state::<Mutex<AppData>>();
        let state = state_mutex.lock().unwrap();
        let stored = state.unlocked_pals.get(&conversation_id);
        assert!(stored.is_some(), "should have entry for conversation");
        let stored_set = stored.unwrap();
        assert!(stored_set.contains("coder"), "should contain coder");
        assert!(stored_set.contains("designer"), "should contain designer");
        assert_eq!(stored_set.len(), 2, "should have exactly 2 entries");
    }

    #[tokio::test]
    async fn unlocked_pals_not_stored_when_no_target_pal_ids() {
        let (handle, conversation_id) = setup_app();

        let request = ConversationSendRequest {
            conversation_id: conversation_id.clone(),
            parent_message_id: None,
            text: "Hello".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: None,
            stream_id: None,
        };

        let _ = conversation_send_message_inner(&handle, request).await;

        // Verify unlocked_pals is empty for this conversation
        let state_mutex = handle.state::<Mutex<AppData>>();
        let state = state_mutex.lock().unwrap();
        let stored = state.unlocked_pals.get(&conversation_id);
        assert!(stored.is_none(), "should NOT have an entry when no target_pal_ids");
    }

    #[tokio::test]
    async fn unlocked_pals_accumulates_across_messages() {
        let (handle, conversation_id) = setup_app();

        // First message: mention @coder
        let request1 = ConversationSendRequest {
            conversation_id: conversation_id.clone(),
            parent_message_id: None,
            text: "@coder review this".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: Some(vec!["coder".to_string()]),
            stream_id: None,
        };
        let _ = conversation_send_message_inner(&handle, request1).await;

        // Second message: mention @designer
        let request2 = ConversationSendRequest {
            conversation_id: conversation_id.clone(),
            parent_message_id: None,
            text: "@designer share feedback".to_string(),
            images: None,
            model: "test-model".to_string(),
            provider: test_provider(),
            parameters: None,
            character: None,
            target_pal_ids: Some(vec!["designer".to_string()]),
            stream_id: None,
        };
        let _ = conversation_send_message_inner(&handle, request2).await;

        // Verify both coder and designer are in unlocked_pals
        let state_mutex = handle.state::<Mutex<AppData>>();
        let state = state_mutex.lock().unwrap();
        let stored = state.unlocked_pals.get(&conversation_id);
        assert!(stored.is_some());
        let stored_set = stored.unwrap();
        assert!(stored_set.contains("coder"), "should contain coder from first message");
        assert!(stored_set.contains("designer"), "should contain designer from second message");
        assert_eq!(stored_set.len(), 2, "should have accumulated both pals");
    }
}
