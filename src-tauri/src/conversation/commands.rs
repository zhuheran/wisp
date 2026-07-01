use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::api::{stream_openai_messages, OpenAiStreamEvents};
use crate::configs::character::Character;
use crate::configs::provider::Provider;
use crate::conversation::payload::{build_openai_messages, tool_result_text};
use crate::conversation::tool_executor::{attach_raw_result, normalize_raw_tool_result, split_qualified_tool_name};
use crate::conversation::tool_parser::parse_tool_calls;
use crate::conversation::types::ConversationToolCall;
use crate::db::types::{ImageContent, Message, MessageRole};
use crate::mcp::types::{NormalizedTool, TransportConfig};
use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
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
    pub enabled_mcp_tools: Option<Vec<String>>,
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
    pub enabled_mcp_tools: Option<Vec<String>>,
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
    pub enabled_mcp_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEventPayload {
    MessageCreated { message: Message, parent_id: Option<String> },
    MessageUpdated { message_id: String, text: String, reasoning: Option<String>, tool_calls: Option<String> },
    Completed { leaf_message_id: String },
    Failed { error: String },
}

fn emit_event(app_handle: &AppHandle, payload: ConversationEventPayload) -> Result<(), String> {
    app_handle
        .emit("conversation_event", payload)
        .map_err(|error| error.to_string())
}

fn insert_message_and_emit(
    app_handle: &AppHandle,
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

fn sanitize_tool_name_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn encode_tool_name_for_model(tool_name: &str, duplicate_index: Option<usize>) -> String {
    let base = format!("mcp__{}", sanitize_tool_name_part(tool_name));
    match duplicate_index {
        Some(index) => format!("{}__{}", base, index),
        None => base,
    }
}

async fn execute_tool_call(
    app_handle: &AppHandle,
    call: ConversationToolCall,
) -> Result<ConversationToolCall, String> {
    let (qualified_name, server, stdio_manager, http_manager) = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state
            .lock()
            .map_err(|error| format!("Failed to acquire app state for tool {}: {}", call.name, error))?;
        let qualified_name = call
            .qualified_name
            .clone()
            .or_else(|| state.global_mcp_tool_state.model_name_map.get(&call.name).cloned())
            .unwrap_or_else(|| call.name.clone());
        let (server_id, _) = split_qualified_tool_name(&qualified_name)
            .map_err(|error| format!("Invalid tool name '{}': {}", qualified_name, error))?;
        let server = state
            .mcp_config_manager
            .get_server(server_id)
            .ok_or_else(|| format!("Server '{}' for tool '{}' was not found", server_id, call.name))?;
        (
            qualified_name,
            server,
            std::sync::Arc::clone(&state.mcp_stdio_manager),
            std::sync::Arc::clone(&state.mcp_http_manager),
        )
    };
    let (server_id, tool_name) = split_qualified_tool_name(&qualified_name)
        .map_err(|error| format!("Invalid tool name '{}': {}", qualified_name, error))?;

    let arguments = match call.arguments.clone() {
        serde_json::Value::Object(map) => Some(serde_json::Value::Object(map)),
        serde_json::Value::Null => None,
        other => Some(other),
    };

    let raw_result = match server.transport {
        TransportConfig::Stdio { .. } => stdio_manager
            .call_tool(server_id, tool_name, arguments)
            .await
            .map_err(|error| format!("Tool '{}' failed on stdio server '{}': {}", tool_name, server_id, error))?,
        TransportConfig::Sse { .. } | TransportConfig::Http { .. } => http_manager
            .call_tool(server_id, tool_name, arguments)
            .await
            .map_err(|error| format!("Tool '{}' failed on http server '{}': {}", tool_name, server_id, error))?,
    };

    let normalized = normalize_raw_tool_result(raw_result.clone())
        .map_err(|error| format!("Tool '{}' returned an invalid payload: {}", call.name, error))?;
    let _ = normalized;
    attach_raw_result(call.clone(), raw_result)
        .map_err(|error| format!("Tool '{}' result could not be attached: {}", call.name, error))
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

fn build_provider_tools(enabled_tools: &[NormalizedTool]) -> Vec<ChatCompletionTool> {
    let mut tools_by_name: HashMap<String, Vec<&NormalizedTool>> = HashMap::new();
    for tool in enabled_tools {
        tools_by_name.entry(tool.name.clone()).or_default().push(tool);
    }

    let mut provider_tools = Vec::new();
    for group in tools_by_name.values() {
        for (index, tool) in group.iter().enumerate() {
            let model_name = encode_tool_name_for_model(
                &tool.name,
                if group.len() > 1 { Some(index + 1) } else { None },
            );
            provider_tools.push(ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: model_name,
                    description: tool.description.clone(),
                    parameters: Some(tool.input_schema.clone()),
                    strict: None,
                },
            });
        }
    }

    provider_tools
}

fn build_enabled_tools_prompt(enabled_tools: &[NormalizedTool]) -> String {
    if enabled_tools.is_empty() {
        return String::new();
    }

    let mut tools_by_server: HashMap<String, Vec<&NormalizedTool>> = HashMap::new();
    for tool in enabled_tools {
        tools_by_server
            .entry(tool.server_id.clone())
            .or_default()
            .push(tool);
    }

    let mut server_ids = tools_by_server.keys().cloned().collect::<Vec<_>>();
    server_ids.sort();

    let tool_sections = server_ids
        .into_iter()
        .map(|server_id| {
            let mut server_tools = tools_by_server.remove(&server_id).unwrap_or_default();
            server_tools.sort_by(|a, b| a.name.cmp(&b.name));

            let tool_list = server_tools.clone()
                .into_iter()
                .map(|tool| {
                    let params = tool
                        .input_schema
                        .get("properties")
                        .and_then(|value| value.as_object())
                        .map(|properties| {
                            let mut names = properties.keys().cloned().collect::<Vec<_>>();
                            names.sort();
                            names
                                .into_iter()
                                .filter_map(|name| {
                                    properties
                                        .get(name.as_str())
                                        .map(|property| format_tool_parameter_line(&name, property))
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .filter(|text| !text.is_empty())
                        .unwrap_or_else(|| "      (no parameters)".to_string());

                    let duplicate_index = if server_tools.iter().filter(|candidate| candidate.name == tool.name).count() > 1 {
                        Some(
                            server_tools
                                .iter()
                                .filter(|candidate| candidate.name == tool.name)
                                .position(|candidate| candidate.qualified_name == tool.qualified_name)
                                .unwrap_or(0)
                                + 1,
                        )
                    } else {
                        None
                    };
                    let safe_name = encode_tool_name_for_model(&tool.name, duplicate_index);
                    format!(
                        "    - **{}**: {}\n      - model_name: {}\n      - internal_name: {}\n{}",
                        tool.name,
                        tool.description
                            .clone()
                            .unwrap_or_else(|| "No description".to_string()),
                        safe_name,
                        tool.qualified_name,
                        params
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            format!("### Server: `{}`\n{}", server_id, tool_list)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "## Available Tools\n\nYou have access to the following tools organized by server. Use them via the native tool calling mechanism when appropriate.\n\n### Tool List by Server\n\n{}\n\n### Tool Name Format\n- Use the exact `model_name` shown below when calling a tool via the API\n- `model_name` contains only letters, digits, and underscores\n- Example: `mcp__tavily_search`\n- Do NOT make up tool names or use server prefixes",
        tool_sections
    )
}

async fn resolve_enabled_mcp_tools(
    app_handle: &AppHandle,
    _enabled_mcp_tools: Option<&[String]>,
) -> Result<Vec<NormalizedTool>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|error| error.to_string())?;

    let enabled = &state.global_mcp_tool_state.enabled_tools;
    if enabled.is_empty() {
        return Ok(Vec::new());
    }

    Ok(state
        .global_mcp_tool_state
        .available_tools
        .iter()
        .filter(|tool| enabled.contains(&tool.qualified_name))
        .cloned()
        .collect())
}

async fn run_conversation_rounds(
    app_handle: AppHandle,
    conversation_id: String,
    mut current_leaf_id: String,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, serde_json::Value>>,
    character: Option<Character>,
    enabled_mcp_tools: Option<Vec<String>>,
) -> Result<String, String> {
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

        let mut openai_messages = build_openai_messages(&path)
            .map_err(|error| format!("Failed to build OpenAI request messages for conversation '{}': {}", conversation_id, error))?;

        let enabled_tools = resolve_enabled_mcp_tools(&app_handle, enabled_mcp_tools.as_deref()).await?;
        let provider_tools = build_provider_tools(&enabled_tools);
        let tools_prompt = build_enabled_tools_prompt(&enabled_tools);

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
            openai_messages.insert(
                0,
                async_openai::types::ChatCompletionRequestMessage::System(
                    async_openai::types::ChatCompletionRequestSystemMessage {
                        content: async_openai::types::ChatCompletionRequestSystemMessageContent::Text(
                            system_prompt_sections.join("\n\n"),
                        ),
                        ..Default::default()
                    },
                ),
            );
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
            };
            let state_mutex = app_handle.state::<Mutex<AppData>>();
            let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
            insert_message_and_emit(
                &app_handle,
                &mut state,
                &conversation_id,
                draft_message,
                Some(&current_leaf_id),
            )?;
        }

        let outcome = stream_openai_messages(
            app_handle.clone(),
            openai_messages,
            Some(provider_tools),
            model.clone(),
            provider.clone(),
            parameters.clone(),
            OpenAiStreamEvents {
                message_id: Some(assistant_message_id.clone()),
                ..OpenAiStreamEvents::CONVERSATION
            },
        )
        .await
        .map_err(|error| format!("Model '{}' failed while streaming conversation '{}': {}", model, conversation_id, error))?;

        let parsed = parse_tool_calls(&outcome.text);
        let calls = if outcome.tool_calls.is_empty() {
            parsed.calls
        } else {
            outcome.tool_calls.clone()
        };
        let calls = calls
            .into_iter()
            .filter(|call| !call.name.trim().is_empty())
            .filter(|call| call.arguments.is_object())
            .collect::<Vec<_>>();
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
            &app_handle,
            ConversationEventPayload::MessageUpdated {
                message_id: assistant_message_id.clone(),
                text: assistant_message.text.clone(),
                reasoning: assistant_message.reasoning.clone(),
                tool_calls: assistant_message.tool_calls.clone(),
            },
        )?;

        current_leaf_id = assistant_message_id.clone();

        if calls.is_empty() {
            if outcome.saw_tool_call_delta {
                emit_event(
                    &app_handle,
                    ConversationEventPayload::Failed {
                        error: "Incomplete tool call from provider".to_string(),
                    },
                )?;
                return Err(format!("Incomplete tool call from provider in conversation '{}'", conversation_id));
            }
            emit_event(
                &app_handle,
                ConversationEventPayload::Completed {
                    leaf_message_id: current_leaf_id.clone(),
                },
            )?;
            return Ok(current_leaf_id);
        }

        if round == 9 {
            emit_event(
                &app_handle,
                ConversationEventPayload::Failed {
                    error: "Max tool rounds reached".to_string(),
                },
            )?;
            return Err(format!("Max tool rounds reached for conversation '{}'", conversation_id));
        }

        let mut completed_calls = Vec::new();
        for call in calls {
            completed_calls.push(execute_tool_call(&app_handle, call).await?);
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
            &app_handle,
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
                text: tool_result_text(call),
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
            };

            {
                let state_mutex = app_handle.state::<Mutex<AppData>>();
                let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
                insert_message_and_emit(
                    &app_handle,
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

#[tauri::command]
pub async fn conversation_send_message(
    app_handle: AppHandle,
    request: ConversationSendRequest,
) -> Result<String, String> {
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
    };

    {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex.lock().map_err(|error| error.to_string())?;
        insert_message_and_emit(
            &app_handle,
            &mut state,
            &request.conversation_id,
            user_message,
            request.parent_message_id.as_deref(),
        )?;
    }

    run_conversation_rounds(
        app_handle,
        request.conversation_id,
        user_message_id,
        request.model,
        request.provider,
        request.parameters,
        request.character,
        request.enabled_mcp_tools,
    )
    .await
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
    run_conversation_rounds(
        app_handle,
        request.conversation_id,
        parent_id,
        request.model,
        request.provider,
        request.parameters,
        request.character,
        request.enabled_mcp_tools,
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
            enabled_mcp_tools: request.enabled_mcp_tools,
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
            enabled_mcp_tools: request.enabled_mcp_tools,
        },
    )
    .await
}
