use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A tool registered in the registry.
///
/// This is what the upper layer (conversation engine, frontend) sees.
/// The `name` is the registered name that the AI uses in `<|tool_calls|>`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    /// Registered name, e.g. `"mcp_tavily_search"`.
    /// The `mcp_` prefix identifies the tool as MCP-provided.
    pub name: String,
    pub description: Option<String>,
    /// JSON Schema object: `{ type: "object", properties: {...} }`
    #[serde(default = "default_input_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// Arbitrary metadata for extensibility.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Whether the tool is currently enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

const fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// The result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "mimeType")]
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

/// Errors that can occur during tool operations.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// The tool name is not registered.
    NotFound(String),
    /// Execution failed (MCP error, transport error, etc.).
    ExecutionFailed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotFound(name) => write!(f, "tool not found: {name}"),
            ToolError::ExecutionFailed(reason) => write!(f, "tool execution failed: {reason}"),
        }
    }
}

impl std::error::Error for ToolError {}

// ── Helpers ────────────────────────────────────────────────────

impl ToolResult {
    /// Parse a raw MCP `{ content: [...], isError: bool }` response.
    pub fn from_mcp_response(raw: serde_json::Value) -> Self {
        let is_error = raw
            .get("isError")
            .or_else(|| raw.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = raw
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        let type_str = item
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text");
                        match type_str {
                            "text" => ToolContent::Text {
                                text: item
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            },
                            "image" => ToolContent::Image {
                                data: item
                                    .get("data")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                mime_type: item
                                    .get("mimeType")
                                    .or_else(|| item.get("mime_type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("image/png")
                                    .to_string(),
                            },
                            "resource" => ToolContent::Resource {
                                uri: item
                                    .get("uri")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                mime_type: item
                                    .get("mimeType")
                                    .or_else(|| item.get("mime_type"))
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                text: item
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                                blob: item
                                    .get("blob")
                                    .and_then(|v| v.as_str())
                                    .map(String::from),
                            },
                            other => ToolContent::Text {
                                text: format!("[Unsupported content type: {other}]"),
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        ToolResult { content, is_error }
    }

    /// Format the result into an AI-readable structured string for storage / system messages.
    pub fn format_for_ai(&self, tool_name: &str, arguments: &serde_json::Value) -> String {
        let status = if self.is_error { "error" } else { "success" };
        let args_str = serde_json::to_string(arguments).unwrap_or_default();

        let mut lines = Vec::new();
        lines.push(format!("[Tool: {tool_name}]"));
        lines.push(format!("Arguments: {args_str}"));
        lines.push(format!("Status: {status}"));

        let has_content = self.content.iter().any(|c| match c {
            ToolContent::Text { text } => !text.is_empty(),
            ToolContent::Image { .. } => true,
            ToolContent::Resource { text, .. } => text.as_ref().map(|t| !t.is_empty()).unwrap_or(true),
        });

        if has_content {
            lines.push(String::new());
            if self.is_error {
                lines.push("[Error]".to_string());
            } else {
                lines.push("[Result]".to_string());
            }
            for content in &self.content {
                match content {
                    ToolContent::Text { text } if !text.is_empty() => {
                        lines.push(text.clone());
                    }
                    ToolContent::Image { .. } => {
                        lines.push("[Image]".to_string());
                    }
                    ToolContent::Resource { uri, text, .. } => {
                        lines.push(
                            text.clone()
                                .unwrap_or_else(|| format!("[Resource: {uri}]")),
                        );
                    }
                    _ => {}
                }
            }
        }

        lines.join("\n")
    }
}

/// Generate the registered name for an MCP tool.
///
/// Format: `mcp_{server_id}_{tool_name}`, lowercased, non-alphanumeric chars
/// replaced with underscores.
///
/// # Examples
///
/// ```
/// use wisp::tool_registry::registered_name;
///
/// assert_eq!(registered_name("tavily", "search"), "mcp_tavily_search");
/// assert_eq!(registered_name("my-server", "read_file"), "mcp_my_server_read_file");
/// ```
pub fn registered_name(server_id: &str, tool_name: &str) -> String {
    let clean = |s: &str| -> String {
        let mut out = String::new();
        let mut prev_underscore = false;
        for ch in s.chars() {
            let mapped = if ch.is_ascii_alphanumeric() || ch == '_' { ch } else { '_' };
            if mapped == '_' {
                if !prev_underscore {
                    out.push('_');
                    prev_underscore = true;
                }
            } else {
                out.push(mapped.to_ascii_lowercase());
                prev_underscore = false;
            }
        }
        out.trim_matches('_').to_string()
    };

    let left = clean(server_id);
    let right = clean(tool_name);
    match (left.is_empty(), right.is_empty()) {
        (true, true) => "mcp_".to_string(),
        (false, true) => format!("mcp_{left}"),
        (true, false) => format!("mcp_{right}"),
        (false, false) => format!("mcp_{left}_{right}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── registered_name ────────────────────────────────────────

    #[test]
    fn test_registered_name_basic() {
        assert_eq!(registered_name("tavily", "search"), "mcp_tavily_search");
    }

    #[test]
    fn test_registered_name_lowercases() {
        assert_eq!(registered_name("MyServer", "FindTool"), "mcp_myserver_findtool");
    }

    #[test]
    fn test_registered_name_replaces_special_chars() {
        assert_eq!(registered_name("my-server", "read-file"), "mcp_my_server_read_file");
    }

    #[test]
    fn test_registered_name_trims_underscores() {
        assert_eq!(registered_name("_server_", "_tool_"), "mcp_server_tool");
    }

    #[test]
    fn test_registered_name_handles_colon() {
        assert_eq!(registered_name("server:1", "tool:name"), "mcp_server_1_tool_name");
    }

    #[test]
    fn test_registered_name_empty_parts() {
        assert_eq!(registered_name("", ""), "mcp_");
    }

    // ── ToolResult::from_mcp_response ──────────────────────────

    #[test]
    fn test_from_mcp_response_text_content() {
        let raw = serde_json::json!({
            "content": [{ "type": "text", "text": "hello" }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0],
            ToolContent::Text { text: "hello".to_string() }
        );
    }

    #[test]
    fn test_from_mcp_response_image_content() {
        let raw = serde_json::json!({
            "content": [{ "type": "image", "data": "abc123", "mimeType": "image/png" }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert_eq!(
            result.content[0],
            ToolContent::Image {
                data: "abc123".to_string(),
                mime_type: "image/png".to_string()
            }
        );
    }

    #[test]
    fn test_from_mcp_response_resource_content() {
        let raw = serde_json::json!({
            "content": [{
                "type": "resource",
                "uri": "file:///tmp/x.txt",
                "mimeType": "text/plain",
                "text": "file content"
            }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert_eq!(
            result.content[0],
            ToolContent::Resource {
                uri: "file:///tmp/x.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: Some("file content".to_string()),
                blob: None
            }
        );
    }

    #[test]
    fn test_from_mcp_response_is_error_false_by_default() {
        let raw = serde_json::json!({ "content": [] });
        let result = ToolResult::from_mcp_response(raw);
        assert!(!result.is_error);
    }

    #[test]
    fn test_from_mcp_response_detects_error() {
        let raw = serde_json::json!({
            "content": [{ "type": "text", "text": "fail" }],
            "isError": true
        });
        let result = ToolResult::from_mcp_response(raw);
        assert!(result.is_error);
    }

    #[test]
    fn test_from_mcp_response_detects_snake_case_error() {
        let raw = serde_json::json!({
            "content": [{ "type": "text", "text": "fail" }],
            "is_error": true
        });
        let result = ToolResult::from_mcp_response(raw);
        assert!(result.is_error);
    }

    #[test]
    fn test_from_mcp_response_unsupported_type_returns_text_fallback() {
        let raw = serde_json::json!({
            "content": [{ "type": "audio", "data": "..." }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert!(matches!(
            &result.content[0],
            ToolContent::Text { text } if text.contains("audio")
        ));
    }

    #[test]
    fn test_from_mcp_response_empty_content() {
        let raw = serde_json::json!({ "content": [], "isError": false });
        let result = ToolResult::from_mcp_response(raw);
        assert!(result.content.is_empty());
    }

    #[test]
    fn test_from_mcp_response_missing_content_field() {
        let raw = serde_json::json!({ "isError": false });
        let result = ToolResult::from_mcp_response(raw);
        assert!(result.content.is_empty());
    }

    #[test]
    fn test_from_mcp_response_resource_with_snake_case_mime_type() {
        let raw = serde_json::json!({
            "content": [{ "type": "resource", "uri": "file://x", "mime_type": "text/csv", "text": "a,b" }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert_eq!(
            result.content[0],
            ToolContent::Resource {
                uri: "file://x".to_string(),
                mime_type: Some("text/csv".to_string()),
                text: Some("a,b".to_string()),
                blob: None
            }
        );
    }

    // ── ToolResult::format_for_ai ──────────────────────────────

    #[test]
    fn test_format_for_ai_success() {
        let result = ToolResult {
            content: vec![ToolContent::Text {
                text: "Temperature is 25°C.".to_string(),
            }],
            is_error: false,
        };
        let formatted = result.format_for_ai("mcp_tavily_search", &serde_json::json!({"q":"weather"}));

        assert!(formatted.contains("[Tool: mcp_tavily_search]"));
        assert!(formatted.contains(r#"Arguments: {"q":"weather"}"#));
        assert!(formatted.contains("Status: success"));
        assert!(formatted.contains("[Result]"));
        assert!(formatted.contains("Temperature is 25°C."));
    }

    #[test]
    fn test_format_for_ai_error() {
        let result = ToolResult {
            content: vec![ToolContent::Text {
                text: "Connection refused.".to_string(),
            }],
            is_error: true,
        };
        let formatted = result.format_for_ai("mcp_http_get", &serde_json::json!({"url":"http://x"}));

        assert!(formatted.contains("Status: error"));
        assert!(formatted.contains("[Error]"));
        assert!(formatted.contains("Connection refused."));
    }

    #[test]
    fn test_format_for_ai_no_result_no_args() {
        let result = ToolResult {
            content: vec![],
            is_error: false,
        };
        let formatted =
            result.format_for_ai("mcp_void", &serde_json::Value::Null);
        assert!(formatted.contains("[Tool: mcp_void]"));
        assert!(formatted.contains("Arguments: null"));
        assert!(formatted.contains("Status: success"));
        // No [Result] section since there's no content
        assert!(!formatted.contains("[Result]"));
    }

    #[test]
    fn test_format_for_ai_image_content() {
        let result = ToolResult {
            content: vec![ToolContent::Image {
                data: "abc".to_string(),
                mime_type: "image/png".to_string(),
            }],
            is_error: false,
        };
        let formatted = result.format_for_ai("mcp_img", &serde_json::json!({}));
        assert!(formatted.contains("[Result]"));
        assert!(formatted.contains("[Image]"));
    }

    #[test]
    fn test_format_for_ai_resource_content() {
        let result = ToolResult {
            content: vec![ToolContent::Resource {
                uri: "file://x".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: Some("file data".to_string()),
                blob: None,
            }],
            is_error: false,
        };
        let formatted = result.format_for_ai("mcp_read", &serde_json::json!({}));
        assert!(formatted.contains("file data"));
    }

    #[test]
    fn test_format_for_ai_resource_without_text() {
        let result = ToolResult {
            content: vec![ToolContent::Resource {
                uri: "file:///secret.bin".to_string(),
                mime_type: None,
                text: None,
                blob: None,
            }],
            is_error: false,
        };
        let formatted = result.format_for_ai("mcp_read", &serde_json::json!({}));
        assert!(formatted.contains("[Resource: file:///secret.bin]"));
    }
}
