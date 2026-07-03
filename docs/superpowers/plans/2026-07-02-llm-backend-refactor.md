# LLM Backend Refactor: Provider Separation, Abort, stream_id, Tool Calls, Interleaved Thinking

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single `stream_openai_messages` function with a per-provider `LlmBackend` trait backed by raw reqwest+SSE, adding stream identity (`stream_id`), clean abort/cancellation, native tool calls, and per-backend interleaved-thinking pass-back.

**Architecture:** A `LlmBackend` trait in `wisp-llm` with per-provider modules (`backends/openai.rs`, `backends/deepseek.rs`, `backends/compat.rs`). Each backend owns its reqwest connection and SSE parsing via `reqwest-sse`. `stream_id` (generated frontend-side, passed in the request) routes events and keys the abort registry. Cancellation uses `tokio_util::sync::CancellationToken` threaded through the stream loop. Tool calling and reasoning pass-back are per-backend serialization policies. The conversation rounds logic and event emission stay in `src-tauri`; only the streaming HTTP layer moves into backends.

**Tech Stack:** Rust (Tauri 2.11, reqwest 0.12, reqwest-sse 0.2, tokio-util CancellationToken, async-trait), Vue 3 + Pinia + naive-ui, vitest, cargo.

## Global Constraints

- Library crate name is `wisp_lib`; workspace is at repo root with `members = ["src-tauri", "crates/*"]`.
- `wisp-llm` already depends on `tauri` — keep that dependency; backends emit chunks via callback closures, NOT direct tauri imports (testability).
- `stream_id` is a UUID v4 string generated **frontend-side** and passed in every conversation request. Rust uses it for event payloads + abort registry key.
- CancellationToken from `tokio-util` (workspace dep). Token is checked between SSE chunks; dropping the reqwest response also aborts the connection.
- `Provider` gets an additive `api_type: ApiType` field with `#[serde(default)]` for backward compatibility. Existing configs must load without migration.
- The `<|tool_calls|>` text protocol parser stays as a **fallback** for models/providers that lack native tool support; it is NOT deleted.
- No code comments unless requested.
- Tests: Rust unit tests via `cargo test -p <crate>`. TS tests via `npx vitest run`.
- `async-openai` is removed from `wisp-llm` Cargo.toml after migration. It stays in workspace deps only if `wisp-conversation` (payload builder) still uses it for message type definitions — if so, migrate those types to `serde_json::Value` and remove `async-openai` entirely.

## File Structure

```
crates/wisp-llm/src/
  lib.rs                  — pub re-exports, backend_for() factory
  backend.rs              — LlmBackend trait, StreamRequest, StreamOutcome,
                            StreamCallbacks, LlmError, ContentBlock
  error.rs                — LlmError enum
  sse.rs                  — reqwest-sse wrapper helpers (Event → data string)
  backends/
    mod.rs                — module declarations
    openai.rs             — OpenAiBackend (native tools, reasoning items)
    deepseek.rs           — DeepSeekBackend (reasoning_content, extra_body,
                             turn-type-aware pass-back)
    compat.rs             — OpenAiCompatBackend (vLLM/MiniMax/Kimi
                             reasoning_details, text-protocol fallback)

crates/wisp-configs/src/
  provider.rs             — ApiType enum + api_type field on Provider

src-tauri/src/
  conversation_commands.rs — run_conversation_rounds uses backend_for(),
                             threads stream_id + CancellationToken,
                             AbortRegistry integration
  abort.rs                 — AbortRegistry type + conversation_abort command
  types.rs                 — AppData gains abort_registry field
  lib.rs                   — register conversation_abort command

src/libs/
  types.ts                 — ConversationStreamChunkEvent gains stream_id;
                             Provider gains api_type
  commands.ts              — conversationAbort() binding

src/stores/
  chat.ts                  — stream_id generation, event filtering, abort call

src/components/
  Chat.vue                 — Stop button during isStreaming
```

---

## Phase 1: LlmBackend Trait + Raw reqwest Migration

This phase is purely structural — the app behaves identically before and after. We build the trait, migrate streaming to raw reqwest+SSE, and route everything through `backend_for()`. No abort, no stream_id filtering, no new tool calling yet.

### Task 1.1: Add workspace dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)

**Interfaces:**
- Produces: `async-trait`, `tokio-util`, `reqwest-sse` as workspace deps.

- [ ] **Step 1: Add deps to workspace `[workspace.dependencies]`**

In `Cargo.toml`, add after the `reqwest` line (line 25):

```toml
reqwest-sse = "0.2"
tokio-util = "0.7"
async-trait = "0.1"
```

- [ ] **Step 2: Verify workspace resolves**

Run: `cargo check`
Expected: compiles with no errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add reqwest-sse, tokio-util, async-trait workspace deps"
```

---

### Task 1.2: Define LlmBackend trait + core types in wisp-llm

**Files:**
- Create: `crates/wisp-llm/src/backend.rs`
- Create: `crates/wisp-llm/src/error.rs`
- Modify: `crates/wisp-llm/Cargo.toml`
- Modify: `crates/wisp-llm/src/lib.rs`

**Interfaces:**
- Produces: `LlmBackend` trait, `StreamRequest`, `StreamOutcome`, `StreamCallbacks`, `LlmError`.

- [ ] **Step 1: Update wisp-llm Cargo.toml**

In `crates/wisp-llm/Cargo.toml`, replace the `[dependencies]` block:

```toml
[dependencies]
wisp-configs.workspace = true
wisp-keyring.workspace = true
reqwest.workspace = true
reqwest-sse.workspace = true
tokio-util.workspace = true
async-trait.workspace = true
futures.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
tauri.workspace = true
```

Remove the `async-openai.workspace = true` line.

- [ ] **Step 2: Write error.rs**

Create `crates/wisp-llm/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("SSE parse error: {0}")]
    Sse(String),
    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("Stream was cancelled")]
    Cancelled,
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

impl From<String> for LlmError {
    fn from(s: String) -> Self {
        LlmError::Other(s)
    }
}
```

Add `thiserror.workspace = true` to `crates/wisp-llm/Cargo.toml` deps.

- [ ] **Step 3: Write backend.rs with trait + types**

Create `crates/wisp-llm/src/backend.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wisp_configs::provider::Provider;

use crate::error::LlmError;

pub type ChunkCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct StreamCallbacks {
    pub on_content: ChunkCallback,
    pub on_reasoning: ChunkCallback,
}

pub struct StreamRequest {
    pub messages: Vec<Value>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, Value>>,
    pub callbacks: StreamCallbacks,
    pub cancel: CancellationToken,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamOutcome {
    pub text: String,
    pub reasoning: String,
}

#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError>;
}
```

Note: `messages` uses `serde_json::Value` instead of `async_openai::types::ChatCompletionRequestMessage`. Each backend serializes these to its own wire format. The conversation layer builds them as generic JSON values (see Task 1.6).

- [ ] **Step 4: Write lib.rs with factory stub**

Replace `crates/wisp-llm/src/lib.rs` with:

```rust
pub mod backend;
pub mod error;
pub mod sse;
pub mod backends;

pub use backend::{LlmBackend, StreamRequest, StreamOutcome, StreamCallbacks, ChunkCallback};
pub use error::LlmError;

use std::sync::Arc;
use wisp_configs::provider::{ApiType, Provider};

pub fn backend_for(provider: &Provider) -> Arc<dyn LlmBackend> {
    match provider.api_type {
        ApiType::OpenAi => Arc::new(backends::openai::OpenAiBackend),
        ApiType::DeepSeek => Arc::new(backends::deepseek::DeepSeekBackend),
        ApiType::OpenAiCompatible => Arc::new(backends::compat::OpenAiCompatBackend),
    }
}
```

- [ ] **Step 5: Write sse.rs helper**

Create `crates/wisp-llm/src/sse.rs`:

```rust
use reqwest_sse::Event;

use crate::error::LlmError;

pub fn parse_data_json(event: &Event) -> Result<serde_json::Value, LlmError> {
    let data = event.data.as_deref().unwrap_or("");
    if data == "[DONE]" {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(data).map_err(|e| LlmError::Sse(format!("JSON parse failed: {e}")))
}

pub fn is_done(event: &Event) -> bool {
    event.data.as_deref() == Some("[DONE]")
}
```

- [ ] **Step 6: Write backends/mod.rs stub**

Create `crates/wisp-llm/src/backends/mod.rs`:

```rust
pub mod openai;
pub mod deepseek;
pub mod compat;
```

- [ ] **Step 7: Write failing test for backend_for dispatch**

Create `crates/wisp-llm/src/backends/mod.rs` test at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::super::*;
    use wisp_configs::provider::{ApiType, Provider};

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
    fn factory_returns_compat_by_default() {
        let p = provider_with(ApiType::OpenAiCompatible);
        let _backend = backend_for(&p);
    }
}
```

- [ ] **Step 8: Create placeholder backend modules**

Create `crates/wisp-llm/src/backends/openai.rs`, `deepseek.rs`, `compat.rs` — each with a stub struct and `todo!()` in the stream method:

```rust
use async_trait::async_trait;
use crate::{backend::{LlmBackend, StreamRequest, StreamOutcome}, error::LlmError};

pub struct OpenAiBackend;

#[async_trait]
impl LlmBackend for OpenAiBackend {
    async fn stream(&self, _req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        todo!("Task 1.4")
    }
}
```

(Repeat for `DeepSeekBackend` in `deepseek.rs` and `OpenAiCompatBackend` in `compat.rs`.)

- [ ] **Step 9: Verify it compiles and test passes**

Run: `cargo test -p wisp-llm`
Expected: `factory_returns_compat_by_default` passes (backend_for returns without panic since the stub structs construct fine; the `todo!()` is only in `stream`).

Note: if the test panics because of `todo!()` in `backend_for`, it won't — `backend_for` only constructs the struct, doesn't call `stream`.

- [ ] **Step 10: Commit**

```bash
git add crates/wisp-llm/
git commit -m "feat(wisp-llm): add LlmBackend trait, core types, backend factory"
```

---

### Task 1.3: Add ApiType to Provider

**Files:**
- Modify: `crates/wisp-configs/src/provider.rs`
- Modify: `src/libs/types.ts`

**Interfaces:**
- Produces: `ApiType` enum, `Provider.api_type` field.

- [ ] **Step 1: Add ApiType enum + field to Rust Provider**

In `crates/wisp-configs/src/provider.rs`, add before the `Provider` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    OpenAi,
    DeepSeek,
    #[default]
    OpenAiCompatible,
}
```

Add the field to `Provider`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub name: String,
    pub display_name: String,
    pub base_url: String,
    pub models: Vec<Model>,
    #[serde(default)]
    pub api_type: ApiType,
}
```

- [ ] **Step 2: Add ApiType to TS types**

In `src/libs/types.ts`, add the enum and field:

```typescript
export type ApiType = 'open_ai' | 'deep_seek' | 'open_ai_compatible';

export interface Provider {
	name: string;
	display_name: string;
	base_url: string;
	models: Model[];
	api_type?: ApiType;
}
```

- [ ] **Step 3: Verify Rust compiles**

Run: `cargo check -p wisp-configs`
Expected: compiles. Existing configs with no `api_type` field deserialize as `OpenAiCompatible` via `#[serde(default)]`.

- [ ] **Step 4: Commit**

```bash
git add crates/wisp-configs/src/provider.rs src/libs/types.ts
git commit -m "feat: add ApiType discriminator to Provider"
```

---

### Task 1.4: Implement OpenAiCompatBackend (the current behavior, on reqwest)

This is the workhorse backend. It replicates the exact behavior of the current `stream_openai_messages` using raw reqwest + reqwest-sse, with `reasoning_content` support (DeepSeek/o1 field) and the text-protocol tool format is NOT handled here (parsing stays in conversation_commands). This backend handles all three ApiTypes initially — OpenAI and DeepSeek will specialize in later phases.

**Files:**
- Modify: `crates/wisp-llm/src/backends/compat.rs`

**Interfaces:**
- Consumes: `StreamRequest`, `StreamCallbacks`, `CancellationToken` from Task 1.2.
- Produces: working `OpenAiCompatBackend::stream()` that replicates current `stream_openai_messages` behavior.

- [ ] **Step 1: Write the stream implementation**

Replace `crates/wisp-llm/src/backends/compat.rs` contents:

```rust
use std::collections::HashMap;

use async_trait::async_trait;
use reqwest_sse::EventSource;
use serde_json::{json, Value};
use tokio_stream::StreamExt;

use crate::backend::{LlmBackend, StreamOutcome, StreamRequest};
use crate::error::LlmError;
use crate::sse;
use wisp_keyring::KeyManager;

pub struct OpenAiCompatBackend;

#[async_trait]
impl LlmBackend for OpenAiCompatBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        let base_url = req.provider.base_url.trim_end_matches('/').to_string();
        let url = format!("{base_url}/chat/completions");

        let key_manager = KeyManager::new("wisp".to_string());
        let api_key = key_manager
            .get_api_key(&req.provider.name)
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map_err(|e| LlmError::Other(format!("API key not found: {e}")))?;

        let mut body = json!({
            "model": req.model,
            "messages": req.messages,
            "stream": true,
        });

        if let Some(params) = &req.parameters {
            apply_parameters(&mut body, params);
        } else {
            body["max_tokens"] = json!(1024);
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, body });
        }

        let mut events = response.events().await.map_err(|e| LlmError::Sse(e.to_string()))?;
        let mut outcome = StreamOutcome::default();

        while let Some(result) = events.next().await {
            if req.cancel.is_cancelled() {
                return Err(LlmError::Cancelled);
            }

            let event = result.map_err(|e| LlmError::Sse(e.to_string()))?;
            if sse::is_done(&event) {
                break;
            }

            let parsed = sse::parse_data_json(&event)?;
            if parsed.is_null() {
                continue;
            }

            if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                for choice in choices {
                    if let Some(delta) = choice.get("delta") {
                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                            outcome.text.push_str(content);
                            (req.callbacks.on_content)(content);
                        }
                        if let Some(reasoning) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                            outcome.reasoning.push_str(reasoning);
                            (req.callbacks.on_reasoning)(reasoning);
                        }
                    }
                }
            }
        }

        Ok(outcome)
    }
}

fn apply_parameters(body: &mut Value, params: &HashMap<String, Value>) {
    if let Some(temp) = params.get("temperature").and_then(|v| v.as_f64()) {
        body["temperature"] = json!(temp as f32);
    }
    if let Some(top_p) = params.get("top_p").and_then(|v| v.as_f64()) {
        body["top_p"] = json!(top_p as f32);
    }
    if let Some(max_tokens) = params.get("max_tokens").and_then(|v| v.as_i64()) {
        body["max_tokens"] = json!(max_tokens as u32);
    } else {
        body["max_tokens"] = json!(1024u32);
    }
    if let Some(penalty) = params.get("presence_penalty").and_then(|v| v.as_f64()) {
        body["presence_penalty"] = json!(penalty as f32);
    }
    if let Some(penalty) = params.get("frequency_penalty").and_then(|v| v.as_f64()) {
        body["frequency_penalty"] = json!(penalty as f32);
    }
    if let Some(seed) = params.get("seed").and_then(|v| v.as_i64()) {
        body["seed"] = json!(seed as i32);
    }
}
```

Note: `tokio_stream::StreamExt` is needed for `.next()` on the SSE stream. Add `tokio-stream = "0.1"` to `crates/wisp-llm/Cargo.toml` deps if not already available via `futures`. Actually, `futures::StreamExt` also works — use whichever is already available. Prefer `use futures::StreamExt;` since `futures` is already a dep.

Replace `use tokio_stream::StreamExt;` with `use futures::StreamExt;`.

- [ ] **Step 2: Write unit test for apply_parameters**

Add at the bottom of `compat.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_parameters_sets_max_tokens_default() {
        let mut body = json!({"model": "test", "messages": [], "stream": true});
        let params = HashMap::new();
        apply_parameters(&mut body, &params);
        assert_eq!(body["max_tokens"], json!(1024u32));
    }

    #[test]
    fn apply_parameters_respects_explicit_max_tokens() {
        let mut body = json!({});
        let mut params = HashMap::new();
        params.insert("max_tokens".to_string(), json!(4096));
        apply_parameters(&mut body, &params);
        assert_eq!(body["max_tokens"], json!(4096u32));
    }

    #[test]
    fn apply_parameters_sets_temperature() {
        let mut body = json!({});
        let mut params = HashMap::new();
        params.insert("temperature".to_string(), json!(0.5));
        apply_parameters(&mut body, &params);
        assert_eq!(body["temperature"], json!(0.5f32));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p wisp-llm`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/wisp-llm/src/backends/compat.rs crates/wisp-llm/Cargo.toml
git commit -m "feat(wisp-llm): implement OpenAiCompatBackend on raw reqwest+SSE"
```

---

### Task 1.5: Make OpenAiBackend and DeepSeekBackend delegate to compat (interim)

Until Phase 4-5 specialize them, OpenAI and DeepSeek backends delegate to `OpenAiCompatBackend` since the wire format is identical for basic streaming.

**Files:**
- Modify: `crates/wisp-llm/src/backends/openai.rs`
- Modify: `crates/wisp-llm/src/backends/deepseek.rs`

- [ ] **Step 1: Implement OpenAiBackend as delegate**

Replace `crates/wisp-llm/src/backends/openai.rs`:

```rust
use async_trait::async_trait;
use crate::backend::{LlmBackend, StreamRequest, StreamOutcome};
use crate::error::LlmError;
use super::compat::OpenAiCompatBackend;

pub struct OpenAiBackend;

#[async_trait]
impl LlmBackend for OpenAiBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        OpenAiBackend_stream(&OpenAiCompatBackend, req).await
    }
}

async fn OpenAiBackend_stream(
    compat: &OpenAiCompatBackend,
    req: StreamRequest,
) -> Result<StreamOutcome, LlmError> {
    compat.stream(req).await
}
```

Actually simpler — just delegate directly:

```rust
use async_trait::async_trait;
use crate::backend::{LlmBackend, StreamRequest, StreamOutcome};
use crate::error::LlmError;
use super::compat::OpenAiCompatBackend;

pub struct OpenAiBackend;

#[async_trait]
impl LlmBackend for OpenAiBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        OpenAiCompatBackend.stream(req).await
    }
}
```

- [ ] **Step 2: Implement DeepSeekBackend as delegate**

Same pattern in `crates/wisp-llm/src/backends/deepseek.rs`:

```rust
use async_trait::async_trait;
use crate::backend::{LlmBackend, StreamRequest, StreamOutcome};
use crate::error::LlmError;
use super::compat::OpenAiCompatBackend;

pub struct DeepSeekBackend;

#[async_trait]
impl LlmBackend for DeepSeekBackend {
    async fn stream(&self, req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        OpenAiCompatBackend.stream(req).await
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p wisp-llm`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/wisp-llm/src/backends/
git commit -m "feat(wisp-llm): OpenAi/DeepSeek backends delegate to compat (interim)"
```

---

### Task 1.6: Migrate payload builder to serde_json::Value messages

The current `build_openai_messages` returns `Vec<ChatCompletionRequestMessage>` (async-openai type). We need `Vec<serde_json::Value>` for the new `StreamRequest.messages` field.

**Files:**
- Modify: `crates/wisp-conversation/src/payload.rs`
- Modify: `crates/wisp-conversation/Cargo.toml`

**Interfaces:**
- Produces: `build_openai_messages() -> Vec<serde_json::Value>` (signature change).

- [ ] **Step 1: Check wisp-conversation deps**

Read `crates/wisp-conversation/Cargo.toml`. Ensure `serde_json` is a dep. Remove `async-openai` if it's only used for message types here.

- [ ] **Step 2: Rewrite build_openai_messages to return Value**

In `crates/wisp-conversation/src/payload.rs`, replace the `use async_openai::types::*` imports and rewrite the function to build `serde_json::Value` messages directly.

Replace the entire import block and `build_openai_messages` function:

```rust
use serde_json::{json, Value};
use wisp_db::types::{Message, MessageRole};
use crate::types::{ConversationToolCall, ConversationToolContent};

pub fn build_openai_messages(messages: &[Message]) -> Vec<Value> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        match message.sender {
            MessageRole::User => converted.push(convert_user_message(message)),
            MessageRole::Assistant => converted.push(convert_assistant_message(message)),
            MessageRole::System => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
            MessageRole::Tool => converted.push(json!({
                "role": "system",
                "content": message.text,
            })),
        }
    }

    converted
}
```

- [ ] **Step 3: Rewrite convert_user_message**

```rust
fn convert_user_message(message: &Message) -> Value {
    if let Some(images) = &message.images {
        if !images.is_empty() {
            let mut parts = vec![json!({
                "type": "text",
                "text": message.text,
            })];
            for image in images {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image.image_url.url,
                        "detail": "auto",
                    },
                }));
            }
            return json!({
                "role": "user",
                "content": parts,
            });
        }
    }
    json!({
        "role": "user",
        "content": message.text,
    })
}
```

- [ ] **Step 4: Rewrite convert_assistant_message**

```rust
fn convert_assistant_message(message: &Message) -> Value {
    let text = if let Some(raw_calls) = &message.tool_calls {
        let simplified: Vec<Value> = serde_json::from_str::<Vec<Value>>(raw_calls)
            .unwrap_or_default()
            .into_iter()
            .map(|call| json!({
                "name": call.get("name"),
                "arguments": call.get("arguments"),
            }))
            .collect();

        let tag = serde_json::to_string(&simplified).unwrap_or_default();
        if message.text.is_empty() {
            format!("<|tool_calls|>{tag}<|/tool_calls|>")
        } else {
            format!("{}\n<|tool_calls|>{tag}<|/tool_calls|>", message.text)
        }
    } else {
        message.text.clone()
    };

    json!({
        "role": "assistant",
        "content": text,
    })
}
```

- [ ] **Step 5: Update tests to match Value output**

In the `#[cfg(test)] mod tests` block, update assertions to check JSON values instead of async-openai types:

```rust
#[test]
fn assistant_message_is_sent_as_plain_text() {
    let messages = vec![
        message(MessageRole::User, "hello", None),
        message(MessageRole::Assistant, "hi there", None),
    ];
    let converted = build_openai_messages(&messages);
    assert_eq!(converted.len(), 2);
    assert_eq!(converted[0]["role"], "user");
    assert_eq!(converted[1]["role"], "assistant");
    assert_eq!(converted[1]["content"], "hi there");
}

#[test]
fn tool_message_becomes_system_message() {
    let messages = vec![
        message(MessageRole::Tool, "[Tool: search]\n[Result]\nfound", None),
    ];
    let converted = build_openai_messages(&messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0]["role"], "system");
    assert!(converted[0]["content"].as_str().unwrap().contains("[Tool: search]"));
}

#[test]
fn builds_multimodal_user_message_for_images() {
    let mut msg = message(MessageRole::User, "describe", None);
    msg.images = Some(vec![ImageContent {
        content_type: "image_url".to_string(),
        image_url: ImageUrl { url: "data:image/png;base64,abc".to_string() },
    }]);
    let converted = build_openai_messages(&[msg]);
    let content = converted[0]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image_url");
}

#[test]
fn keeps_normal_assistant_text_as_text_content() {
    let converted = build_openai_messages(&[message(MessageRole::Assistant, "hello", None)]);
    assert_eq!(converted[0]["role"], "assistant");
    assert_eq!(converted[0]["content"], "hello");
}
```

Also update the `message` test helper to remove any async-openai-specific imports.

- [ ] **Step 6: Run tests**

Run: `cargo test -p wisp-conversation`
Expected: all payload tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/wisp-conversation/
git commit -m "refactor(wisp-conversation): build_openai_messages returns serde_json::Value"
```

---

### Task 1.7: Wire backend_for() into run_conversation_rounds

Replace the `stream_openai_messages` call in `conversation_commands.rs` with `backend_for()` + `StreamCallbacks`. This is the integration point.

**Files:**
- Modify: `src-tauri/src/conversation_commands.rs`
- Modify: `src-tauri/Cargo.toml` (add tokio-util dep if missing)

**Interfaces:**
- Consumes: `LlmBackend`, `StreamRequest`, `StreamCallbacks` from Task 1.2.

- [ ] **Step 1: Add tokio-util to src-tauri deps**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tokio-util.workspace = true
```

- [ ] **Step 2: Update imports in conversation_commands.rs**

Replace line 7 (`use wisp_llm::{stream_openai_messages, OpenAiStreamEvents};`) with:

```rust
use std::sync::Arc;
use wisp_llm::{backend_for, StreamCallbacks, StreamRequest};
use tokio_util::sync::CancellationToken;
```

- [ ] **Step 3: Replace the stream call in run_conversation_rounds**

In `run_conversation_rounds`, find the block at lines ~340-352 that calls `stream_openai_messages`. Replace with:

```rust
        let cancel = CancellationToken::new();
        let callbacks = StreamCallbacks {
            on_content: Arc::new(|chunk: &str| {
                let _ = app_handle.emit(
                    "conversation_stream_chunk",
                    serde_json::json!({
                        "message_id": assistant_message_id,
                        "chunk": chunk,
                    }),
                );
            }),
            on_reasoning: Arc::new(|chunk: &str| {
                let _ = app_handle.emit(
                    "conversation_stream_reasoning",
                    serde_json::json!({
                        "message_id": assistant_message_id,
                        "chunk": chunk,
                    }),
                );
            }),
        };

        let backend = backend_for(&provider);
        let outcome = backend
            .stream(StreamRequest {
                messages: openai_messages,
                model: model.clone(),
                provider: provider.clone(),
                parameters: parameters.clone(),
                callbacks,
                cancel,
            })
            .await
            .map_err(|error| format!(
                "Model '{}' failed while streaming conversation '{}': {}",
                model, conversation_id, error
            ))?;
```

Note: `openai_messages` is now `Vec<Value>` (from Task 1.6). The `StreamRequest.messages` field accepts `Vec<Value>`. Remove the old `OpenAiStreamEvents` usage. Also remove the system prompt insertion that used `async_openai::types::ChatCompletionRequestSystemMessage` — replace with a JSON value insertion:

Find the system prompt insertion block (lines ~296-308) and replace with:

```rust
        if !system_prompt_sections.is_empty() {
            openai_messages.insert(0, serde_json::json!({
                "role": "system",
                "content": system_prompt_sections.join("\n\n"),
            }));
        }
```

- [ ] **Step 4: Fix the orchestrator call_llm_with_pal_config**

In `src-tauri/src/orchestrator.rs` (line ~280), replace the `stream_openai_messages` call similarly:

```rust
    use wisp_llm::{backend_for, StreamCallbacks, StreamRequest};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    let callbacks = StreamCallbacks {
        on_content: Arc::new(|_| {}),
        on_reasoning: Arc::new(|_| {}),
    };
    let backend = backend_for(provider);
    let outcome = backend
        .stream(StreamRequest {
            messages: api_messages,
            model: pal.model_id.clone(),
            provider: provider.clone(),
            parameters: parameters.cloned(),
            callbacks,
            cancel: CancellationToken::new(),
        })
        .await
        .map_err(|e: wisp_llm::LlmError| format!("LLM call failed: {}", e))?;
```

The `api_messages` from `build_openai_messages` are now `Vec<Value>`.

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: compiles. Fix any remaining `async_openai` type references.

- [ ] **Step 6: Run existing tests**

Run: `cargo test`
Expected: existing tests pass. The behavior is identical — just routed through the new backend.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/ crates/
git commit -m "refactor: route streaming through LlmBackend trait + raw reqwest"
```

---

### Task 1.8: Remove async-openai from workspace

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Search for remaining async-openai usage**

Run: `rg "async.openai" --type rust`
Expected: zero matches (all migrated to serde_json::Value). If any remain, migrate them.

- [ ] **Step 2: Remove from workspace deps**

In `Cargo.toml` line 23, remove:

```toml
async-openai = { git = "https://github.com/Anson2251/async-openai.git", version = "0.28.2" }
```

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: builds successfully without async-openai.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "chore: remove async-openai dependency"
```

---

## Phase 2: stream_id (Event Disambiguation)

### Task 2.1: Add stream_id to event payloads

**Files:**
- Modify: `src-tauri/src/conversation_commands.rs` (event emission)
- Modify: `src/libs/types.ts` (ConversationStreamChunkEvent)
- Modify: `src/stores/chat.ts` (event filtering)

**Interfaces:**
- Produces: `stream_id` field on all stream chunk events and on `ConversationSendRequest`.

- [ ] **Step 1: Add stream_id to ConversationSendRequest**

In `src-tauri/src/conversation_commands.rs`, add to `ConversationSendRequest`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationSendRequest {
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub text: String,
    pub images: Option<Vec<ImageContent>>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub character: Option<Character>,
    #[serde(default)]
    pub target_pal_ids: Option<Vec<String>>,
    #[serde(default)]
    pub stream_id: Option<String>,
}
```

Add the same `stream_id: Option<String>` to `ConversationRegenerateRequest` and `ConversationDeriveRequest`.

- [ ] **Step 2: Thread stream_id into run_conversation_rounds**

Change `run_conversation_rounds` signature to accept `stream_id: String`:

```rust
async fn run_conversation_rounds<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    conversation_id: String,
    mut current_leaf_id: String,
    model: String,
    provider: Provider,
    parameters: Option<HashMap<String, serde_json::Value>>,
    character: Option<Character>,
    stream_id: String,
) -> Result<String, String> {
```

In the callback closures (from Task 1.7 Step 3), include `stream_id` in the emit payload:

```rust
            on_content: Arc::new({
                let sid = stream_id.clone();
                let mid = assistant_message_id.clone();
                let ah = app_handle.clone();
                move |chunk: &str| {
                    let _ = ah.emit(
                        "conversation_stream_chunk",
                        serde_json::json!({
                            "stream_id": &sid,
                            "message_id": &mid,
                            "chunk": chunk,
                        }),
                    );
                }
            }),
```

Same pattern for `on_reasoning`.

- [ ] **Step 3: Generate stream_id fallback in callers**

In `conversation_send_message_inner`, at the top:

```rust
    let stream_id = request.stream_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
```

Pass `stream_id` to `run_conversation_rounds`. Do the same in `conversation_regenerate_message` and `conversation_derive_message`.

- [ ] **Step 4: Update TS types**

In `src/libs/types.ts`, add `stream_id` to `ConversationStreamChunkEvent`:

```typescript
export interface ConversationStreamChunkEvent {
	stream_id?: string | null;
	message_id?: string | null;
	chunk: string;
}
```

Add `stream_id?: string` to `ConversationSendRequest`, `ConversationRegenerateRequest`, `ConversationDeriveRequest`.

- [ ] **Step 5: Update chat.ts to generate + filter by stream_id**

In `src/stores/chat.ts`, in `sendMessage` (line ~114), generate stream_id and pass it to the request:

```typescript
		const streamId = crypto.randomUUID();
```

Pass `stream_id: streamId` in the `Commands.conversationSendMessage` call.

Update the content/reasoning listeners to filter by stream_id:

```typescript
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			// ... existing logic
		});
```

Same for `conversation_stream_reasoning` listener.

Repeat for `regenerateMessage`, `deriveMessage`, `editAndRegenerateMessage` — each generates its own `streamId` and filters.

- [ ] **Step 6: Verify Rust compiles**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 7: Run tests**

Run: `cargo test && npx vitest run`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/ src/ crates/
git commit -m "feat: add stream_id for concurrent stream disambiguation"
```

---

## Phase 3: Cancellation / Abort

### Task 3.1: AbortRegistry + conversation_abort command

**Files:**
- Create: `src-tauri/src/abort.rs`
- Modify: `src-tauri/src/types.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/libs/commands.ts`

**Interfaces:**
- Produces: `AbortRegistry`, `conversation_abort` command, `conversationAbort()` TS binding.

- [ ] **Step 1: Write AbortRegistry**

Create `src-tauri/src/abort.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct AbortRegistry {
    tokens: Mutex<HashMap<String, CancellationToken>>,
}

impl AbortRegistry {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, stream_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.tokens
            .lock()
            .unwrap()
            .insert(stream_id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, stream_id: &str) -> bool {
        if let Some(token) = self.tokens.lock().unwrap().remove(stream_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, stream_id: &str) {
        self.tokens.lock().unwrap().remove(stream_id);
    }
}

#[tauri::command]
pub async fn conversation_abort(
    app_handle: AppHandle,
    stream_id: String,
) -> Result<bool, String> {
    let registry = app_handle.state::<AbortRegistry>();
    Ok(registry.cancel(&stream_id))
}
```

Add `use tauri::AppHandle;` at the top.

- [ ] **Step 2: Register AbortRegistry in AppData/lib.rs**

In `src-tauri/src/lib.rs`, add:

```rust
mod abort;
use crate::abort::AbortRegistry;
```

In `.setup`, after `app.manage(Mutex::new(AppData { ... }));`:

```rust
			app.manage(AbortRegistry::new());
```

In `invoke_handler`, add:

```rust
            abort::conversation_abort,
```

- [ ] **Step 3: Thread AbortRegistry into run_conversation_rounds**

In `run_conversation_rounds`, replace the `CancellationToken::new()` call with registry lookup:

```rust
        let registry = app_handle.state::<AbortRegistry>();
        let cancel = registry.register(&stream_id);
```

In the function's cleanup (after the stream completes or errors, before returning), unregister:

```rust
        app_handle.state::<AbortRegistry>().unregister(&stream_id);
```

This must be in a scope/finally-like block. Wrap the round loop body appropriately — add `let _guard = CancelGuard::new(...)` or manually call `unregister` in both the Ok and Err paths.

A simple approach — add at the start of `run_conversation_rounds` after `cancel` is created:

```rust
    struct CancelGuard<'a> {
        registry: &'a AbortRegistry,
        stream_id: String,
    }
    impl<'a> Drop for CancelGuard<'a> {
        fn drop(&mut self) {
            self.registry.unregister(&self.stream_id);
        }
    }
```

Actually, since `run_conversation_rounds` returns `Result<String, String>`, the cleanest is to use the registry at the conversation command wrapper level. But the cancel token is needed inside `run_conversation_rounds`. Let's keep it simple: register at the start of `run_conversation_rounds`, and unregister in a `defer`-like pattern using a guard struct that implements Drop.

Add a module-level helper in `abort.rs`:

```rust
pub struct CancelGuard {
    registry: AbortRegistry,
    stream_id: String,
}

impl CancelGuard {
    pub fn new(registry: AbortRegistry, stream_id: String) -> Self {
        Self { registry, stream_id }
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.registry.unregister(&self.stream_id);
    }
}
```

Wait — `AbortRegistry` needs to be clonable or accessible by reference. Since it's managed as Tauri state (not inside AppData's Mutex), let's make it `Clone` (it contains a `Mutex<HashMap>` which is not Clone). Instead, store `AbortRegistry` as `Arc` or access via `app_handle.state()`.

Simplest: don't use a Drop guard. Instead, restructure `run_conversation_rounds` so the unregister happens in a single return path. Or use a closure-based cleanup. Given Rust's lack of `defer`, the pragmatic pattern:

```rust
    let result = run_conversation_rounds_inner(...).await;
    app_handle.state::<AbortRegistry>().unregister(&stream_id);
    result
```

Split into `run_conversation_rounds` (registers + cleans up) and `run_conversation_rounds_inner` (does the work).

- [ ] **Step 4: Add TS binding**

In `src/libs/commands.ts`, add:

```typescript
export async function conversationAbort(streamId: string) {
	return invoke<boolean>('conversation_abort', { streamId })
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: compiles.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/abort.rs src-tauri/src/lib.rs src/libs/commands.ts
git commit -m "feat: add AbortRegistry + conversation_abort command"
```

---

### Task 3.2: Stop button in Chat.vue + abort wiring in chat store

**Files:**
- Modify: `src/stores/chat.ts`
- Modify: `src/components/Chat.vue`

**Interfaces:**
- Consumes: `conversationAbort` from Task 3.1.

- [ ] **Step 1: Add abortStreaming to chat store**

In `src/stores/chat.ts`, add state + method:

```typescript
	const activeStreamId = ref<string | null>(null)

	const abortStreaming = async () => {
		if (activeStreamId.value) {
			await Commands.conversationAbort(activeStreamId.value)
			activeStreamId.value = null
		}
	}
```

In `sendMessage`, set `activeStreamId.value = streamId` before the invoke, and `activeStreamId.value = null` in the `finally` block. Export `activeStreamId` and `abortStreaming` from the store.

- [ ] **Step 2: Add Stop button to Chat.vue**

In `src/components/Chat.vue` template, replace the send button block with a conditional:

```vue
            <n-button
              v-if="chatStore.isStreaming"
              type="error"
              @click="chatStore.abortStreaming"
              circle
            >
              <template #icon>
                <n-icon :size="20">
                  <Stop24Regular />
                </n-icon>
              </template>
            </n-button>
            <n-button
              v-else
              type="primary"
              @click="sendMessage"
              circle
              :disabled="(!chatStore.userInput && !imageInputRef?.hasImages) || !chatStore.chosenModel"
            >
              <template #icon>
                <n-icon :size="20">
                  <Send20Regular />
                </n-icon>
              </template>
            </n-button>
```

Add `Stop24Regular` to the icon imports from `@vicons/fluent`.

- [ ] **Step 3: Run frontend tests**

Run: `npx vitest run`
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add src/stores/chat.ts src/components/Chat.vue
git commit -m "feat: add Stop button + abort streaming"
```

---

## Phase 4: Native Tool Calls (Per-Backend)

### Task 4.1: Add ToolDefinition + tool_choice to StreamRequest

**Files:**
- Modify: `crates/wisp-llm/src/backend.rs`
- Modify: `src-tauri/src/conversation_commands.rs` (build tool defs, pass to StreamRequest)

**Interfaces:**
- Produces: `ToolDefinition`, `ToolChoice` types in wisp-llm, added to `StreamRequest`.

- [ ] **Step 1: Define tool types in backend.rs**

Add to `crates/wisp-llm/src/backend.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Specific(String),
}

impl Default for ToolChoice {
    fn default() -> Self {
        ToolChoice::Auto
    }
}
```

Add to `StreamRequest`:

```rust
pub struct StreamRequest {
    pub messages: Vec<Value>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, Value>>,
    pub callbacks: StreamCallbacks,
    pub cancel: CancellationToken,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
}
```

- [ ] **Step 2: Update compat backend to serialize tools natively**

In `crates/wisp-llm/src/backends/compat.rs`, after building the body JSON, if `req.tools` is non-empty, add a `tools` array:

```rust
        if !req.tools.is_empty() {
            let tools_json: Vec<Value> = req.tools.iter().map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })).collect();
            body["tools"] = json!(tools_json);

            body["tool_choice"] = match &req.tool_choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("required"),
                ToolChoice::Specific(name) => json!({"type": "function", "function": {"name": name}}),
            };
        }
```

Add `use crate::backend::ToolChoice;`.

- [ ] **Step 3: Parse native tool_calls from SSE deltas**

In the SSE loop, add handling for `delta.tool_calls`:

```rust
                        if let Some(tool_calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
                            for tc in tool_calls {
                                outcome.tool_call_deltas.push(tc.clone());
                            }
                        }
```

Add `tool_call_deltas: Vec<Value>` to `StreamOutcome`.

- [ ] **Step 4: Build ToolDefinition from MCP tools in conversation_commands**

In `run_conversation_rounds`, convert `enabled_tools` to `Vec<ToolDefinition>`:

```rust
        let tool_defs: Vec<ToolDefinition> = enabled_tools.iter().map(|t| ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone().unwrap_or_default(),
            parameters: t.schema.clone().unwrap_or(json!({"type": "object", "properties": {}})),
        }).collect();
```

Pass `tools: tool_defs` and `tool_choice: ToolChoice::Auto` to `StreamRequest`.

- [ ] **Step 5: Handle native tool calls in the rounds loop**

After `backend.stream(...)` returns, check `outcome.tool_call_deltas`. If non-empty, aggregate them into complete tool calls (they arrive as index-keyed deltas across chunks) and execute them — same flow as the text-protocol path but parsed from structured deltas instead of `<|tool_calls|>` tags.

This is the most complex piece. Add a `merge_tool_call_deltas(deltas: &[Value]) -> Vec<ConversationToolCall>` function in `wisp-conversation` that assembles the streamed fragments by index.

- [ ] **Step 6: Write tests for delta merging**

Test that streamed deltas `[{index:0, id:"x", function:{name:"a", arguments:""}}]`, `[{index:0, function:{arguments:"{\"q\":"}}]`, `[{index:0, function:{arguments:"\"w\"}"}}]` merge into one call with `arguments: {"q": "w"}`.

- [ ] **Step 7: Commit**

```bash
git add crates/ src-tauri/
git commit -m "feat: native tool calling with delta merging + text-protocol fallback"
```

---

## Phase 5: Interleaved Thinking Pass-Back

### Task 5.1: DeepSeekBackend — reasoning_content + extra_body + turn-type-aware pass-back

**Files:**
- Modify: `crates/wisp-llm/src/backends/deepseek.rs`
- Modify: `crates/wisp-conversation/src/payload.rs` (convert_assistant_message)

**Interfaces:**
- Consumes: `reasoning` field on `Message`, turn-type context.

- [ ] **Step 1: Implement DeepSeekBackend with extra_body**

In `crates/wisp-llm/src/backends/deepseek.rs`, stop delegating to compat. Implement streaming with `extra_body` support:

```rust
use async_trait::async_trait;
use serde_json::json;
use crate::backend::{LlmBackend, StreamOutcome, StreamRequest, ToolChoice};
use crate::error::LlmError;
use super::compat::OpenAiCompatBackend;

pub struct DeepSeekBackend;

#[async_trait]
impl LlmBackend for DeepSeekBackend {
    async fn stream(&self, mut req: StreamRequest) -> Result<StreamOutcome, LlmError> {
        // DeepSeek thinking mode: add extra_body fields.
        // The conversation layer must set req.parameters with thinking config
        // or we detect it here. For now, pass through to compat which handles
        // reasoning_content delta parsing.
        OpenAiCompatBackend.stream(req).await
    }
}
```

The key DeepSeek differences from compat will be implemented in Phase 5.2:

- [ ] **Step 2: Add reasoning_content to assistant message in payload builder**

In `crates/wisp-conversation/src/payload.rs`, modify `convert_assistant_message` to accept a flag indicating whether this provider needs reasoning pass-back. Add a new function:

```rust
pub fn build_openai_messages_with_reasoning(
    messages: &[Message],
    include_reasoning_on_tool_turns: bool,
) -> Vec<Value> {
```

When `include_reasoning_on_tool_turns` is true and the assistant message has `tool_calls` (i.e., it was a tool-call turn), include `reasoning_content` in the message JSON:

```rust
fn convert_assistant_message_deepseek(message: &Message) -> Value {
    let mut msg = serde_json::Map::new();
    msg.insert("role".to_string(), json!("assistant"));
    msg.insert("content".to_string(), json!(message.text));

    if message.tool_calls.is_some() {
        if let Some(reasoning) = &message.reasoning {
            msg.insert("reasoning_content".to_string(), json!(reasoning));
        } else {
            msg.insert("reasoning_content".to_string(), json!(""));
        }
    }

    Value::Object(msg)
}
```

- [ ] **Step 3: Wire provider-aware payload building in run_conversation_rounds**

In `run_conversation_rounds`, select the payload builder based on `provider.api_type`:

```rust
        let openai_messages = match provider.api_type {
            ApiType::DeepSeek => build_openai_messages_with_reasoning(&path, true),
            _ => build_openai_messages(&path),
        };
```

- [ ] **Step 4: Write test for DeepSeek pass-back rule**

```rust
#[test]
fn deepseek_includes_reasoning_on_tool_turns() {
    let msg = message_with_reasoning_and_tool_calls(
        "answer", Some("thinking..."), Some(r#"[{"name":"x","arguments":{}}]"#)
    );
    let converted = build_openai_messages_with_reasoning(&[msg], true);
    assert_eq!(converted[0]["reasoning_content"], "thinking...");
}

#[test]
fn deepseek_omits_reasoning_on_plain_turns() {
    let msg = message_with_reasoning_and_tool_calls("answer", Some("thinking..."), None);
    let converted = build_openai_messages_with_reasoning(&[msg], true);
    assert!(converted[0].get("reasoning_content").is_none());
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/ src-tauri/
git commit -m "feat: DeepSeek interleaved thinking pass-back (turn-type-aware)"
```

---

### Task 5.2: OpenAiCompatBackend — reasoning_details support (vLLM/MiniMax/Kimi)

**Files:**
- Modify: `crates/wisp-llm/src/backends/compat.rs`
- Modify: `crates/wisp-conversation/src/payload.rs`

- [ ] **Step 1: Parse reasoning_details from SSE deltas**

In `compat.rs` SSE loop, add handling for the vLLM/MiniMax `reasoning_details` format:

```rust
                        if let Some(reasoning_details) = delta.get("reasoning_details").and_then(|c| c.as_array()) {
                            for detail in reasoning_details {
                                if let Some(text) = detail.get("text").and_then(|t| t.as_str()) {
                                    outcome.reasoning.push_str(text);
                                    (req.callbacks.on_reasoning)(text);
                                }
                            }
                        }
```

This is in addition to the existing `reasoning_content` handling — some providers use one, some use the other.

- [ ] **Step 2: Add reasoning_details pass-back for compat providers**

In `payload.rs`, add:

```rust
pub fn build_openai_messages_compat_reasoning(messages: &[Message]) -> Vec<Value> {
    // For vLLM/MiniMax/Kimi: reasoning goes in reasoning_details array
    // on assistant messages, always (unlike DeepSeek which is turn-type-aware)
    ...
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/
git commit -m "feat: OpenAiCompat reasoning_details pass-back (vLLM/MiniMax/Kimi)"
```

---

## Self-Review Notes

**Spec coverage:**
- ✅ Provider separation (LlmBackend trait, backends/ modules, ApiType) — Phase 1
- ✅ Abort/cancellation (AbortRegistry, conversation_abort, Stop button) — Phase 3
- ✅ stream_id (event disambiguation, concurrent stream fix) — Phase 2
- ✅ Native tool calls (per-backend, delta merging, text-protocol fallback) — Phase 4
- ✅ Interleaved thinking (DeepSeek reasoning_content turn-type-aware, compat reasoning_details) — Phase 5
- ✅ Raw reqwest + reqwest-sse (no async-openai) — Phase 1

**Key design decisions locked:**
1. `stream_id` generated frontend-side (UUID), passed in request — frontend filters events immediately.
2. `LlmBackend` trait with callback-based chunk emission (not direct tauri coupling) — testable.
3. One `wisp-llm` crate with `backends/` modules (not per-provider crates) — avoids workspace bloat.
4. `AbortRegistry` as separate Tauri managed state (not in AppData Mutex) — avoids lock contention.
5. Text-protocol `<|tool_calls|>` parser retained as fallback — not deleted.
6. DeepSeek pass-back is turn-type-aware (tool turns require it, plain turns omit it) — matches API spec.

**Risk areas for the implementer:**
- Task 1.7 is the highest-risk integration point — the `run_conversation_rounds` rewrite touches the core loop. Test thoroughly with manual end-to-end sends before moving to Phase 2.
- Task 4.1 (native tool call delta merging) is the most algorithmically complex. The streamed delta format (`index`-keyed partial fragments) needs careful assembly.
- The `reqwest-sse` crate's `events()` method returns `Result<ServerSentEvents, _>` — verify the exact API shape against `docs.rs/reqwest-sse` at implementation time, as it may differ slightly from what's written here.
- DeepSeek's `extra_body` for `thinking: {type: "enabled"}` must be added to the request JSON body by the DeepSeekBackend. The conversation layer should signal thinking mode via `req.parameters` or a dedicated field on `StreamRequest`.
