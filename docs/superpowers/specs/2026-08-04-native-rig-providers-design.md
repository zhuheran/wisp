# Native Rig Provider Routing and Provider Registry

**Date:** 2026-08-04  
**Status:** Approved for implementation  
**Scope:** Replace the three-value Provider API type with an explicit native-rig provider registry for non-OAuth chat providers.

## Goals

1. Route each supported Provider to its actual `rig-core` native provider adapter.
2. Make selecting DeepSeek use `rig_core::providers::deepseek`, including completion and model listing.
3. Expose all `rig-core` 0.41.0 native providers that provide chat completion and do not require OAuth.
4. Keep OpenAI Compatible as a custom OpenAI-chat endpoint fallback.
5. Use provider metadata to drive endpoint, authentication, Base URL visibility, model-listing support, and UI labels.
6. Preserve existing configs where possible and provide a backward-compatible migration for existing `open_ai`, `deep_seek`, and `open_ai_compatible` values.
7. Keep API keys in KeyManager; never serialize them in Provider config or cross the Tauri boundary.

## Non-goals

- No OAuth support for ChatGPT or GitHub Copilot.
- No embedding-only, reranker-only, or other non-chat Provider options such as Voyage AI.
- No agent-loop changes.
- No new external dependencies.
- No automatic migration of model IDs or local Model settings.

## Supported Provider Kinds

The new `ProviderKind` is serialized in snake_case. The supported chat-capable, non-OAuth options are:

| Kind | Rig adapter | Default endpoint | Custom Base URL | Model listing | Extra settings |
|---|---|---|---:|---:|---|
| `open_ai` | `providers::openai::CompletionsClient` | OpenAI default | no | yes | none |
| `deep_seek` | `providers::deepseek::Client` | DeepSeek default | no | yes | none |
| `anthropic` | `providers::anthropic::Client` | Anthropic default | no | yes | none |
| `azure` | `providers::azure::Client` | endpoint supplied by user | yes, required | no | API version uses rig default |
| `doubleword` | `providers::doubleword::Client` | Doubleword default | no | no | none |
| `cohere` | `providers::cohere::Client` | Cohere default | no | no | none |
| `gemini` | `providers::gemini::Client` | Gemini default | no | yes | none |
| `groq` | `providers::groq::Client` | Groq default | no | no | none |
| `hugging_face` | `providers::huggingface::Client` | Hugging Face default | no | no | none |
| `hyperbolic` | `providers::hyperbolic::Client` | Hyperbolic default | no | no | none |
| `llamafile` | `providers::llamafile::Client` | local default | yes | no | none |
| `minimax` | `providers::minimax::Client` | MiniMax OpenAI-compatible default | no | no | `api_mode` |
| `mira` | `providers::mira::Client` | Mira default | no | no | none |
| `mistral` | `providers::mistral::Client` | Mistral default | no | yes | none |
| `moonshot` | `providers::moonshot::Client` | Moonshot OpenAI-compatible default | no | no | `api_mode` |
| `ollama` | `providers::ollama::Client` | local default | yes | yes | none |
| `open_router` | `providers::openrouter::Client` | OpenRouter default | no | yes | none |
| `perplexity` | `providers::perplexity::Client` | Perplexity default | no | no | none |
| `together` | `providers::together::Client` | Together default | no | no | none |
| `x_ai` | `providers::xai::Client` | xAI default | no | no | none |
| `xiaomi_mimo` | `providers::xiaomimimo::Client` | Xiaomi MiMo OpenAI-compatible default | no | yes | `api_mode` |
| `z_ai` | `providers::zai::Client` | Z.ai OpenAI-compatible default | no | no | `api_mode` |
| `open_ai_compatible` | rig OpenAI compatible completion client | user supplied | yes, required | yes | none |

The exact default endpoint is owned by rig's `ProviderBuilder::BASE_URL`; the adapter must not duplicate those URLs in application code. `ProviderKind` metadata is the single source for UI behavior and capability flags.

Azure is included with the current API-key-plus-endpoint contract. The endpoint is stored in the existing non-sensitive Base URL field and rig's default API version is used. Azure model listing remains disabled because rig 0.41.0 declares no native ModelListing capability for Azure.

## Configuration Model

### Provider kind

Replace the old `ApiType` enum with `ProviderKind` while retaining the serialized field name `api_type` for backward-compatible config files during this migration:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    DeepSeek,
    Anthropic,
    Azure,
    Doubleword,
    Cohere,
    Gemini,
    Groq,
    HuggingFace,
    Hyperbolic,
    Llamafile,
    MiniMax,
    Mira,
    Mistral,
    Moonshot,
    Ollama,
    OpenRouter,
    Perplexity,
    Together,
    XAi,
    XiaomiMiMo,
    ZAi,
    #[default]
    OpenAiCompatible,
}
```

Existing `open_ai`, `deep_seek`, and `open_ai_compatible` values deserialize unchanged. Existing Rust references migrate from `ApiType` to `ProviderKind` without changing the JSON/TOML field name.

### Provider settings

Add a non-sensitive settings object with serde defaults:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProviderSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_mode: Option<ApiMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    OpenAi,
    Anthropic,
}
```

`Provider.base_url` remains temporarily readable for config compatibility but new code reads `settings.base_url`; when writing, preserve the old top-level field only if required by the existing config migration path. The frontend type mirrors the serialized shape and does not contain API keys.

The provider registry decides which settings are valid:

- native hosted providers: no Base URL input;
- Ollama/Llamafile: optional Base URL input with local defaults;
- OpenAI Compatible: required Base URL input;
- multi-mode providers: show an API mode select.

### Registry metadata

Expose a pure Rust registry API and a matching frontend metadata table:

```rust
pub struct ProviderDescriptor {
    pub kind: ProviderKind,
    pub label: &'static str,
    pub supports_model_listing: bool,
    pub allows_custom_base_url: bool,
    pub requires_base_url: bool,
    pub requires_api_key: bool,
    pub supports_api_mode: bool,
}
```

The registry must be testable without network access.

## Rig Routing

### Completion

`wisp-llm` must stop returning one concrete OpenAI client from `build_client`. It should dispatch by `ProviderKind` and invoke a generic streaming helper for each concrete rig client/model type.

The generic helper owns the shared behavior:

- convert Wisp OpenAI-wire messages to the rig request;
- apply parameters and tools;
- pass the provider kind into provider-specific request preparation;
- drain streaming events with cancellation;
- surface aggregated tool calls;
- map rig errors to `LlmError`.

The provider dispatch owns only construction and calls the generic helper. DeepSeek must use `rig_core::providers::deepseek::Client::builder()` and therefore inherit its request finalization and response handling.

Base URL behavior:

- native providers use rig's default `ProviderBuilder::BASE_URL` when no custom URL is configured;
- local providers use configured Base URL or their native default;
- OpenAI Compatible requires a non-empty Base URL;
- never call `.base_url("")` for a native provider.

### Model listing

`provider_fetch_models` dispatches to the provider's native client only when its descriptor says `supports_model_listing`. Unsupported listing returns a clear structured error; the frontend disables or hides Fetch models and continues to support manual model entry.

DeepSeek listing must use `rig_core::providers::deepseek::Client`, not `providers::openai::Client`. The returned rig `Model` continues through the existing `to_wisp_model` capability inference.

## Reasoning and Request Semantics

`reasoning_config_for` remains provider-kind aware:

- OpenAI: no reasoning passback;
- DeepSeek: tool-turn-only passback;
- OpenAI Compatible: always pass back reasoning;
- other native providers: use an explicit default policy documented in the registry, initially `Never` unless the adapter's response contract is verified.

DeepSeek-specific `thinking` injection must only happen for `ProviderKind::DeepSeek`. DeepSeek's native adapter is responsible for suppressing forced tool choice while thinking is enabled; application code must not duplicate that behavior.

## Tauri and Frontend

### Tauri commands

- Keep `configs_get/create/update/delete_provider` signatures stable at the command boundary where possible.
- Keep `provider_fetch_models(name)` stable; route internally by ProviderKind.
- Add a read-only `provider_descriptors` command only if the frontend cannot share the registry table safely. Prefer a frontend static descriptor table matching the Rust enum to avoid a new command in this phase.

### Frontend Provider form

- Provider type select lists every supported `ProviderKind` above, grouped into Hosted, Local, and Compatible sections.
- Base URL appears only when descriptor allows it; it is required only for OpenAI Compatible.
- API mode appears only for MiniMax, Moonshot, Xiaomi MiMo, and Z.ai.
- API key is shown for hosted providers and remains stored through the existing keyring calls.
- OAuth providers are not listed.
- Fetch models is disabled with explanatory text when the selected provider has no native model-listing capability.
- Existing Provider IDs remain stable; changing display name or kind never changes `name` or the keyring key.

### Type migration

The frontend replaces `ApiType` with `ProviderKind` and updates all labels, comparisons, tests, and request types. Existing persisted `api_type` values continue to work.

## Validation

Rust:

```sh
cargo test -p wisp-configs
cargo test -p wisp-llm
cargo test -p wisp-configs --lib provider
```

Frontend:

```sh
npm run build
npm test
```

Required focused tests:

1. Every ProviderKind descriptor has a unique serialized value and correct Base URL/model-listing flags.
2. Legacy `api_type` values deserialize into the matching ProviderKind.
3. Native providers never receive `.base_url("")`.
4. DeepSeek completion dispatch uses the DeepSeek adapter path.
5. DeepSeek model listing dispatch uses the DeepSeek adapter path.
6. Unsupported model listing produces a clear error.
7. Frontend displays Base URL only for providers that allow it and requires it for OpenAI Compatible.
8. Existing Provider IDs and keyring names remain stable.
