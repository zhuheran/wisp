use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    NotFound(String),
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

impl ToolResult {
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
                        let type_str = item.get("type").and_then(|v| v.as_str()).unwrap_or("text");
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
                                text: item.get("text").and_then(|v| v.as_str()).map(String::from),
                                blob: item.get("blob").and_then(|v| v.as_str()).map(String::from),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_mcp_response_text_content() {
        let raw = serde_json::json!({
            "content": [{ "type": "text", "text": "hello" }],
            "isError": false
        });
        let result = ToolResult::from_mcp_response(raw);
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0], ToolContent::Text { text: "hello".to_string() });
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
            ToolContent::Image { data: "abc123".to_string(), mime_type: "image/png".to_string() }
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
}
