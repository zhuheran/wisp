use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::api::{stream_openai_messages, OpenAiStreamEvents};
use crate::configs::character::Character;
use crate::configs::provider::Provider;
use crate::conversation::payload::{build_openai_messages, format_tool_result};
use crate::conversation::tool_parser::parse_tool_calls;
use crate::conversation::types::ConversationToolCall;
use crate::db::types::{ImageContent, Message, MessageRole};
use crate::tool_registry::{ToolContent, ToolDefinition};
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

async fn execute_tool_call(
    app_handle: &AppHandle,
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

    let tool_result = crate::conversation::types::ConversationToolResult {
        content: result
            .content
            .into_iter()
            .map(|c| match c {
                ToolContent::Text { text } => {
                    crate::conversation::types::ConversationToolContent::Text { text }
                }
                ToolContent::Image { data, mime_type } => {
                    crate::conversation::types::ConversationToolContent::Image {
                        data,
                        mime_type,
                    }
                }
                ToolContent::Resource {
                    uri,
                    mime_type,
                    text,
                    blob,
                } => crate::conversation::types::ConversationToolContent::Resource {
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

async fn resolve_enabled_mcp_tools(
    app_handle: &AppHandle,
) -> Result<Vec<ToolDefinition>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|error| error.to_string())?;
    Ok(state.tool_registry.list_enabled_tools())
}

async fn run_conversation_rounds(
    app_handle: AppHandle,
    conversation_id: String,
    mut current_leaf_id: String,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, serde_json::Value>>,
    character: Option<Character>,
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

        let mut openai_messages = build_openai_messages(&path);

        let enabled_tools = resolve_enabled_mcp_tools(&app_handle).await?;
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
        let calls = parsed
            .calls
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
        },
    )
    .await
}
