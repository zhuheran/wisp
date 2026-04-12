use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ========== Transport Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        cwd: Option<String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        session_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub transport: TransportConfig,
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    #[serde(default = "default_reconnect_interval_ms")]
    pub reconnect_interval_ms: u64,
    #[serde(default = "default_max_reconnect_attempts")]
    pub max_reconnect_attempts: u32,
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    pub protocol_version: Option<String>,
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_reconnect_interval_ms() -> u64 {
    5000
}

fn default_max_reconnect_attempts() -> u32 {
    5
}

fn default_heartbeat_interval_ms() -> u64 {
    30000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub server_id: String,
    pub connected: bool,
    pub last_ping_at: Option<u64>,
    pub reconnect_attempts: u32,
    pub error: Option<String>,
}

// ========== Registry Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTool {
    pub name: String,
    pub server_id: String,
    pub qualified_name: String,
    pub description: Option<String>,
    pub input_schema: InputSchema,
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, NormalizedProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedProperty {
    #[serde(rename = "type")]
    pub property_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<NormalizedProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, NormalizedProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<NormalizedProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<NormalizedProperty>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub server_id: String,
    pub tool_name: String,
    pub content: Vec<ToolCallContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        blob: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRegistryOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_separator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval_ms: Option<u64>,
}

// ========== Pipeline Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadItem {
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub kind: String,
    pub mime_type: Option<String>,
    pub size_bytes: usize,
    pub needs_compression: bool,
    pub needs_prefix_fix: bool,
    pub is_base64: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformResult {
    #[serde(rename = "type")]
    pub result_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    pub original_size_bytes: usize,
    pub transformed_size_bytes: usize,
    pub was_compressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionRouteResult {
    pub content: VisionContent,
    pub fallback_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisionContent {
    ImageUrl { image_url: ImageUrl },
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    #[serde(default = "default_compression_threshold")]
    pub compression_threshold_bytes: usize,
    #[serde(default = "default_max_payload")]
    pub max_payload_bytes: usize,
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    #[serde(default = "default_max_width")]
    pub max_width: u32,
    #[serde(default = "default_max_height")]
    pub max_height: u32,
    #[serde(default = "default_mime_whitelist")]
    pub mime_whitelist: Vec<String>,
    #[serde(default = "default_enable_compression")]
    pub enable_compression: bool,
    pub temp_url_endpoint: Option<String>,
}

fn default_compression_threshold() -> usize {
    4 * 1024 * 1024
}

fn default_max_payload() -> usize {
    20 * 1024 * 1024
}

fn default_jpeg_quality() -> u8 {
    80
}

fn default_max_width() -> u32 {
    2048
}

fn default_max_height() -> u32 {
    2048
}

fn default_mime_whitelist() -> Vec<String> {
    vec![
        "image/png".to_string(),
        "image/jpeg".to_string(),
        "image/gif".to_string(),
        "image/webp".to_string(),
        "image/svg+xml".to_string(),
        "image/bmp".to_string(),
        "image/tiff".to_string(),
    ]
}

fn default_enable_compression() -> bool {
    true
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            compression_threshold_bytes: default_compression_threshold(),
            max_payload_bytes: default_max_payload(),
            jpeg_quality: default_jpeg_quality(),
            max_width: default_max_width(),
            max_height: default_max_height(),
            mime_whitelist: default_mime_whitelist(),
            enable_compression: default_enable_compression(),
            temp_url_endpoint: None,
        }
    }
}

// ========== Engine Types ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: Vec<VisionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationLoopConfig {
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: u32,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u32,
    #[serde(default = "default_image_token_cost")]
    pub image_token_cost: u32,
    #[serde(default = "default_context_window_sliding_ratio")]
    pub context_window_sliding_ratio: f32,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_enable_vision_injection")]
    pub enable_vision_injection: bool,
}

fn default_max_tool_rounds() -> u32 {
    10
}

fn default_max_context_tokens() -> u32 {
    128000
}

fn default_image_token_cost() -> u32 {
    85
}

fn default_context_window_sliding_ratio() -> f32 {
    0.7
}

fn default_retry_attempts() -> u32 {
    2
}

fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_enable_vision_injection() -> bool {
    true
}

impl Default for ConversationLoopConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: default_max_tool_rounds(),
            max_context_tokens: default_max_context_tokens(),
            image_token_cost: default_image_token_cost(),
            context_window_sliding_ratio: default_context_window_sliding_ratio(),
            retry_attempts: default_retry_attempts(),
            retry_delay_ms: default_retry_delay_ms(),
            enable_vision_injection: default_enable_vision_injection(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub messages: Vec<ConversationMessage>,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: HashMap<String, serde_json::Value>,
}

// ========== MCP Config Storage ==========

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    pub servers: Vec<ServerConfig>,
    pub pipeline_config: Option<PipelineConfig>,
    pub conversation_config: Option<ConversationLoopConfig>,
}
