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
impl PipelineConfig {
    pub fn normalize(self) -> Self {
        let d = Self::default();
        Self {
            compression_threshold_bytes: if self.compression_threshold_bytes >= 1024 {
                self.compression_threshold_bytes
            } else {
                d.compression_threshold_bytes
            },
            max_payload_bytes: if self.max_payload_bytes >= 1024 * 1024 {
                self.max_payload_bytes
            } else {
                d.max_payload_bytes
            },
            jpeg_quality: self.jpeg_quality.clamp(1, 100),
            max_width: if self.max_width >= 64 {
                self.max_width
            } else {
                d.max_width
            },
            max_height: if self.max_height >= 64 {
                self.max_height
            } else {
                d.max_height
            },
            mime_whitelist: if self.mime_whitelist.is_empty() {
                d.mime_whitelist
            } else {
                self.mime_whitelist
            },
            enable_compression: self.enable_compression,
            temp_url_endpoint: self.temp_url_endpoint,
        }
    }
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
    #[serde(default = "default_context_window_sliding_ratio")]
    pub context_window_sliding_ratio: f32,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

fn default_max_tool_rounds() -> u32 {
    10
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

impl ConversationLoopConfig {
    pub fn normalize(self) -> Self {
        let d = Self::default();
        Self {
            max_tool_rounds: if self.max_tool_rounds >= 1 {
                self.max_tool_rounds
            } else {
                d.max_tool_rounds
            },
            context_window_sliding_ratio: if (0.1..=0.95)
                .contains(&self.context_window_sliding_ratio)
            {
                self.context_window_sliding_ratio
            } else {
                d.context_window_sliding_ratio
            },
            retry_attempts: self.retry_attempts.min(10),
            retry_delay_ms: if self.retry_delay_ms >= 100 {
                self.retry_delay_ms
            } else {
                d.retry_delay_ms
            },
        }
    }
}

impl Default for ConversationLoopConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: default_max_tool_rounds(),
            context_window_sliding_ratio: default_context_window_sliding_ratio(),
            retry_attempts: default_retry_attempts(),
            retry_delay_ms: default_retry_delay_ms(),
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
        assert!((config.context_window_sliding_ratio - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.retry_attempts, 2);
        assert_eq!(config.retry_delay_ms, 1000);
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
        assert_eq!(
            deserialized.compression_threshold_bytes,
            config.compression_threshold_bytes
        );
        assert_eq!(deserialized.mime_whitelist, config.mime_whitelist);
    }

    #[test]
    fn conversation_config_toml_roundtrip() {
        let config = ConversationLoopConfig { max_tool_rounds: 15, ..Default::default() };
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: ConversationLoopConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.max_tool_rounds, 15);
        assert_eq!(deserialized.retry_attempts, 2);
    }

    #[test]
    fn pipeline_config_normalize_clamps_bad_values() {
        let bad = PipelineConfig {
            compression_threshold_bytes: 0,
            max_payload_bytes: 100,
            jpeg_quality: 0,
            max_width: 10,
            max_height: 10,
            mime_whitelist: vec![],
            enable_compression: false,
            temp_url_endpoint: None,
        };
        let normalized = bad.normalize();
        let d = PipelineConfig::default();
        assert_eq!(normalized.compression_threshold_bytes, d.compression_threshold_bytes);
        assert_eq!(normalized.max_payload_bytes, d.max_payload_bytes);
        assert_eq!(normalized.jpeg_quality, 1);
        assert_eq!(normalized.max_width, d.max_width);
        assert_eq!(normalized.max_height, d.max_height);
        assert!(!normalized.mime_whitelist.is_empty());
        assert!(!normalized.enable_compression);
    }

    #[test]
    fn pipeline_config_normalize_preserves_good_values() {
        let good = PipelineConfig {
            compression_threshold_bytes: 2048,
            max_payload_bytes: 5 * 1024 * 1024,
            jpeg_quality: 95,
            max_width: 4096,
            max_height: 4096,
            mime_whitelist: vec!["image/png".to_string()],
            enable_compression: false,
            temp_url_endpoint: Some("https://example.com".to_string()),
        };
        let normalized = good.normalize();
        assert_eq!(normalized.compression_threshold_bytes, 2048);
        assert_eq!(normalized.max_payload_bytes, 5 * 1024 * 1024);
        assert_eq!(normalized.jpeg_quality, 95);
        assert_eq!(normalized.max_width, 4096);
        assert_eq!(normalized.mime_whitelist, vec!["image/png"]);
        assert_eq!(normalized.temp_url_endpoint.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn conversation_config_normalize_clamps_bad_values() {
        let bad = ConversationLoopConfig {
            max_tool_rounds: 0,
            context_window_sliding_ratio: 5.0,
            retry_attempts: 100,
            retry_delay_ms: 10,
        };
        let normalized = bad.normalize();
        let d = ConversationLoopConfig::default();
        assert_eq!(normalized.max_tool_rounds, d.max_tool_rounds);
        assert_eq!(
            normalized.context_window_sliding_ratio,
            d.context_window_sliding_ratio
        );
        assert_eq!(normalized.retry_attempts, 10);
        assert_eq!(normalized.retry_delay_ms, d.retry_delay_ms);
    }

    #[test]
    fn conversation_config_normalize_preserves_good_values() {
        let good = ConversationLoopConfig {
            max_tool_rounds: 5,
            context_window_sliding_ratio: 0.5,
            retry_attempts: 3,
            retry_delay_ms: 500,
        };
        let normalized = good.normalize();
        assert_eq!(normalized.max_tool_rounds, 5);
        assert_eq!(normalized.context_window_sliding_ratio, 0.5);
        assert_eq!(normalized.retry_attempts, 3);
        assert_eq!(normalized.retry_delay_ms, 500);
    }
}
