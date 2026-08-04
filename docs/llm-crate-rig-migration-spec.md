# LLM Crate Migration Spec — wisp-llm → thin rig-core adapter

> Date: 2026-08-03
> Companion to: `docs/llm-crate-migration-report.md`
> Decision: replace the hand-rolled reqwest/SSE implementation inside `crates/wisp-llm` with `rig-core` 0.41.0. Use only the completion layer; do **not** adopt `rig-agent`.
> API shape: **thin functional layer** — drop the `LlmBackend` trait and most intermediate structs; consumers use rig's types directly where they map 1:1.

## 1. Goals / Non-Goals

**Goals**

- Remove the hand-rolled SSE parser and HTTP layer from `wisp-llm` (≈100 lines of byte-level stream handling)
- Shrink wisp-llm to a thin adapter over rig-core: keep only business semantics (reasoning passback policy, parameter merging, streaming callbacks, structured errors, message conversion); delete types that merely mirror rig's
- Update the three consumers (`src-tauri/src/{chore.rs, conversation_commands.rs, orchestrator.rs}`) to the new API — call sites change shape, but logic stays identical
- Gain: built-in cancellation (`StreamingCompletionResponse::cancel()`), `reasoning_details` support, DeepSeek thinking/tool_choice handling

**Non-Goals**

- No changes to the conversation loop (`conversation_commands.rs` logic), payload building (`wisp-conversation::payload`), or the `AbortRegistry` interface
- No `rig-agent` adoption
- No new model metadata features (out of scope; wisp-configs already has `context_window`)

## 2. Dependency Changes

`crates/wisp-llm/Cargo.toml`:

```toml
[dependencies]
rig-core = "0.41"        # new
# removed: reqwest, reqwest-sse
# kept:    wisp-configs, wisp-keyring, tokio-util (CancellationToken),
#          async-trait, thiserror, futures, serde, serde_json
```

`rig-core` default features (`reqwest`, `derive`, `rustls`) are sufficient; no extra features needed.

## 3. wisp-llm Public API — Before / After

### Deleted (rig has native equivalents)

| wisp-llm (removed) | rig replacement |
|---|---|
| `ToolDefinition` | `rig_core::completion::ToolDefinition` (identical shape) |
| `ToolChoice` | `rig_core::message::ToolChoice` (variants map 1:1) |
| `StreamRequest` | `rig_core::completion::CompletionRequest` built via `CompletionRequestBuilder` |
| `LlmBackend` trait + `backend_for()` | free function `build_client(&Provider)` |

### Kept (business semantics, no rig equivalent)

| wisp-llm (kept) | Notes |
|---|---|
| `StreamCallbacks { on_content, on_reasoning }` | Tauri event emission closures, constructed in 3 places — keep the type |
| `StreamOutcome { text, reasoning, tool_call_deltas }` | aggregation + delta-index mapping, done once in the adapter |
| `resolve_parameters(model, runtime)` | model defaults + runtime override merge (unchanged) |
| `ReasoningConfig` / `ReasoningPassback` + `reasoning_config_for(&Provider)` | per-provider policy consumed by `payload.rs`; replaces `backend.reasoning_config()` |
| `LlmError` (simplified) | `Api / Serde / Cancelled / Other`; `Http(reqwest::Error)` variant removed |
| message converter (OpenAI wire → rig `Message`) | `payload.rs` keeps emitting OpenAI wire format; conversion lives here, unit-tested |

### New functional entry point

```rust
// wisp-llm lib.rs — all free functions + two small structs

/// One-shot streaming chat: builds client, converts messages, streams, maps events.
pub async fn stream(
    provider: &Provider,
    model: String,
    messages: Vec<Value>,                                     // OpenAI wire (from payload.rs)
    parameters: Option<HashMap<String, Value>>,               // raw merged params (resolve_parameters output)
    tools: Vec<rig_core::completion::ToolDefinition>,         // rig types passed through
    tool_choice: Option<rig_core::message::ToolChoice>,       // rig types passed through
    cancel: CancellationToken,
    callbacks: StreamCallbacks,
) -> Result<StreamOutcome, LlmError>;

/// Provider → rig client (KeyManager key + base_url + api_key).
pub fn build_client(provider: &Provider) -> Result<CompletionsClient, LlmError>;

/// Per-provider reasoning passback policy (replaces `backend.reasoning_config()`).
pub fn reasoning_config_for(provider: &Provider) -> ReasoningConfig;

pub fn resolve_parameters(...) -> Option<HashMap<String, Value>>;   // unchanged
pub struct StreamCallbacks { on_content: ChunkCallback, on_reasoning: ChunkCallback }
pub struct StreamOutcome { text: String, reasoning: String, tool_call_deltas: Vec<Value> }
pub enum LlmError { Api { status, code, message }, Serde(..), Cancelled, Other(String) }
```

## 4. Backend Construction

All three `ApiType`s (OpenAi / DeepSeek / OpenAiCompatible) use the same unified path — the OpenAI chat-completions client with a custom base URL. `deepseek::Client` is *not* used because `Provider.base_url` is user-configurable (e.g. proxies) and `DeepSeekExtBuilder::BASE_URL` is a const.

```rust
// wisp-llm: build_client
use rig_core::providers::openai::{CompletionsClient, CompletionsClientBuilder};

let api_key = KeyManager::global()
    .get_api_key(&provider.name)
    .or_else(|_| std::env::var("OPENAI_API_KEY"))?;

let client: CompletionsClient = CompletionsClientBuilder::default()
    .api_key(&api_key)
    .base_url(provider.base_url.trim_end_matches('/'))
    .build()
    .map_err(|e| LlmError::Other(e.to_string()))?;
```

Per-request construction is acceptable (client is cheap); a shared-cache optimization is a possible follow-up.

## 4b. Custom Model Names

rig has **no model-name registry or validation** — model names are pass-through runtime strings:

- `CompletionClient::completion_model(model: impl Into<String>)` stores the name as-is (`client/completion.rs`); the built-in constants (`GPT_5_6`, `DEEPSEEK_V4_FLASH`, …) are just `&str` aliases, not a required registry
- `CompletionRequest.model: Option<String>` allows per-request overrides (`request.model.unwrap_or(model)`) without rebuilding the client
- Model names from `wisp-configs` (user-configured IDs like `glm-4.5`, fine-tuned names) pass straight through — zero mapping. The only requirement is that the configured endpoint itself recognizes the model ID

This is why the spec has no "model name mapping" step and why `wisp_llm::stream()` takes `model: String` directly.

## 5. Message Converter (OpenAI wire → rig `message::Message`)

`messages` is the OpenAI wire format produced by `wisp-conversation::payload` (role-tagged `Value`s). A new module `src/convert.rs` maps each role:

| Wire message | rig `message::Message` |
|---|---|
| `{"role": "system", "content": s}` | `Message::system(s)` |
| `{"role": "user", "content": s}` | `Message::user(s)` |
| `{"role": "user", "content": [{text…}, {image_url…}]}` | `Message::User { content: OneOrMany<UserContent> }` — `UserContent::text(t)` / `UserContent::image_url(url, None, detail)` |
| `{"role": "assistant", "content": s, "tool_calls": […], "reasoning_content": r}` | `Message::Assistant { content: OneOrMany<AssistantContent> }` — `AssistantContent::text(s)` + `AssistantContent::tool_call(id, name, args)` per call + `AssistantContent::reasoning(r)` if present |
| `{"role": "tool", "tool_call_id": id, "content": s}` | `Message::tool_result_with_call_id(id, None, s)` |

Edge cases:

- Assistant with empty `content` and no tool calls → skip the text part; if nothing remains, emit `AssistantContent::text("")` (rig's `OneOrMany` requires ≥1 item)
- `reasoning` alias accepted alongside `reasoning_content` (OpenRouter dialect)
- `image_url.url` may be a data URI or https URL — both pass through `DocumentSourceKind::Url`
- Converter must be pure and unit-tested against the payload shapes produced by `build_openai_messages_with_reasoning` (including `native_tools` and reasoning-passback variants)

## 6. Parameter Split

`resolve_parameters` output (e.g. `temperature`, `max_tokens`, `top_p`, `top_k`, `presence_penalty`, `frequency_penalty`, `stop_sequences`, `seed`, `reasoning_effort`, `thinking`) splits when building the rig `CompletionRequest`:

1. **Dedicated `CompletionRequest` fields:**
   - `temperature: Option<f64>` (from JSON number)
   - `max_tokens: Option<u64>` (from JSON integer)
   - `tool_choice` (§7)
2. **Everything else** → `additional_params: Option<Value>` (serde-flattened into the body)

**Important:** never place `temperature`/`max_tokens` in `additional_params` — `#[serde(flatten)]` would emit duplicate keys in the request body.

`thinking` (DeepSeek) stays in `additional_params` as today: `{"thinking": {"type": "enabled"}}`.

`output_schema` is not used (wisp-pro has no structured-output calls today).

## 7. Tool Choice

```rust
// wisp ToolChoice (removed) → rig message::ToolChoice (passed through)
Auto      → ToolChoice::Auto
None      → ToolChoice::None
Required  → ToolChoice::Required
Specific(n) → ToolChoice::Specific { function_names: vec![n] }
```

The adapter exposes a small helper `tool_choice_from_wisp(...)` so call sites don't repeat the mapping, or call sites construct rig's `ToolChoice` directly (preferred — it is the type they already hold).

## 8. Cancellation

```rust
// inside wisp-llm::stream
let mut stream = model.stream(request).await?;
tokio::pin!(stream);

loop {
    tokio::select! {
        _ = cancel.cancelled() => {
            stream.as_mut().cancel();          // normal termination semantics
            return Err(LlmError::Cancelled);
        }
        item = stream.next() => {
            let Some(item) = item else { break };
            // map item (see §9)
        }
    }
}
```

- `cancel()` aborts the inner `Abortable` and replaces the stream with an empty one — surfaces as clean termination, no connection leak
- This is *strictly better* than today: the current implementation only notices cancellation between SSE lines (blocking on `byte_stream.next()`), while `select!` responds while the stream is pending
- `chore.rs` / `orchestrator.rs` pass a fresh `CancellationToken::new()` — unchanged behavior
- **Fix in the same change:** `conversation_commands.rs` currently retries `LlmError::Cancelled` until attempts are exhausted (a cancelled token stays cancelled). Treat `Cancelled` as terminal in the retry loop.

## 9. Streaming Event Mapping

`StreamingCompletionResponse<R>` yields `Result<StreamedAssistantContent<R>, CompletionError>`:

| rig event | action |
|---|---|
| `Text(t)` | `outcome.text.push_str`; `on_content(&t.text)` |
| `ReasoningDelta { reasoning, .. }` | `outcome.reasoning.push_str`; `on_reasoning(&reasoning)` |
| `Reasoning(r)` (complete block) | same as delta using `r.display_text()` |
| `ToolCallDelta { id, internal_call_id, content }` | append OpenAI-shaped delta to `outcome.tool_call_deltas` (§9.1) |
| `ToolCall { tool_call, internal_call_id }` | append as a complete delta (name + full args in one chunk) |
| `Final(_)` | ignore (usage not currently surfaced) |
| `Unknown(_)` | ignore |

### 9.1 tool_call_deltas compatibility

`wisp_conversation::merge_tool_call_deltas(&[Value])` consumes OpenAI deltas keyed by `index` with `id` / `function.name` / `function.arguments`. rig's `ToolCallDelta` carries no index, so the adapter maintains a `HashMap<internal_call_id, u64>` counter:

```json
{ "index": 0, "id": "call_abc", "function": { "name": "get_weather", "arguments": "{\"loc\":" } }
```

- First sighting of an `internal_call_id` → assign next index, emit `name` (from `ToolCallDeltaContent::Name`) or `arguments` (from `Delta`)
- Complete `ToolCall` events → emit one delta with full name + arguments
- `merge_tool_call_deltas` and `conversation_commands.rs` remain untouched

## 10. Error Mapping (`CompletionError` → `LlmError`)

- `CompletionError::HttpError` → `LlmError::Other(e.to_string())` (rig's error type is not `reqwest::Error`; the `LlmError::Http` variant is removed — safe, consumers only use `to_string()`)
- `CompletionError::ProviderResponse(p)` → `LlmError::Api { status, code, message }` using `p.status()` / parsed body (existing `api_from_response` parsing logic reused)
- `CompletionError::ProviderError / ResponseError / JsonError / RequestError / UrlError` → `LlmError::Other(e.to_string())`
- Cancellation → `LlmError::Cancelled` (from §8)
- Existing `error.rs` unit tests updated to the new variant shape; the public `Display` strings stay stable

## 11. Consumer Changes (3 files)

| File | Before | After |
|---|---|---|
| `chore.rs` | `backend_for` + `StreamRequest {…}` | `build_client(&provider)?` + `stream(provider, model, messages, params, vec![], None, CancellationToken::new(), callbacks)` → `outcome.text` |
| `orchestrator.rs` | same + `resolve_parameters` | same shape; `resolve_parameters` unchanged; `stream(...)` positional args |
| `conversation_commands.rs` | `backend.reasoning_config()` | `reasoning_config_for(&provider)` |
| | `StreamRequest { …, tools, tool_choice: ToolChoice::Auto, … }` | build rig `ToolDefinition`s (already available via `wisp_tool_registry`) + `Some(rig_core::message::ToolChoice::Auto)` |
| | retry loop | treat `LlmError::Cancelled` as terminal (no retry) |

No changes to `wisp-conversation` (payload, tool_merger) or `AbortRegistry`.

## 11b. UI & Tauri Command Changes (model management)

### New Tauri command: `provider_fetch_models`

Replaces the frontend-direct `useOpenAI.fetchModels` path (`getCredential` → `getUrl(baseUrl + "/models")`). The API key never leaves the backend.

```rust
// src-tauri/src/provider_commands.rs (new module or commands.rs)
#[tauri::command]
pub async fn provider_fetch_models(
    app_handle: AppHandle,
    name: String,
) -> Result<Vec<provider::Model>, String> {
    let provider = /* config_manager.get_provider(&name)? */;
    let client = wisp_llm::build_client(&provider)?;            // KeyManager key, custom base_url
    let models = client.list_models().await                     // rig ModelListingClient
        .map_err(|e| e.to_string())?;                           // ModelList = Vec<rig Model{id, owned_by, context_length, ..}>
    Ok(models.into_iter().map(to_wisp_model).collect())         // mapping + capability inference
}
```

### wisp-configs: `ModelMetadata.owned_by`

Add `owned_by: Option<String>` to `ModelMetadata` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — backward compatible with existing config files. Populated from rig's `Model.owned_by`; surfaced as a new table column.

### Capability / type inference (backend, pure function, unit-tested)

`to_wisp_model(rig Model)` maps `id` → `metadata.name`/`display_name` and infers:

| Model id pattern | ModelInfo type | capabilities (text_generation only) |
|---|---|---|
| contains `embed` | `embedding` | — |
| contains `rerank` | `reranker` | — |
| contains `dall-e`, `gpt-image`, `image` | `image_generation` | — |
| contains `tts`, `whisper`, `audio` | `audio` | — |
| default | `text_generation` | `ToolUse` = true (modern chat models; user can edit), plus: |
| id contains `reasoning`/`reasoner`/`thinking`, starts with `o1`/`o3`/`o4`/`r1`, contains `deepseek-reasoner` | | `Reasoning` |
| id contains `coder`/`code` (e.g. `deepseek-coder`, `qwen2.5-coder`) | | `FIM` |

- `context_window`: use rig `Model.context_length` when present, else omit (wisp-configs default 128k applies on load)
- `description`: pass through rig `Model.description` when present
- New models get default `TextGenerationParams` (empty) — same as today

### Frontend changes

| File | Change |
|---|---|
| `src/libs/commands.ts` | add `providerFetchModels(name)` binding; remove `getUrl` wrapper (its only consumer was `useOpenAI`) |
| `src/composables/useOpenAI.ts` | **delete** (only consumer of `getUrl`) |
| `src/components/ModelTable.vue` | `handleFetch` calls `providerFetchModels(provider.name)` (no key handling); add columns: **Capabilities** (n-tag chips: Reasoning/ToolUse/FIM), **Owned By** (`metadata.owned_by`) |
| `src/views/ProvidersView.vue` | no change (layout shell) |
| `src/components/ProviderDetailForm.vue` | no change (API Type still drives `reasoning_config_for`) |
| `src/components/ModelForm.vue` | no change (schema-driven; inferred values are pre-filled and editable) |

### Tests

- Unit tests for the inference function (each row of the table above)
- `cargo test` for wisp-configs (new field serde round-trip)
- Manual: fetch models on an OpenAI-compatible endpoint + DeepSeek, verify capabilities chips and that fetch works without a stored key returning a clear error

## 12. Testing Strategy

1. **Unit tests (wisp-llm):**
   - `convert.rs`: every payload shape from `build_openai_messages_with_reasoning` (system/user/assistant+tool_calls+reasoning/tool/multimodal user) round-trips to the expected rig `Message`
   - Parameter split: dedicated fields vs `additional_params`, duplicate-key prevention
   - Streaming mapping: feed a synthetic `Vec<StreamedAssistantContent>` and assert `outcome` + callback order
   - tool_call_deltas index assignment across interleaved deltas
   - Error mapping incl. OpenAI-style error envelope
2. **Existing suites must stay green:** `wisp-conversation` (payload/tool_merger), `src-tauri` compile
3. **Manual smoke test:** one real streaming chat per provider type (OpenAI-compatible endpoint, DeepSeek with thinking, reasoning passback on tool turns), plus cancel mid-stream

## 13. Implementation Steps

1. Add `rig-core = "0.41"` to `wisp-llm/Cargo.toml`
2. New `src/convert.rs` (message converter + parameter split) with unit tests
3. Rewrite wisp-llm as the functional layer: `build_client`, `stream` (client build → request build → select!/cancel loop → event mapping → delta index assignment), `reasoning_config_for`; delete `LlmBackend`, `StreamRequest`, `ToolDefinition`, `ToolChoice`, `backend_for`
4. Rework `error.rs` variants; update tests
5. Remove `reqwest`/`reqwest-sse` deps; delete hand-rolled SSE code
6. Update consumers: `chore.rs`, `orchestrator.rs`, `conversation_commands.rs` (incl. Cancelled-terminal fix)
7. Model management backend: `ModelMetadata.owned_by` + `provider_fetch_models` command + inference fn (+ unit tests)
8. Frontend: `commands.ts` binding, delete `useOpenAI.ts`/`getUrl`, `ModelTable.vue` fetch + Capabilities/Owned By columns
9. `cargo test -p wisp-llm -p wisp-conversation -p wisp-configs`, `cargo check` for the full workspace (incl. `src-tauri`); `pnpm build`/type-check for the frontend
10. Manual smoke tests (§12.3)

## 14. Risks & Rollback

| Risk | Mitigation |
|---|---|
| rig-core API differences vs. local source copy (0.41.0 matches crates.io exactly, published 2026-07-28) | pin `rig-core = "=0.41.0"` initially |
| Duplicate-key risk from `additional_params` flatten | parameter split rule in §6 + unit test |
| `reasoning_details` now flows through but is not consumed by the UI | no change needed: adapter already concatenates text from details into `outcome.reasoning` (same as today) |
| Consumer rewrite introduces behavior drift | call-site changes are mechanical (type/shape only); logic assertions covered by §12.2 suites + smoke tests |
| Behavior drift in `merge_tool_call_deltas` consumers | delta index mapping unit-tested; `conversation_commands.rs` merge call untouched |
| Rollback | wisp-llm is a thin layer with a small API surface; revert = restore previous implementation + call sites (git) |
