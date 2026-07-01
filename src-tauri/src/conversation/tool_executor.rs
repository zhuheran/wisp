use std::error::Error;
use std::fmt;

use serde_json::Value;

use super::types::{ConversationToolCall, ConversationToolContent, ConversationToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionError {
    InvalidQualifiedName(String),
    InvalidResult(String),
}

impl fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolExecutionError::InvalidQualifiedName(name) => {
                write!(f, "invalid qualified tool name: {name}")
            }
            ToolExecutionError::InvalidResult(reason) => write!(f, "invalid tool result: {reason}"),
        }
    }
}

impl Error for ToolExecutionError {}

pub fn split_qualified_tool_name(name: &str) -> Result<(&str, &str), ToolExecutionError> {
    let Some((server_id, tool_name)) = name.split_once(':') else {
        return Err(ToolExecutionError::InvalidQualifiedName(name.to_string()));
    };

    if server_id.is_empty() || tool_name.is_empty() {
        return Err(ToolExecutionError::InvalidQualifiedName(name.to_string()));
    }

    Ok((server_id, tool_name))
}

pub fn attach_raw_result(
    call: ConversationToolCall,
    raw_result: Value,
) -> Result<ConversationToolCall, ToolExecutionError> {
    let result = normalize_raw_tool_result(raw_result)?;
    Ok(ConversationToolCall {
        result: Some(result),
        ..call
    })
}

pub fn normalize_raw_tool_result(raw_result: Value) -> Result<ConversationToolResult, ToolExecutionError> {
    let is_error = raw_result
        .get("isError")
        .or_else(|| raw_result.get("is_error"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let Some(content) = raw_result.get("content") else {
        return Ok(ConversationToolResult {
            content: vec![ConversationToolContent::Text {
                text: serde_json::to_string(&raw_result).map_err(|error| {
                    ToolExecutionError::InvalidResult(error.to_string())
                })?,
            }],
            is_error,
        });
    };

    let Some(items) = content.as_array() else {
        return Err(ToolExecutionError::InvalidResult(
            "content must be an array".to_string(),
        ));
    };

    let mut normalized = Vec::new();
    for item in items {
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("text");
        match item_type {
            "text" => normalized.push(ConversationToolContent::Text {
                text: item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }),
            "image" => normalized.push(ConversationToolContent::Image {
                data: item
                    .get("data")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                mime_type: item
                    .get("mimeType")
                    .or_else(|| item.get("mime_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("image/png")
                    .to_string(),
            }),
            "resource" => normalized.push(ConversationToolContent::Resource {
                uri: item
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                mime_type: item
                    .get("mimeType")
                    .or_else(|| item.get("mime_type"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                text: item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                blob: item
                    .get("blob")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            other => {
                normalized.push(ConversationToolContent::Text {
                    text: format!("[Unsupported tool content type: {other}]"),
                });
            }
        }
    }

    Ok(ConversationToolResult {
        content: normalized,
        is_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_qualified_tool_name_splits_at_first_colon_only() {
        let (server_id, tool_name) =
            split_qualified_tool_name("server-1:tool:name:with:colon").expect("valid name");

        assert_eq!(server_id, "server-1");
        assert_eq!(tool_name, "tool:name:with:colon");
    }

    #[test]
    fn split_qualified_tool_name_rejects_invalid_names() {
        assert!(matches!(
            split_qualified_tool_name("missing_colon"),
            Err(ToolExecutionError::InvalidQualifiedName(_))
        ));
        assert!(matches!(
            split_qualified_tool_name(":missing_server"),
            Err(ToolExecutionError::InvalidQualifiedName(_))
        ));
        assert!(matches!(
            split_qualified_tool_name("missing_tool:"),
            Err(ToolExecutionError::InvalidQualifiedName(_))
        ));
    }

    #[test]
    fn normalize_raw_tool_result_preserves_text_image_resource_and_error_state() {
        let result = normalize_raw_tool_result(serde_json::json!({
            "isError": true,
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "image", "data": "abc", "mimeType": "image/jpeg" },
                { "type": "resource", "uri": "file://x", "mimeType": "text/plain", "text": "resource text" }
            ]
        }))
        .expect("result normalized");

        assert!(result.is_error);
        assert_eq!(result.content.len(), 3);
        assert!(matches!(result.content[0], ConversationToolContent::Text { .. }));
        assert!(matches!(result.content[1], ConversationToolContent::Image { .. }));
        assert!(matches!(result.content[2], ConversationToolContent::Resource { .. }));
    }

    #[test]
    fn attach_raw_result_returns_completed_call() {
        let call = ConversationToolCall {
            id: "call_1".to_string(),
            name: "server:tool".to_string(),
            arguments: serde_json::json!({"q":"x"}),
            result: None,
            qualified_name: None,
        };

        let completed = attach_raw_result(
            call,
            serde_json::json!({"content":[{"type":"text","text":"done"}]}),
        )
        .expect("completed");

        assert_eq!(completed.id, "call_1");
        assert!(completed.result.is_some());
    }
}
