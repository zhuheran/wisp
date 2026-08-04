# LLM Crate Replacement — Investigation Report

> Date: 2026-08-03
> Scope: Evaluate replacing the hand-rolled `crates/wisp-llm` implementation with a third-party LLM crate, and select the best option.

## 1. Background

`wisp-llm` is currently a hand-rolled implementation:

- `backends/compat.rs`: ~100 lines of byte-level SSE parsing (raw `reqwest` bytes stream, manual `\n` line splitting, `[DONE]` detection, error fallbacks)
- Three backends (OpenAI / DeepSeek / OpenAI Compat) sharing one OpenAI-compatible protocol
- Custom logic: dual-format reasoning parsing (`reasoning_content` + `reasoning_details`), `CancellationToken` cancellation, structured `LlmError::Api` errors, pass-through of arbitrary parameters

Reasons for replacement:

1. Hand-rolled SSE parsing is the most bug-prone part (edge cases: `finish_reason` co-delivered with content, tool calls co-delivered with `finish_reason`, trailing usage chunks, DeepSeek cache-usage payloads)
2. Provider-specific quirks are unhandled (e.g. DeepSeek rejects forced `tool_choice` while thinking is enabled)
3. Future agent work would benefit from a maintained foundation (memory, vector stores, embeddings)

## 2. Requirements (from the wisp-pro codebase)

| Requirement | Current implementation | Where |
|---|---|---|
| OpenAI / DeepSeek / arbitrary OpenAI-compatible endpoints | 3 backends, `provider.base_url` | `wisp-configs::provider::Provider` |
| Streaming `reasoning_content` callbacks | `StreamCallbacks.on_reasoning` | `backend.rs`, `conversation_commands.rs` |
| `reasoning_details` (newer DeepSeek format) | parsed explicitly | `compat.rs` |
| Tool calls: definitions, `tool_choice`, delta accumulation | `ToolDefinition`, `ToolChoice`, `merge_tool_call_deltas` | `backend.rs`, `tool_merger.rs` |
| Cancellation (`CancellationToken`) | checked in SSE loop, returns `LlmError::Cancelled` | `abort.rs`, `compat.rs` |
| API keys from `wisp_keyring::KeyManager` | `KeyManager::global().get_api_key` | `compat.rs` |
| Arbitrary parameter pass-through | `HashMap<String, Value>` merged into body | `resolve_parameters`, `apply_parameters` |
| Structured errors (status/code/message) | `LlmError::Api` | `error.rs` |

## 3. Candidates Evaluated

All four candidates were downloaded and their source code was inspected directly (not just docs):

### 3.1 genai (0.6.5 stable, 2026-06-06)

- Provider-agnostic client with adapter layer (OpenAI, Gemini, Anthropic, Ollama, OpenRouter, …)
- Custom endpoints: `ProviderConfig { endpoint, auth }` / `ServiceTarget`
- Streaming `ReasoningChunk` event; **no `reasoning_details` support** (0 matches in source)
- No built-in cancellation, but `ChatStream` is a plain `Stream` — caller owns it, dropping it closes the connection; `tokio::select!` wrapping works
- No model metadata (only `all_model_names`; static lists in 0.6.5)
- Maintained by JeremyChone; mature

### 3.2 aisdk (0.5.2, lazy-hq)

- Vercel AI SDK port; agents + built-in tool execution loop
- **Architectural conflicts:**
  - The agent loop is inside the library (`stream_text` runs the full loop in a `tokio::spawn` task, executing tools via compile-time `#[tool]` macros). wisp-pro's loop is self-managed (MCP runtime-registered tools, `<|tool_calls|>` text-protocol fallback, `trim_context`, reasoning passback)
  - Runtime MCP tools cannot be registered into a compile-time macro tool set
  - **No cancellation mechanism at all** (0 matches for abort/cancel), and dropping the channel receiver does not stop the spawned background task
- `reasoning_content` streamed (ReasoningStart/Delta/End)
- Young: 2 maintainers, ~10k downloads

### 3.3 llm-sdk (0.3.0, hoangvvo)

- Cross-language (JS/Rust/Go) unified wire format; model metadata JSON with **pricing + capabilities** (unique), cost calculation
- Custom endpoints: `OpenAIChatModel::new(model_id, api_key, base_url)`
- **Decisive flaw:** the OpenAI chat-completions path does **not parse `reasoning_content` at all** (0 matches; `ChatCompletionStreamResponseDelta` has no such field) — DeepSeek reasoning would be silently dropped, both streaming and non-streaming
- No cancellation, but stream is a plain `BoxedStream` (droppable)
- v0, single author

### 3.4 rig / rig-core (0.41.0, 2026-07-28, ~2M downloads, 61 versions)

- `rig-core` = low-level completion/embedding layer; `rig-agent` = optional agent layer (NOT required)
- **Built-in cancellation:** `StreamingCompletionResponse::cancel()` (futures `AbortHandle` + `Abortable`; cancellation surfaces as normal stream termination) + `pause()`/`resume()`
- **Reasoning fully covered:** `reasoning_content` + `reasoning` (Groq dialect) + **`reasoning_details`** (only candidate that supports the newer DeepSeek format)
- **Dedicated DeepSeek provider** (`providers/deepseek.rs`): handles `thinking` param, suppresses forced `tool_choice` when thinking is enabled (DeepSeek rejects it), cache hit/miss usage, `GET /models` ModelLister
- Custom endpoints: `ClientBuilder::base_url()`; API keys: `builder.api_key()` / `Client::from_val()`
- Dynamic tools: `CompletionRequest.tools: Vec<ToolDefinition>` (name/description/JSON-schema parameters) — identical shape to wisp-llm's `ToolDefinition`
- Arbitrary params: `additional_params: Option<Value>` (serde-flattened into the request body)
- Model metadata: `Model { context_length, owned_by }` + ModelLister
- Structured errors: `CompletionError` with `provider_response_status/json/body` helpers
- No built-in agent loop in `rig-core` (matches wisp-pro's self-managed loop)
- reqwest 0.13 (matches the project), edition 2024

## 4. Comparison Matrix

| Requirement | genai | aisdk | llm-sdk | **rig** |
|---|---|---|---|---|
| Custom endpoint / KeyManager | ✅ | ✅ | ✅ | ✅ |
| `reasoning_content` streaming | ✅ | ✅ | ❌ (OpenAI path) | ✅ |
| `reasoning_details` | ❌ | ❌ | ❌ | **✅** |
| Built-in cancellation | ❌ (select!) | ❌ (unfixable task) | ❌ (drop) | **✅ cancel()** |
| Dynamic tools (MCP runtime) | ✅ | ❌ compile-time macros | ✅ | ✅ |
| Agent loop separation | ✅ none | ❌ built-in | ✅ separate | ✅ separate (`rig-agent` optional) |
| DeepSeek-specific handling | ❌ generic | ❌ generic | ❌ generic | **✅ dedicated** |
| Model metadata | only names | compile-time markers | pricing+capabilities | context_length + ModelLister |
| Maturity | 0.6.5 stable | 0.5.2, 2 maintainers | 0.3.0 v0, 1 author | 0.41.0, active, ~2M downloads |
| Dependency fit | ok | ok | reqwest 0.13 | reqwest 0.13, edition 2024 |

## 5. Decision

**Adopt `rig-core` 0.41.0 and reshape `wisp-llm` into a thin functional adapter over it.**

- Use **only** `rig-core` (completion layer). **Do not** use `rig-agent` for now — wisp-pro's conversation loop is mature (MCP dynamic tools, text-protocol fallback, trim_context, reasoning passback) and should stay self-managed.
- Most wisp-llm data structures are dropped (`LlmBackend` trait, `StreamRequest`, `ToolDefinition`, `ToolChoice` — the latter two map 1:1 onto rig types). Kept: `StreamCallbacks`, `StreamOutcome`, `resolve_parameters`, `ReasoningConfig`/`ReasoningPassback`, a simplified `LlmError`, and the OpenAI-wire → rig message converter. The three consumers (`chore.rs`, `orchestrator.rs`, `conversation_commands.rs`) are updated to the new call shape; `wisp-conversation` and `AbortRegistry` are untouched. See the spec §3/§11 for the full API mapping.
- `rig-agent` remains an option for future agent work (AgentHook, memory, vector stores, MCP bridge), and adopting it later does not require changing the completion layer.

## 6. Key Facts Verified in Source (rig-core 0.41.0)

- `client::ClientBuilder::base_url()` / `api_key()` — `providers/openai/client.rs`, `client/mod.rs`
- `CompletionClient::completion_model(model)` — `client/completion.rs`
- `CompletionRequest { chat_history, tools: Vec<ToolDefinition>, temperature, max_tokens, tool_choice, additional_params, output_schema, .. }` — `completion/request.rs`
- `additional_params` is `#[serde(flatten)]`-merged into the request body — `providers/openai/completion/mod.rs` (duplicate-key risk if the same param is set both as a dedicated field and in `additional_params`)
- OpenAI wire `Message::Assistant` serializes reasoning as **`reasoning_content`** (alias `reasoning`), and parses `reasoning_details` — `providers/openai/completion/mod.rs`
- Streaming chunks normalize `delta.reasoning_content.or(delta.reasoning)` + `delta.reasoning_details` — `providers/openai/completion/streaming.rs`
- `StreamedAssistantContent::{Text, ToolCall, ToolCallDelta, Reasoning, ReasoningDelta, Final, Unknown}` — `streaming.rs`
- `StreamingCompletionResponse::cancel()` — `streaming.rs`
- `stream: true` + `stream_options.include_usage` are injected automatically — `providers/openai/completion/streaming.rs`

See the companion spec: `docs/llm-crate-rig-migration-spec.md`.
