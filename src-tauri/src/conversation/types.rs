use serde::{Deserialize, Serialize};

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
