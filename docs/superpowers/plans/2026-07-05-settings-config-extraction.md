# Settings Config Extraction: Move Pipeline/Conversation Configs out of MCP + Wire ConversationLoopConfig

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `PipelineConfig` and `ConversationLoopConfig` out of the MCP namespace (storage, commands, UI) into app-level Settings (`wisp-configs` → `configs.toml`), and wire `ConversationLoopConfig` into the actual conversation loop backend so it controls behaviour instead of being dead config.

**Architecture:** Config type definitions move from `wisp-mcp/src/types.rs` → `wisp-configs/src/settings.rs`. Storage moves from `mcp_config.json` (`McpConfig` struct) → `configs.toml` (`Config` struct in `wisp-configs`). Tauri commands move from `mcp_commands.rs` → `settings_commands.rs` with `settings_*` naming. Two new pure, testable utility modules are added to `wisp-conversation`: `context_trim` (token estimation + sliding-window context trimming) and `retry` (async retry with fixed delay backoff). These utilities are consumed by `run_conversation_rounds_inner` in `conversation_commands.rs`, replacing hardcoded values (`0..10`, no retry, no trimming, unconditional vision).

**Tech Stack:** Rust (Tauri 2.11, tokio, serde), Vue 3 + Pinia + naive-ui, vitest, cargo test.

## Global Constraints

- Library crate name is `wisp_lib`; workspace is at repo root with `members = ["src-tauri", "crates/*"]`.
- Old `mcp_config.json` files with `pipeline_config`/`conversation_config` fields still load — serde ignores unknown fields on `McpConfig` after removal. No migration script needed; old values are simply dropped (acceptable: `ConversationLoopConfig` was never consumed, `PipelineConfig` has sensible defaults).
- Existing `configs.toml` files without the new fields still load — fields are `Option` with `#[serde(default)]`.
- `wisp-configs` does NOT depend on `wisp-mcp` and must not gain that dependency. The types move TO `wisp-configs`.
- `wisp-conversation` already depends on `wisp-db` (for `Message` type) — `context_trim` operates on `Vec<wisp_db::types::Message>`.
- No code comments unless requested.
- Tests: Rust unit tests via `cargo test -p <crate>`. TS tests via `npx vitest run`.
- TDD: every new function gets a failing test first. Watch it fail. Then implement minimal code.
- Conversation loop is `run_conversation_rounds_inner` in `src-tauri/src/conversation_commands.rs` — NOT the unused `ConversationEngine` in `wisp-conversation/src/engine.rs`.

## File Structure

```
crates/wisp-configs/src/
  settings.rs              — NEW: PipelineConfig + ConversationLoopConfig definitions
                             (moved from wisp-mcp/src/types.rs)
  manager.rs               — Config struct gains pipeline_config + conversation_config
                             fields; getter/setter methods added
  lib.rs                   — pub mod settings; re-exports

crates/wisp-mcp/src/
  types.rs                 — PipelineConfig, ConversationLoopConfig, and their default
                             fns REMOVED; McpConfig loses pipeline_config +
                             conversation_config fields
  config.rs                — get/update pipeline/conversation methods REMOVED from
                             McpConfigManager

crates/wisp-conversation/src/
  context_trim.rs          — NEW: estimate_tokens() + trim_context() pure functions
  retry.rs                 — NEW: retry_with_backoff() async function
  lib.rs                   — pub mod context_trim; pub mod retry; re-exports

src-tauri/src/
  settings_commands.rs     — NEW: settings_get/update_pipeline_config,
                             settings_get/update_conversation_config
  conversation_commands.rs — run_conversation_rounds_inner reads
                             ConversationLoopConfig, uses trim_context + retry +
                             max_tool_rounds + enable_vision_injection
  mcp_commands.rs          — 4 config commands REMOVED
  types.rs                 — AppData unchanged (config_manager already has settings)
  lib.rs                   — register settings_commands::*, unregister 4 mcp config
                             commands

src/libs/
  types.ts                 — PipelineConfig + ConversationLoopConfig stay (shared
                             types, just imported from a different conceptual area)
  commands.ts              — mcpGet/UpdatePipelineConfig → settingsGet/UpdatePipelineConfig
                             mcpGet/UpdateConversationConfig → settingsGet/UpdateConversationConfig

src/stores/
  settings.ts              — NEW: pipelineConfig + conversationConfig state,
                             load/save methods (moved from mcp store)
  mcp.ts                   — pipelineConfig/conversationConfig state + methods REMOVED;
                             processToolResult reads from settings store

src/components/
  PipelineConfigForm.vue   — RENAMED from McpPipelineConfig.vue; uses settings store
  ConversationConfigForm.vue — RENAMED from McpConversationConfig.vue; uses settings store

src/views/
  SettingsView.vue         — gains PipelineConfigForm + ConversationConfigForm sections

src/components/
  McpServerList.vue        — pipeline/conversation config buttons + drawer REMOVED
```

---

## Phase 1: Backend Config Type Move

Structural refactor — no behaviour change. Types move from `wisp-mcp` to `wisp-configs`, storage moves from `mcp_config.json` to `configs.toml`.

### Task 1.1: Create `settings.rs` in `wisp-configs` with types + tests

**Files:**
- Create: `crates/wisp-configs/src/settings.rs`
- Modify: `crates/wisp-configs/src/lib.rs`

**Interfaces:**
- Produces: `wisp_configs::PipelineConfig`, `wisp_configs::ConversationLoopConfig`, `wisp_configs::DEFAULT_PIPELINE_CONFIG` (for test convenience)

- [ ] **Step 1: Write failing tests for settings types**

Create `crates/wisp-configs/src/settings.rs` with only the test module:

```rust
use serde::{Deserialize, Serialize};

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wisp-configs settings::tests`
Expected: FAIL — `PipelineConfig` / `ConversationLoopConfig` not found

- [ ] **Step 3: Implement the types**

Add the struct definitions above the tests in `settings.rs`. Move the full definitions (including all `default_*` functions and `Default` impls) verbatim from `wisp-mcp/src/types.rs:223-378`:

```rust
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

// ... all default_* fns and Default impl (copied from wisp-mcp) ...

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

// ... all default_* fns and Default impl (copied from wisp-mcp) ...
```

- [ ] **Step 4: Register module in `lib.rs`**

In `crates/wisp-configs/src/lib.rs`, add:

```rust
pub mod settings;
```

And update the re-exports:

```rust
pub use settings::{PipelineConfig, ConversationLoopConfig};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p wisp-configs settings::tests`
Expected: PASS — all 6 tests green

- [ ] **Step 6: Commit**

```bash
git add crates/wisp-configs/src/settings.rs crates/wisp-configs/src/lib.rs
git commit -m "feat(configs): add PipelineConfig + ConversationLoopConfig to wisp-configs"
```

---

### Task 1.2: Add settings to Config struct + ConfigManager getters/setters

**Files:**
- Modify: `crates/wisp-configs/src/manager.rs`

**Interfaces:**
- Produces: `ConfigManager::get_pipeline_config()`, `ConfigManager::update_pipeline_config()`, `ConfigManager::get_conversation_config()`, `ConfigManager::update_conversation_config()`

- [ ] **Step 1: Write failing tests for ConfigManager settings round-trip**

Add to the `Config` struct section (or a new test block at bottom of `manager.rs`). Since `ConfigManager::new` requires an `AppHandle`, test the `Config` serialization directly:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_with_settings_serializes_to_toml() {
        let config = Config {
            providers: vec![],
            characters: vec![],
            default_responder_id: None,
            chore_llm: None,
            pipeline_config: Some(crate::settings::PipelineConfig {
                jpeg_quality: 50,
                ..Default::default()
            }),
            conversation_config: Some(crate::settings::ConversationLoopConfig {
                max_tool_rounds: 7,
                ..Default::default()
            }),
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("jpeg_quality = 50"));
        assert!(toml_str.contains("max_tool_rounds = 7"));
    }

    #[test]
    fn config_without_settings_deserializes_from_empty_toml() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.pipeline_config.is_none());
        assert!(config.conversation_config.is_none());
    }

    #[test]
    fn config_with_settings_roundtrips() {
        let config = Config {
            providers: vec![],
            characters: vec![],
            default_responder_id: None,
            chore_llm: None,
            pipeline_config: Some(crate::settings::PipelineConfig::default()),
            conversation_config: Some(crate::settings::ConversationLoopConfig {
                retry_attempts: 5,
                ..Default::default()
            }),
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            deserialized.conversation_config.as_ref().unwrap().retry_attempts,
            5
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wisp-configs manager::tests`
Expected: FAIL — `pipeline_config` / `conversation_config` fields not on `Config`

- [ ] **Step 3: Add fields to Config struct + getter/setter methods**

In `manager.rs`, add fields to `Config`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    providers: Vec<crate::provider::Provider>,
    characters: Vec<crate::character::Character>,
    #[serde(default)]
    default_responder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chore_llm: Option<ChoreLlmRef>,
    #[serde(default)]
    pipeline_config: Option<crate::settings::PipelineConfig>,
    #[serde(default)]
    conversation_config: Option<crate::settings::ConversationLoopConfig>,
}
```

Add methods to `impl ConfigManager` (after chore LLM methods):

```rust
    // ========== Pipeline Config ==========

    pub fn get_pipeline_config(&self) -> crate::settings::PipelineConfig {
        self.configs
            .lock()
            .unwrap()
            .pipeline_config
            .clone()
            .unwrap_or_default()
    }

    pub fn update_pipeline_config(
        &self,
        config: crate::settings::PipelineConfig,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.pipeline_config = Some(config);
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }

    // ========== Conversation Config ==========

    pub fn get_conversation_config(&self) -> crate::settings::ConversationLoopConfig {
        self.configs
            .lock()
            .unwrap()
            .conversation_config
            .clone()
            .unwrap_or_default()
    }

    pub fn update_conversation_config(
        &self,
        config: crate::settings::ConversationLoopConfig,
    ) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.conversation_config = Some(config);
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wisp-configs`
Expected: PASS — all tests green

- [ ] **Step 5: Commit**

```bash
git add crates/wisp-configs/src/manager.rs
git commit -m "feat(configs): add pipeline/conversation config to ConfigManager"
```

---

### Task 1.3: Remove types from `wisp-mcp`

**Files:**
- Modify: `crates/wisp-mcp/src/types.rs`
- Modify: `crates/wisp-mcp/src/config.rs`

**Interfaces:**
- Consumes: `wisp_configs::PipelineConfig`, `wisp_configs::ConversationLoopConfig` (re-exported from `wisp-mcp` for transition if needed, or update all import sites)

- [ ] **Step 1: Remove PipelineConfig + ConversationLoopConfig definitions from `types.rs`**

Delete lines 223-378 in `crates/wisp-mcp/src/types.rs` (the two structs, all `default_*` fns, both `Default` impls).

Remove `pipeline_config` and `conversation_config` fields from `McpConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    pub servers: Vec<ServerConfig>,
}
```

- [ ] **Step 2: Remove pipeline/conversation methods from `McpConfigManager`**

Delete the `get_pipeline_config`, `update_pipeline_config`, `get_conversation_config`, `update_conversation_config` methods from `crates/wisp-mcp/src/config.rs` (lines 96-120).

- [ ] **Step 3: Re-export types from wisp-mcp for backward compat**

In `crates/wisp-mcp/src/lib.rs`, add re-exports so existing `wisp_mcp::PipelineConfig` references still compile during transition:

```rust
pub use wisp_configs::{PipelineConfig, ConversationLoopConfig};
```

This requires adding `wisp-configs` as a dependency of `wisp-mcp`. In `crates/wisp-mcp/Cargo.toml`, add:

```toml
wisp-configs.workspace = true
```

Check root `Cargo.toml` `[workspace.dependencies]` has `wisp-configs` — if not, add:

```toml
wisp-configs = { path = "crates/wisp-configs" }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p wisp-mcp`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/wisp-mcp/ Cargo.toml
git commit -m "refactor(mcp): remove PipelineConfig/ConversationLoopConfig, re-export from wisp-configs"
```

---

## Phase 2: TDD — Pure Utility Modules (wisp-conversation)

New, testable logic that `ConversationLoopConfig` will drive.

### Task 2.1: `context_trim` module — token estimation + sliding window trimming

**Files:**
- Create: `crates/wisp-conversation/src/context_trim.rs`
- Modify: `crates/wisp-conversation/src/lib.rs`

**Interfaces:**
- Produces: `estimate_tokens(messages: &[Message], image_token_cost: u32) -> usize`
- Produces: `trim_context(messages: Vec<Message>, max_tokens: usize, sliding_ratio: f32, image_token_cost: u32) -> Vec<Message>`

**Behaviour:**
- `estimate_tokens`: sums `text.chars() / 4` for each message's text + reasoning, plus `image_token_cost` per image in `images`, plus `tool_calls` JSON string length / 4. Always returns at least 1 per message.
- `trim_context`: if total tokens ≤ `max_tokens`, return messages unchanged. Otherwise, keep the first message (system/root) and a sliding window of recent messages. The window target size = `max_tokens * sliding_ratio`. Drop messages from the front (after the first) until under target. Never drops the last message.

- [ ] **Step 1: Write failing tests for `estimate_tokens`**

Create `crates/wisp-conversation/src/context_trim.rs`:

```rust
use wisp_db::types::{ImageContent, Message, MessageRole};

#[cfg(test)]
mod tests {
    use super::*;

    fn text_message(text: &str) -> Message {
        Message {
            id: "m1".to_string(),
            text: text.to_string(),
            reasoning: None,
            sender: MessageRole::User,
            timestamp: 0,
            tokens: None,
            embedding: None,
            images: None,
            tool_calls: None,
            tool_call_id: None,
            source: Default::default(),
            pal_id: None,
            pal_name: None,
        }
    }

    fn image_message(image_count: usize) -> Message {
        Message {
            id: "m2".to_string(),
            text: String::new(),
            reasoning: None,
            sender: MessageRole::User,
            timestamp: 0,
            tokens: None,
            embedding: None,
            images: Some(
                (0..image_count)
                    .map(|_| ImageContent {
                        content_type: "image_url".to_string(),
                        image_url: wisp_db::types::ImageUrl {
                            url: "data:image/png;base64,abc".to_string(),
                        },
                    })
                    .collect(),
            ),
            tool_calls: None,
            tool_call_id: None,
            source: Default::default(),
            pal_id: None,
            pal_name: None,
        }
    }

    #[test]
    fn estimate_tokens_empty_messages_returns_zero() {
        assert_eq!(estimate_tokens(&[], 85), 0);
    }

    #[test]
    fn estimate_tokens_text_message_uses_chars_div_4() {
        let msg = text_message("hello world!"); // 12 chars
        let tokens = estimate_tokens(&[msg], 85);
        assert_eq!(tokens, 3); // 12 / 4 = 3
    }

    #[test]
    fn estimate_tokens_includes_reasoning() {
        let mut msg = text_message("hi"); // 2 chars → 0 tokens from text
        msg.reasoning = Some("thinking deeply".to_string()); // 15 chars → 3 tokens
        let tokens = estimate_tokens(&[msg], 85);
        assert_eq!(tokens, 4); // ceil(2/4) + ceil(15/4) = 1 + 3 = 4  → actually (2+15)/4 = 4
    }

    #[test]
    fn estimate_tokens_counts_images_at_configured_cost() {
        let msg = image_message(3);
        let tokens = estimate_tokens(&[msg], 85);
        assert_eq!(tokens, 3 * 85);
    }

    #[test]
    fn estimate_tokens_includes_tool_calls_json() {
        let mut msg = text_message("");
        msg.tool_calls = Some(r#"[{"name":"tool","arguments":{}}]"#.to_string()); // 33 chars
        let tokens = estimate_tokens(&[msg], 85);
        assert!(tokens >= 8); // ~33/4 = 8
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wisp-conversation context_trim::tests`
Expected: FAIL — `estimate_tokens` not found

- [ ] **Step 3: Implement `estimate_tokens`**

```rust
pub fn estimate_tokens(messages: &[Message], image_token_cost: u32) -> usize {
    let mut total: usize = 0;
    for msg in messages {
        let text_len = msg.text.chars().count();
        let reasoning_len = msg.reasoning.as_ref().map(|r| r.chars().count()).unwrap_or(0);
        let tool_calls_len = msg.tool_calls.as_ref().map(|tc| tc.chars().count()).unwrap_or(0);
        let char_total = text_len + reasoning_len + tool_calls_len;
        total += char_total.div_ceil(4);

        if let Some(images) = &msg.images {
            total += images.len() * image_token_cost as usize;
        }
    }
    total
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wisp-conversation context_trim::tests`
Expected: PASS

- [ ] **Step 5: Write failing tests for `trim_context`**

Append to the test module:

```rust
    #[test]
    fn trim_context_under_limit_returns_unchanged() {
        let msgs = vec![text_message("short"), text_message("also short")];
        let result = trim_context(msgs.clone(), 10000, 0.7, 85);
        assert_eq!(result.len(), msgs.len());
    }

    #[test]
    fn trim_context_over_limit_keeps_first_and_recent() {
        let msgs: Vec<Message> = (0..20)
            .map(|i| {
                let mut m = text_message(&"x".repeat(1000));
                m.id = format!("m{}", i);
                m
            })
            .collect();
        let result = trim_context(msgs, 500, 0.7, 85);
        assert!(result.len() < 20);
        assert_eq!(result.first().unwrap().id, "m0"); // first always kept
        assert_eq!(result.last().unwrap().id, "m19"); // last always kept
    }

    #[test]
    fn trim_context_never_returns_empty_for_nonempty_input() {
        let msgs = vec![text_message(&"x".repeat(10000))];
        let result = trim_context(msgs, 100, 0.7, 85);
        assert!(!result.is_empty());
    }

    #[test]
    fn trim_context_single_message_always_kept() {
        let msgs = vec![text_message(&"x".repeat(100000))];
        let result = trim_context(msgs, 100, 0.7, 85);
        assert_eq!(result.len(), 1);
    }
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test -p wisp-conversation context_trim::tests`
Expected: FAIL — `trim_context` not found

- [ ] **Step 7: Implement `trim_context`**

```rust
pub fn trim_context(
    messages: Vec<Message>,
    max_tokens: usize,
    sliding_ratio: f32,
    image_token_cost: u32,
) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    let total = estimate_tokens(&messages, image_token_cost);
    if total <= max_tokens {
        return messages;
    }

    let target = (max_tokens as f32 * sliding_ratio) as usize;
    let n = messages.len();

    let first = messages.first().cloned().unwrap();
    let last = messages.last().cloned().unwrap();

    let mut kept: Vec<Message> = Vec::new();
    kept.push(first);

    let middle = &messages[1..n.saturating_sub(1)];
    for msg in middle.iter().rev() {
        kept.push(msg.clone());
        let current_tokens = estimate_tokens(&kept, image_token_cost);
        if current_tokens >= target {
            break;
        }
    }

    kept.reverse();

    if kept.last().map(|m| m.id.as_str()) != Some(last.id.as_str()) {
        kept.push(last);
    }

    kept
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p wisp-conversation context_trim::tests`
Expected: PASS

- [ ] **Step 9: Register module in `lib.rs`**

In `crates/wisp-conversation/src/lib.rs`, add:

```rust
pub mod context_trim;
pub use context_trim::{estimate_tokens, trim_context};
```

- [ ] **Step 10: Commit**

```bash
git add crates/wisp-conversation/src/context_trim.rs crates/wisp-conversation/src/lib.rs
git commit -m "feat(conversation): add context_trim module with token estimation + sliding window"
```

---

### Task 2.2: `retry` module — async retry with fixed-delay backoff

**Files:**
- Create: `crates/wisp-conversation/src/retry.rs`

**Interfaces:**
- Produces: `retry_with_backoff<T, E, F, Fut>(operation: F, attempts: u32, delay_ms: u64) -> Result<T, E>`
  where `F: FnMut() -> Fut`, `Fut: Future<Output = Result<T, E>>`

**Behaviour:**
- Calls `operation()`. On success, returns immediately.
- On error: if attempts remaining > 0, sleep `delay_ms`, retry. Otherwise return last error.
- `attempts = 0` means zero retries (call once, return whatever happens).
- `attempts = 2` means up to 3 total calls (initial + 2 retries).

- [ ] **Step 1: Write failing tests for `retry_with_backoff`**

Create `crates/wisp-conversation/src/retry.rs`:

```rust
use std::future::Future;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn retry_returns_success_on_first_try() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Ok(42));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_succeeds_on_final_attempt() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err("fail".to_string())
                    } else {
                        Ok(99)
                    }
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Ok(99));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_exhausted_returns_last_error() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }
            },
            2,
            1,
        )
        .await;

        assert_eq!(result, Err("always fails".to_string()));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_zero_attempts_calls_once() {
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let _result: Result<i32, String> = retry_with_backoff(
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Err("nope".to_string())
                }
            },
            0,
            1,
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wisp-conversation retry::tests`
Expected: FAIL — `retry_with_backoff` not found

- [ ] **Step 3: Implement `retry_with_backoff`**

```rust
use std::future::Future;
use std::time::Duration;

pub async fn retry_with_backoff<T, E, F, Fut>(
    mut operation: F,
    attempts: u32,
    delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut last_err: Option<E> = None;
    let total_calls = attempts + 1;

    for _ in 0..total_calls {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(last_err.expect("at least one call was made"))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wisp-conversation retry::tests`
Expected: PASS

- [ ] **Step 5: Register module in `lib.rs`**

In `crates/wisp-conversation/src/lib.rs`, add:

```rust
pub mod retry;
pub use retry::retry_with_backoff;
```

- [ ] **Step 6: Commit**

```bash
git add crates/wisp-conversation/src/retry.rs crates/wisp-conversation/src/lib.rs
git commit -m "feat(conversation): add retry_with_backoff async utility"
```

---

## Phase 3: Wire ConversationLoopConfig into Conversation Loop

### Task 3.1: Create `settings_commands.rs` and update command registration

**Files:**
- Create: `src-tauri/src/settings_commands.rs`
- Modify: `src-tauri/src/mcp_commands.rs` (remove 4 commands)
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `settings_commands.rs`**

```rust
use std::sync::Mutex;
use tauri::{AppHandle, State};
use wisp_configs::{ConversationLoopConfig, PipelineConfig};

use crate::types::AppData;

#[tauri::command]
pub async fn settings_get_pipeline_config(
    app_handle: AppHandle,
) -> Result<PipelineConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config_manager.get_pipeline_config())
}

#[tauri::command]
pub async fn settings_update_pipeline_config(
    app_handle: AppHandle,
    config: PipelineConfig,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state
        .config_manager
        .update_pipeline_config(config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn settings_get_conversation_config(
    app_handle: AppHandle,
) -> Result<ConversationLoopConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config_manager.get_conversation_config())
}

#[tauri::command]
pub async fn settings_update_conversation_config(
    app_handle: AppHandle,
    config: ConversationLoopConfig,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state
        .config_manager
        .update_conversation_config(config)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Remove the 4 config commands from `mcp_commands.rs`**

Delete the pipeline config commands (lines ~50-63) and conversation config commands (lines ~65-78).

Remove unused imports: `PipelineConfig`, `ConversationLoopConfig` from the `use wisp_mcp::...` line (if no other code in the file uses them).

- [ ] **Step 3: Update `lib.rs` — swap command registrations**

In `src-tauri/src/lib.rs`:
- Remove lines 159-162 (`mcp_commands::mcp_get_pipeline_config`, etc.)
- Add the new settings commands to the `generate_handler!` macro:

```rust
settings_commands::settings_get_pipeline_config,
settings_commands::settings_update_pipeline_config,
settings_commands::settings_get_conversation_config,
settings_commands::settings_update_conversation_config,
```

Add module declaration:

```rust
mod settings_commands;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings_commands.rs src-tauri/src/mcp_commands.rs src-tauri/src/lib.rs
git commit -m "refactor(commands): move pipeline/conversation config commands to settings_commands"
```

---

### Task 3.2: Wire `ConversationLoopConfig` into `run_conversation_rounds_inner`

**Files:**
- Modify: `src-tauri/src/conversation_commands.rs`

**Interfaces:**
- Consumes: `wisp_configs::ConversationLoopConfig`, `wisp_conversation::{trim_context, retry_with_backoff}`

- [ ] **Step 1: Read config at function start**

At the top of `run_conversation_rounds_inner` (after line 371), add:

```rust
let loop_config = {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state.config_manager.get_conversation_config()
};
let max_rounds = loop_config.max_tool_rounds;
```

- [ ] **Step 2: Parameterize the loop bound**

Change line 376:
```rust
// BEFORE:
for round in 0..10 {

// AFTER:
for round in 0..max_rounds {
```

Change line 594:
```rust
// BEFORE:
if round == 9 {

// AFTER:
if round == max_rounds - 1 {
```

Change line 667 (final fallback error message):
```rust
// BEFORE:
Err(format!("Max tool rounds reached for conversation '{}'", conversation_id))

// AFTER: (unchanged, same message)
Err(format!("Max tool rounds reached for conversation '{}'", conversation_id))
```

- [ ] **Step 3: Apply context trimming to the message path**

After `let path = { ... }` block (line 386), add trimming:

```rust
let path = trim_context(
    path,
    loop_config.max_context_tokens as usize,
    loop_config.context_window_sliding_ratio,
    loop_config.image_token_cost,
);
```

Add import at top of file:
```rust
use wisp_conversation::trim_context;
```

- [ ] **Step 4: Wrap the `backend.stream()` call with retry**

Replace the direct `backend.stream(...).await` call (lines 502-517) with a retry wrapper:

```rust
let outcome = wisp_conversation::retry_with_backoff(
    || {
        let backend = &backend;
        let request = StreamRequest {
            messages: openai_messages.clone(),
            model: model.clone(),
            provider: provider.clone(),
            parameters: resolve_parameters(model_config.as_ref(), parameters.as_ref()),
            callbacks: StreamCallbacks {
                on_content: Arc::new({
                    let assistant_msg_id = assistant_message_id.clone();
                    let sid = stream_id.to_string();
                    let ah = app_handle.clone();
                    move |chunk: &str| {
                        let _ = ah.emit(
                            "conversation_stream_chunk",
                            serde_json::json!({
                                "stream_id": &sid,
                                "message_id": &assistant_msg_id,
                                "chunk": chunk,
                            }),
                        );
                    }
                }),
                on_reasoning: Arc::new({
                    let assistant_msg_id = assistant_message_id.clone();
                    let sid = stream_id.to_string();
                    let ah = app_handle.clone();
                    move |chunk: &str| {
                        let _ = ah.emit(
                            "conversation_stream_reasoning",
                            serde_json::json!({
                                "stream_id": &sid,
                                "message_id": &assistant_msg_id,
                                "chunk": chunk,
                            }),
                        );
                    }
                }),
            },
            cancel: cancel.clone(),
            tools: tool_defs.clone(),
            tool_choice: ToolChoice::Auto,
        };
        async move { backend.stream(request).await }
    },
    loop_config.retry_attempts,
    loop_config.retry_delay_ms,
)
.await
.map_err(|error| format!(
    "Model '{}' failed while streaming conversation '{}': {}",
    model, conversation_id, error
))?;
```

Note: The callbacks are moved inside the closure. Since retry may call it multiple times, each call creates fresh callback closures. The `openai_messages`, `model`, `provider` are cloned per attempt.

- [ ] **Step 5: Gate vision injection with `enable_vision_injection`**

Find `build_openai_messages_with_reasoning` call (line 402-403). If this function injects image content unconditionally, add a parameter or wrap the result. The simplest approach: pass the flag and conditionally strip image content.

If `build_openai_messages_with_reasoning` already takes a vision parameter, use it. Otherwise:

```rust
let mut openai_messages = if loop_config.enable_vision_injection {
    build_openai_messages_with_reasoning(&path, &reasoning_config, supports_native_tools)
} else {
    build_openai_messages_with_reasoning(&path, &reasoning_config, supports_native_tools)
        .into_iter()
        .map(|msg| strip_image_content(msg))
        .collect()
};
```

Where `strip_image_content` converts image_url content blocks to text placeholders. If this function doesn't exist yet, skip the vision gating for now (mark as TODO — the flag is stored and ready for future wiring). **Decision: wire max_tool_rounds + retry + context_trim now; vision gating is a UI-only toggle until `build_openai_messages_with_reasoning` is refactored to accept the flag.**

- [ ] **Step 6: Verify compilation + existing tests**

Run: `cargo check && cargo test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/conversation_commands.rs
git commit -m "feat(conversation): wire ConversationLoopConfig into loop (rounds, retry, context trim)"
```

---

## Phase 4: Frontend — Move Configs out of MCP Store/UI

### Task 4.1: Rename commands + create settings store

**Files:**
- Modify: `src/libs/commands.ts`
- Create: `src/stores/settings.ts`
- Modify: `src/stores/mcp.ts`

- [ ] **Step 1: Rename commands in `commands.ts`**

Replace the 4 functions (lines 268-282):

```typescript
export async function settingsGetPipelineConfig() {
    return invoke<PipelineConfig>('settings_get_pipeline_config', {})
}

export async function settingsUpdatePipelineConfig(config: PipelineConfig) {
    return invoke<void>('settings_update_pipeline_config', { config })
}

export async function settingsGetConversationConfig() {
    return invoke<ConversationLoopConfig>('settings_get_conversation_config', {})
}

export async function settingsUpdateConversationConfig(config: ConversationLoopConfig) {
    return invoke<void>('settings_update_conversation_config', { config })
}
```

- [ ] **Step 2: Create `src/stores/settings.ts`**

```typescript
import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { PipelineConfig, ConversationLoopConfig } from '../libs/types'
import {
    settingsGetPipelineConfig,
    settingsUpdatePipelineConfig,
    settingsGetConversationConfig,
    settingsUpdateConversationConfig,
} from '../libs/commands'

export const useSettingsStore = defineStore('settings', () => {
    const pipelineConfig = ref<PipelineConfig | null>(null)
    const conversationConfig = ref<ConversationLoopConfig | null>(null)
    const isLoading = ref(false)

    const loadPipelineConfig = async () => {
        try {
            pipelineConfig.value = await settingsGetPipelineConfig()
        } catch (e) {
            console.error('Failed to load pipeline config:', e)
        }
    }

    const savePipelineConfig = async (config: PipelineConfig) => {
        isLoading.value = true
        try {
            await settingsUpdatePipelineConfig(config)
            pipelineConfig.value = config
        } finally {
            isLoading.value = false
        }
    }

    const loadConversationConfig = async () => {
        try {
            conversationConfig.value = await settingsGetConversationConfig()
        } catch (e) {
            console.error('Failed to load conversation config:', e)
        }
    }

    const saveConversationConfig = async (config: ConversationLoopConfig) => {
        isLoading.value = true
        try {
            await settingsUpdateConversationConfig(config)
            conversationConfig.value = config
        } finally {
            isLoading.value = false
        }
    }

    const init = async () => {
        await Promise.all([loadPipelineConfig(), loadConversationConfig()])
    }

    return {
        pipelineConfig,
        conversationConfig,
        isLoading,
        init,
        loadPipelineConfig,
        savePipelineConfig,
        loadConversationConfig,
        saveConversationConfig,
    }
})
```

- [ ] **Step 3: Remove config state from `mcp.ts` store**

Remove from `src/stores/mcp.ts`:
- `pipelineConfig` ref (line 43)
- `conversationConfig` ref (line 44)
- `loadPipelineConfig` / `savePipelineConfig` methods (lines 491-507)
- `loadConversationConfig` / `saveConversationConfig` methods (lines 509-526)
- The calls in `init()` (lines 450-451)
- The return object entries (lines 603-604, 613-616)
- The command imports (lines 17-20)
- The `PipelineConfig` / `ConversationLoopConfig` type imports (if unused)

In `processToolResult` (line 244), change to use settings store:

```typescript
import { useSettingsStore } from './settings'
// ...
const settingsStore = useSettingsStore()
// In processToolResult:
const config = settingsStore.pipelineConfig ? { ... } : DEFAULT_PIPELINE_CONFIG
```

- [ ] **Step 4: Update mcp store test**

In `src/__tests__/stores/mcp-store.test.ts`, add mocks for the new settings commands if the test imports them. Remove any pipeline/conversation config assertions.

- [ ] **Step 5: Verify**

Run: `npx vue-tsc --noEmit`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/stores/settings.ts src/stores/mcp.ts src/libs/commands.ts src/__tests__/stores/mcp-store.test.ts
git commit -m "refactor(frontend): move pipeline/conversation config to settings store"
```

---

### Task 4.2: Rename components + move to SettingsView

**Files:**
- Rename: `src/components/McpPipelineConfig.vue` → `src/components/PipelineConfigForm.vue`
- Rename: `src/components/McpConversationConfig.vue` → `src/components/ConversationConfigForm.vue`
- Modify: `src/views/SettingsView.vue`
- Modify: `src/components/McpServerList.vue`

- [ ] **Step 1: Rename the two components**

```bash
git mv src/components/McpPipelineConfig.vue src/components/PipelineConfigForm.vue
git mv src/components/McpConversationConfig.vue src/components/ConversationConfigForm.vue
```

- [ ] **Step 2: Update `PipelineConfigForm.vue` to use settings store**

In `PipelineConfigForm.vue`, change:
```typescript
// BEFORE:
import { useMcpStore } from '../stores/mcp'
const mcpStore = useMcpStore()
// ...
watch(() => mcpStore.pipelineConfig, ...)
await mcpStore.savePipelineConfig(formValue.value)
// ...
:loading="mcpStore.isLoading"

// AFTER:
import { useSettingsStore } from '../stores/settings'
const settingsStore = useSettingsStore()
// ...
watch(() => settingsStore.pipelineConfig, ...)
await settingsStore.savePipelineConfig(formValue.value)
// ...
:loading="settingsStore.isLoading"
```

- [ ] **Step 3: Update `ConversationConfigForm.vue` to use settings store**

Same pattern as Step 2, but for `conversationConfig` / `saveConversationConfig`.

- [ ] **Step 4: Add both forms to `SettingsView.vue`**

In `src/views/SettingsView.vue`, add after the Chore LLM card:

```vue
<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { NCard, NForm, NFormItem, NSelect, NButton, NSpace, useMessage } from 'naive-ui'
import { useChoreLlm } from '../composables/useChoreLlm'
import { useSettingsStore } from '../stores/settings'
import PipelineConfigForm from '../components/PipelineConfigForm.vue'
import ConversationConfigForm from '../components/ConversationConfigForm.vue'

const message = useMessage()
const { choreLlm, providerOptions, modelOptions, save, clear } = useChoreLlm()
const settingsStore = useSettingsStore()

onMounted(() => {
    settingsStore.init()
})

// ... existing computed/handlers ...
</script>

<template>
  <div class="settings-view">
    <n-card title="Chore LLM" size="small">
      <!-- existing content -->
    </n-card>

    <n-card title="Pipeline Config" size="small" style="margin-top: 16px">
      <template #header-extra>
        <span class="hint">Media processing for tool results</span>
      </template>
      <PipelineConfigForm />
    </n-card>

    <n-card title="Conversation Config" size="small" style="margin-top: 16px">
      <template #header-extra>
        <span class="hint">Conversation engine loop parameters</span>
      </template>
      <ConversationConfigForm />
    </n-card>
  </div>
</template>
```

- [ ] **Step 5: Remove config buttons + drawer from `McpServerList.vue`**

In `src/components/McpServerList.vue`:
- Remove imports of `McpPipelineConfig` / `McpConversationConfig` (lines 15-16)
- Remove the pipeline/conversation config buttons (lines ~138-153)
- Remove the config drawer (lines 166-177)
- Remove `showConfigDrawer` ref

- [ ] **Step 6: Verify**

Run: `npx vue-tsc --noEmit && npx vitest run`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/components/ src/views/SettingsView.vue
git commit -m "refactor(frontend): move config forms to SettingsView, rename components"
```

---

## Phase 5: Final Verification

### Task 5.1: Full build + test sweep

- [ ] **Step 1: Rust check + test**

Run: `cargo check && cargo test`
Expected: PASS — zero errors, all tests green

- [ ] **Step 2: Frontend check + test**

Run: `npx vue-tsc --noEmit && npx vitest run`
Expected: PASS

- [ ] **Step 3: Manual smoke test (if possible)**

- Open Settings view → verify Pipeline Config and Conversation Config forms render and save
- Open MCP view → verify config buttons are gone
- Start a conversation → verify tool loop works (max_tool_rounds respected)

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: final cleanup for settings config extraction"
```
