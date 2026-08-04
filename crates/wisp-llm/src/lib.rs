pub mod backend;
pub mod convert;
pub mod error;
pub mod events;

pub use backend::{
    resolve_parameters, ChunkCallback, ReasoningConfig, ReasoningPassback, StreamCallbacks,
    StreamOutcome,
};
pub use error::LlmError;

use std::collections::HashMap;

use rig_core::client::CompletionClient;
use rig_core::completion::CompletionModel;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wisp_configs::provider::{ApiType, Provider};
use wisp_keyring::KeyManager;

use crate::convert::build_completion_request;
use crate::error::map_completion_error;
use crate::events::drain_stream;

/// Resolve the provider's API key from the keyring, then from the native
/// provider environment variable. API keys never cross the Tauri boundary.
fn api_key_for(provider: &Provider) -> Result<String, LlmError> {
    let env_name = match &provider.api_type {
        ApiType::OpenAi | ApiType::OpenAiCompatible => "OPENAI_API_KEY",
        ApiType::DeepSeek => "DEEPSEEK_API_KEY",
        ApiType::Anthropic => "ANTHROPIC_API_KEY",
        ApiType::Doubleword => "DOUBLEWORD_API_KEY",
        ApiType::Azure => "AZURE_API_KEY",
        ApiType::Cohere => "COHERE_API_KEY",
        ApiType::Gemini => "GEMINI_API_KEY",
        ApiType::Groq => "GROQ_API_KEY",
        ApiType::HuggingFace => "HF_TOKEN",
        ApiType::Hyperbolic => "HYPERBOLIC_API_KEY",
        ApiType::MiniMax => "MINIMAX_API_KEY",
        ApiType::Mira => "MIRA_API_KEY",
        ApiType::Mistral => "MISTRAL_API_KEY",
        ApiType::Moonshot => "MOONSHOT_API_KEY",
        ApiType::Ollama | ApiType::Llamafile => return Ok(String::new()),
        ApiType::OpenRouter => "OPENROUTER_API_KEY",
        ApiType::Perplexity => "PERPLEXITY_API_KEY",
        ApiType::Together => "TOGETHER_API_KEY",
        ApiType::XAi => "XAI_API_KEY",
        ApiType::XiaomiMiMo => "XIAOMI_MIMO_API_KEY",
        ApiType::ZAi => "ZAI_API_KEY",
    };

    KeyManager::global()
        .get_api_key(&provider.name)
        .or_else(|_| std::env::var(env_name))
        .map_err(|e| LlmError::Other(format!("API key not found for {env_name}: {e}")))
}

fn custom_base_url(provider: &Provider) -> Option<&str> {
    let value = provider.base_url.trim().trim_end_matches('/');
    (!value.is_empty()).then_some(value)
}

fn client_error(error: impl std::fmt::Display) -> LlmError {
    LlmError::Other(error.to_string())
}

fn ensure_base_url(provider: &Provider) -> Result<&str, LlmError> {
    custom_base_url(provider).ok_or_else(|| {
        LlmError::Other("OpenAI-compatible providers require a Base URL".to_string())
    })
}

async fn stream_model<M>(
    model: M,
    request: rig_core::completion::CompletionRequest,
    cancel: CancellationToken,
    callbacks: StreamCallbacks,
) -> Result<StreamOutcome, LlmError>
where
    M: CompletionModel,
{
    let mut stream = model.stream(request).await.map_err(map_completion_error)?;
    let mut outcome = drain_stream(&mut stream, cancel, &callbacks).await?;
    for content in stream.choice.iter() {
        if let rig_core::message::AssistantContent::ToolCall(tool_call) = content {
            outcome.tool_calls.push(tool_call.clone());
        }
    }
    Ok(outcome)
}

/// Per-provider reasoning passback policy (used by `wisp-conversation` payload
/// building to decide whether/when to echo `reasoning_content` back to the
/// model on assistant tool-call turns).
pub fn reasoning_config_for(provider: &Provider) -> ReasoningConfig {
    match provider.api_type {
        ApiType::OpenAi => ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::Never,
        },
        ApiType::DeepSeek => ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::ToolTurnsOnly,
        },
        ApiType::OpenAiCompatible => ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::Always,
        },
        _ => ReasoningConfig {
            field_name: "reasoning_content",
            policy: ReasoningPassback::Never,
        },
    }
}

/// One-shot streaming chat: builds the client, converts OpenAI-wire messages,
/// streams and maps events. Cancellation is surfaced via
/// [`StreamOutcome::cancelled`] — never as an error.
pub async fn stream(
    provider: &Provider,
    model: String,
    messages: Vec<Value>,
    parameters: Option<HashMap<String, Value>>,
    tools: Vec<rig_core::completion::ToolDefinition>,
    tool_choice: Option<rig_core::message::ToolChoice>,
    cancel: CancellationToken,
    callbacks: StreamCallbacks,
) -> Result<StreamOutcome, LlmError> {
    let request = build_completion_request(
        messages,
        parameters,
        tools,
        tool_choice,
        provider.api_type.clone(),
    )?;
    let api_key = if provider.api_type.requires_api_key() {
        Some(api_key_for(provider)?)
    } else {
        None
    };

    macro_rules! stream_with_client {
        ($client:expr) => {{
            let model_handle = $client.completion_model(model);
            stream_model(model_handle, request, cancel, callbacks).await
        }};
    }

    match provider.api_type {
        ApiType::OpenAi => {
            let client = rig_core::providers::openai::CompletionsClient::builder()
                .api_key(api_key.as_deref().expect("OpenAI requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::DeepSeek => {
            let client = rig_core::providers::deepseek::Client::builder()
                .api_key(api_key.as_deref().expect("DeepSeek requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::OpenAiCompatible => {
            let client = rig_core::providers::openai::CompletionsClient::builder()
                .api_key(api_key.as_deref().expect("OpenAI-compatible providers require an API key"))
                .base_url(ensure_base_url(provider)?)
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Anthropic => {
            let client = rig_core::providers::anthropic::Client::builder()
                .api_key(api_key.as_deref().expect("Anthropic requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Azure => {
            let client = rig_core::providers::azure::Client::builder()
                .api_key(rig_core::providers::azure::AzureOpenAIAuth::ApiKey(
                    api_key.as_deref().expect("Azure requires an API key").to_string(),
                ))
                .azure_endpoint(ensure_base_url(provider)?.to_string())
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Doubleword => {
            let client = rig_core::providers::doubleword::Client::builder()
                .api_key(api_key.as_deref().expect("Doubleword requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Cohere => {
            let client = rig_core::providers::cohere::Client::builder()
                .api_key(api_key.as_deref().expect("Cohere requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Gemini => {
            let client = rig_core::providers::gemini::Client::builder()
                .api_key(api_key.as_deref().expect("Gemini requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Groq => {
            let client = rig_core::providers::groq::Client::builder()
                .api_key(api_key.as_deref().expect("Groq requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::HuggingFace => {
            let client = rig_core::providers::huggingface::Client::builder()
                .api_key(api_key.as_deref().expect("Hugging Face requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Hyperbolic => {
            let client = rig_core::providers::hyperbolic::Client::builder()
                .api_key(api_key.as_deref().expect("Hyperbolic requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Llamafile => {
            let builder = rig_core::providers::llamafile::Client::builder()
                .api_key(rig_core::client::Nothing);
            let builder = if let Some(url) = custom_base_url(provider) {
                builder.base_url(url)
            } else {
                builder
            };
            let client = builder.build().map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::MiniMax => {
            let client = rig_core::providers::minimax::Client::builder()
                .api_key(api_key.as_deref().expect("MiniMax requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Mira => {
            let client = rig_core::providers::mira::Client::builder()
                .api_key(api_key.as_deref().expect("Mira requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Mistral => {
            let client = rig_core::providers::mistral::Client::builder()
                .api_key(api_key.as_deref().expect("Mistral requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Moonshot => {
            let client = rig_core::providers::moonshot::Client::builder()
                .api_key(api_key.as_deref().expect("Moonshot requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Ollama => {
            let builder = rig_core::providers::ollama::Client::builder()
                .api_key(api_key.as_deref().unwrap_or_default());
            let builder = if let Some(url) = custom_base_url(provider) {
                builder.base_url(url)
            } else {
                builder
            };
            let client = builder.build().map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::OpenRouter => {
            let client = rig_core::providers::openrouter::Client::builder()
                .api_key(api_key.as_deref().expect("OpenRouter requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Perplexity => {
            let client = rig_core::providers::perplexity::Client::builder()
                .api_key(api_key.as_deref().expect("Perplexity requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::Together => {
            let client = rig_core::providers::together::Client::builder()
                .api_key(api_key.as_deref().expect("Together requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::XAi => {
            let client = rig_core::providers::xai::Client::builder()
                .api_key(api_key.as_deref().expect("xAI requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::XiaomiMiMo => {
            let client = rig_core::providers::xiaomimimo::Client::builder()
                .api_key(api_key.as_deref().expect("Xiaomi MiMo requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
        ApiType::ZAi => {
            let client = rig_core::providers::zai::Client::builder()
                .api_key(api_key.as_deref().expect("Z.ai requires an API key"))
                .build()
                .map_err(client_error)?;
            stream_with_client!(client)
        }
    }
}

async fn list_models_with_client<C>(client: C) -> Result<rig_core::model::ModelList, LlmError>
where
    C: rig_core::client::ModelListingClient,
{
    client.list_models().await.map_err(|error| LlmError::Other(error.to_string()))
}

/// List models through the provider's native rig adapter when supported.
pub async fn list_models(provider: &Provider) -> Result<rig_core::model::ModelList, LlmError> {
    let api_key = if provider.api_type.requires_api_key() {
        Some(api_key_for(provider)?)
    } else {
        None
    };

    macro_rules! list_with_builder {
        ($builder:expr, $message:literal) => {{
            let client = $builder
                .api_key(api_key.as_deref().expect($message))
                .build()
                .map_err(client_error)?;
            list_models_with_client(client).await
        }};
    }

    match &provider.api_type {
        ApiType::OpenAi => list_with_builder!(
            rig_core::providers::openai::Client::builder(),
            "OpenAI requires an API key"
        ),
        ApiType::DeepSeek => list_with_builder!(
            rig_core::providers::deepseek::Client::builder(),
            "DeepSeek requires an API key"
        ),
        ApiType::Anthropic => list_with_builder!(
            rig_core::providers::anthropic::Client::builder(),
            "Anthropic requires an API key"
        ),
        ApiType::Gemini => list_with_builder!(
            rig_core::providers::gemini::Client::builder(),
            "Gemini requires an API key"
        ),
        ApiType::Mistral => list_with_builder!(
            rig_core::providers::mistral::Client::builder(),
            "Mistral requires an API key"
        ),
        ApiType::OpenRouter => list_with_builder!(
            rig_core::providers::openrouter::Client::builder(),
            "OpenRouter requires an API key"
        ),
        ApiType::XiaomiMiMo => list_with_builder!(
            rig_core::providers::xiaomimimo::Client::builder(),
            "Xiaomi MiMo requires an API key"
        ),
        ApiType::Ollama => {
            let builder = rig_core::providers::ollama::Client::builder()
                .api_key(api_key.as_deref().unwrap_or_default());
            let builder = if let Some(url) = custom_base_url(provider) {
                builder.base_url(url)
            } else {
                builder
            };
            list_models_with_client(builder.build().map_err(client_error)?).await
        }
        ApiType::OpenAiCompatible => {
            let client = rig_core::providers::openai::Client::builder()
                .api_key(api_key.as_deref().expect("OpenAI-compatible providers require an API key"))
                .base_url(ensure_base_url(provider)?)
                .build()
                .map_err(client_error)?;
            list_models_with_client(client).await
        }
        kind => Err(LlmError::Other(format!(
            "Provider type {kind:?} does not support native model listing"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_configs::provider::Provider;

    fn provider_with(api_type: ApiType) -> Provider {
        Provider {
            name: "test".to_string(),
            display_name: "Test".to_string(),
            base_url: "http://localhost".to_string(),
            models: vec![],
            api_type,
        }
    }

    #[test]
    fn openai_never_passes_back_reasoning() {
        let config = reasoning_config_for(&provider_with(ApiType::OpenAi));
        assert_eq!(config.field_name, "reasoning_content");
        assert_eq!(config.policy, ReasoningPassback::Never);
    }

    #[test]
    fn deepseek_passes_back_reasoning_on_tool_turns_only() {
        let config = reasoning_config_for(&provider_with(ApiType::DeepSeek));
        assert_eq!(config.field_name, "reasoning_content");
        assert_eq!(config.policy, ReasoningPassback::ToolTurnsOnly);
    }

    #[test]
    fn openai_compatible_always_passes_back_reasoning() {
        let config = reasoning_config_for(&provider_with(ApiType::OpenAiCompatible));
        assert_eq!(config.field_name, "reasoning_content");
        assert_eq!(config.policy, ReasoningPassback::Always);
    }
}
