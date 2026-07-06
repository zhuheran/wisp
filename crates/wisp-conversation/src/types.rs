pub use wisp_common::MessageSource;

use serde::{Deserialize, Serialize};
use wisp_common::{ToolContent, ToolResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub result: Option<ConversationToolResult>,
    #[serde(default)]
    pub qualified_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationToolResult {
    #[serde(default)]
    pub content: Vec<ConversationToolContent>,
    #[serde(default, alias = "isError", rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ConversationToolContent {
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
        #[serde(default, rename = "mimeType")]
        mime_type: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        blob: Option<String>,
    },
}

impl From<&ConversationToolContent> for ToolContent {
    fn from(content: &ConversationToolContent) -> Self {
        match content {
            ConversationToolContent::Text { text } => ToolContent::Text { text: text.clone() },
            ConversationToolContent::Image { data, mime_type } => {
                ToolContent::Image { data: data.clone(), mime_type: mime_type.clone() }
            },
            ConversationToolContent::Resource { uri, mime_type, text, blob } => {
                ToolContent::Resource {
                    uri: uri.clone(),
                    mime_type: mime_type.clone(),
                    text: text.clone(),
                    blob: blob.clone(),
                }
            },
        }
    }
}

impl From<&ConversationToolResult> for ToolResult {
    fn from(result: &ConversationToolResult) -> Self {
        ToolResult {
            content: result.content.iter().map(ToolContent::from).collect(),
            is_error: result.is_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_db::types::Message;

    #[test]
    fn message_source_defaults_to_user_prompted() {
        let m = Message {
            id: "id".into(),
            text: "hello".into(),
            source: MessageSource::UserPrompted,
            ..Default::default()
        };
        assert_eq!(m.source, MessageSource::UserPrompted);
    }
}
