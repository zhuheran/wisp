use serde::{Deserialize, Serialize};

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
    #[serde(default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_config_default_has_sensible_values() {
        let config = PipelineConfig::default();
        assert_eq!(config.compression_threshold_bytes, 4 * 1024 * 1024);
        assert_eq!(config.max_payload_bytes, 20 * 1024 * 1024);
        assert_eq!(config.jpeg_quality, 80);
        assert_eq!(config.max_width, 2048);
        assert_eq!(config.max_height, 2048);
        assert!(config.enable_compression);
        assert!(!config.mime_whitelist.is_empty());
        assert!(config.temp_url_endpoint.is_none());
    }

    #[test]
    fn conversation_loop_config_default_has_sensible_values() {
        let config = ConversationLoopConfig::default();
        assert_eq!(config.max_tool_rounds, 10);
        assert_eq!(config.max_context_tokens, 128000);
        assert_eq!(config.image_token_cost, 85);
        assert!((config.context_window_sliding_ratio - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.retry_attempts, 2);
        assert_eq!(config.retry_delay_ms, 1000);
        assert!(config.enable_vision_injection);
    }

    #[test]
    fn pipeline_config_partial_deserialize_uses_defaults() {
        let json = r#"{"compression_threshold_bytes":1024}"#;
        let config: PipelineConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.compression_threshold_bytes, 1024);
        assert_eq!(config.jpeg_quality, 80);
    }

    #[test]
    fn conversation_config_partial_deserialize_uses_defaults() {
        let json = r#"{"max_tool_rounds":5}"#;
        let config: ConversationLoopConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_tool_rounds, 5);
        assert_eq!(config.retry_attempts, 2);
    }

    #[test]
    fn pipeline_config_toml_roundtrip() {
        let config = PipelineConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: PipelineConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.compression_threshold_bytes, config.compression_threshold_bytes);
        assert_eq!(deserialized.mime_whitelist, config.mime_whitelist);
    }

    #[test]
    fn conversation_config_toml_roundtrip() {
        let config = ConversationLoopConfig {
            max_tool_rounds: 15,
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ConversationLoopConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.max_tool_rounds, 15);
        assert_eq!(deserialized.retry_attempts, 2);
    }
}
