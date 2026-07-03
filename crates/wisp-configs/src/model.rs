use serde::{Deserialize, Serialize};

// ========== COMMON STRUCTURES ==========
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextModelCapability {
    FIM,
    ToolUse,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub display_name: String,
    pub creator: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

// ========== MODEL-SPECIFIC CONFIGS ==========
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextGenerationParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub max_tokens: Option<i32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageGenerationParams {
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg_scale: f32,
    pub sampler: Option<String>,
    pub style_preset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingParams {
    pub embedding_dim: Option<usize>,
    pub normalize: bool,
    pub truncate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RerankerParams {
    pub top_n: Option<usize>,
    pub return_documents: bool,
    pub score_threshold: Option<f32>,
}

// ========== MULTIMODAL SUPPORT ==========
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VisionSupport {
    pub context_window: Option<u32>,
    pub max_resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioSupport {
    pub sample_rate: Option<u32>,
    pub max_duration: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MultimodalConfig {
    pub vision: Option<VisionSupport>,
    pub audio: Option<AudioSupport>,
    pub text: Option<TextSupport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextSupport {
    pub context_window: u32,
    pub languages: Vec<String>,
}

// ========== MODEL TYPE ENUM ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "configs")]
pub enum ModelInfo {
    #[serde(rename = "text_generation")]
    TextGeneration {
        parameters: TextGenerationParams,
        capabilities: Vec<TextModelCapability>,
        multimodal: Option<MultimodalConfig>,
    },

    #[serde(rename = "image_generation")]
    ImageGeneration { parameters: ImageGenerationParams },

    #[serde(rename = "embedding")]
    Embedding { parameters: EmbeddingParams },

    #[serde(rename = "reranker")]
    Reranker { parameters: RerankerParams },

    #[serde(rename = "audio")]
    Audio {
        // Audio-specific config
    },
}

// ========== TOP-LEVEL MODEL STRUCT ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub metadata: ModelMetadata,
    pub model_info: ModelInfo,
    pub tokenizer: Option<String>,
    pub max_input_size: usize,
    pub api_endpoint: Option<String>,
}

// impl Model {
//     /// Create a new Model with required fields
//     pub fn new(
//         name: String,
//         display_name: String,
//         model_info: ModelInfo,
//         max_input_size: usize,
//     ) -> Self {
//         Self {
//             metadata: ModelMetadata {
//                 name,
//                 display_name,
//                 creator: None,
//                 version: None,
//                 description: None,
//             },
//             model_info,
//             tokenizer: None,
//             max_input_size,
//             api_endpoint: None,
//         }
//     }

//     /// Builder method for setting tokenizer
//     pub fn with_tokenizer(mut self, tokenizer: String) -> Self {
//         self.tokenizer = Some(tokenizer);
//         self
//     }

//     /// Builder method for setting API endpoint
//     pub fn with_api_endpoint(mut self, endpoint: String) -> Self {
//         self.api_endpoint = Some(endpoint);
//         self
//     }

//     /// Builder method for setting creator
//     pub fn with_creator(mut self, creator: String) -> Self {
//         self.metadata.creator = Some(creator);
//         self
//     }

//     /// Builder method for setting version
//     pub fn with_version(mut self, version: String) -> Self {
//         self.metadata.version = Some(version);
//         self
//     }

//     /// Builder method for setting description
//     pub fn with_description(mut self, description: String) -> Self {
//         self.metadata.description = Some(description);
//         self
//     }

//     // Getters
//     pub fn name(&self) -> &str {
//         &self.metadata.name
//     }

//     pub fn display_name(&self) -> &str {
//         &self.metadata.display_name
//     }

//     /// Check if model has a specific capability
//     pub fn has_capability(&self, capability: ModelCapability) -> bool {
//         match &self.model_info {
//             ModelInfo::TextGeneration { capabilities, .. } => capabilities.contains(&capability),
//             ModelInfo::ImageGeneration { capabilities, .. } => capabilities.contains(&capability),
//             _ => false,
//         }
//     }

//     /// Validate model parameters
//     pub fn validate(&self) -> Result<(), String> {
//         // Validate max input size
//         if self.max_input_size == 0 {
//             return Err("Max input size cannot be zero".to_string());
//         }

//         // Validate model-specific parameters
//         match &self.model_info {
//             ModelInfo::TextGeneration { parameters, .. } => {
//                 if let Some(temp) = parameters.temperature {
//                     if temp < 0.0 || temp > 2.0 {
//                         return Err("Temperature must be between 0.0 and 2.0".to_string());
//                     }
//                 }
//             }
//             ModelInfo::ImageGeneration { parameters, .. } => {
//                 if parameters.width == 0 || parameters.height == 0 {
//                     return Err("Image dimensions cannot be zero".to_string());
//                 }
//             }
//             _ => {}
//         }

//         Ok(())
//     }
// }
