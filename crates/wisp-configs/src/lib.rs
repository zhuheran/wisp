pub mod character;
pub mod model;
pub mod provider;
pub mod manager;
pub mod settings;

pub use manager::{ConfigManager, ConfigError, ChoreLlmRef};
pub use settings::{PipelineConfig, ConversationLoopConfig};
