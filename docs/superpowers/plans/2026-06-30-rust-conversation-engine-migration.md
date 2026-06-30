# Rust Conversation Engine Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move canonical chat state, branch selection, context construction, model streaming, and MCP tool-call loops from Vue/Pinia into Rust while preserving all existing chat features.

**Architecture:** Add a Rust `conversation` module that owns the conversation state machine and exposes Tauri commands/events. Keep existing SQLite conversations/messages/threads tables initially for compatibility, then add typed metadata helpers around existing `images` and `tool_calls` JSON fields. The UI becomes a thin client: it sends user intents to Rust, subscribes to stream/message events, and renders returned message snapshots.

**Tech Stack:** Rust 2021, Tauri 2 commands/events, SQLite via `rusqlite`, async-openai, existing MCP stdio/http managers, Vue 3 + Pinia as UI cache only, Vitest for front-end adapter tests, Rust unit tests for engine logic.

## Global Constraints

- Preserve all existing user-visible chat features: streaming text, reasoning text, images, Markdown rendering, KaTeX/Mermaid rendering, MCP server/tool selection, tool-call rendering, tool result bubbles, max 10 tool rounds, regenerate, resend/edit, derive/branch, sibling navigation, deletion, conversation list/load/update/delete.
- Do not remove existing commands until the new Rust-backed path is wired and tested.
- Rust is the canonical owner of conversation state, branch path selection, model payload construction, tool execution loop, and DB persistence.
- Frontend may keep ephemeral UI state only: input text, selected model/provider/character/tools, scroll position, modals, branch navigation selection cache.
- All migration tasks must include tests before implementation.
- Avoid schema rewrites in the first migration phase; use existing `messages`, `threads`, and `conversations` tables, including `images` and `tool_calls` JSON columns.
- OpenAI-compatible API payloads must preserve native `assistant.tool_calls` and `tool.tool_call_id` semantics.
- Existing project build currently has unrelated TypeScript errors in `src/components/McpServerDetails.vue`; do not hide or conflate them with this migration.

---

## File Structure

### Rust files to create

- `src-tauri/src/conversation/mod.rs` — module exports.
- `src-tauri/src/conversation/types.rs` — request/response/event types for Rust engine commands.
- `src-tauri/src/conversation/payload.rs` — pure conversion from selected message path to OpenAI request messages.
- `src-tauri/src/conversation/tool_parser.rs` — parse JSON fenced or inline `tool_call` / tool-call arrays from assistant text.
- `src-tauri/src/conversation/tool_executor.rs` — execute structured tool calls through existing stdio/http managers.
- `src-tauri/src/conversation/engine.rs` — orchestration: persist user message, create/update assistant draft, execute tools, append tool result messages, continue rounds.
- `src-tauri/src/conversation/commands.rs` — Tauri command surface and event emission.
- `src-tauri/src/conversation/tests.rs` — Rust unit tests for pure engine components.

### Rust files to modify

- `src-tauri/src/lib.rs` — register `conversation` module and new commands.
- `src-tauri/src/types.rs` — add `conversation_engine` shared state if needed.
- `src-tauri/src/db/types.rs` — add typed serde structs for `tool_calls` JSON while keeping DB column as `Option<String>`.
- `src-tauri/src/db/chat.rs` — add path-selection helpers that return active branch messages by decisions or leaf.
- `src-tauri/src/api.rs` — split streaming implementation into reusable `stream_openai_chat` function accepting typed request messages.
- `src-tauri/src/commands.rs` — keep legacy commands; mark old `ask_openai_stream` as compatibility path.

### Frontend files to modify

- `src/libs/types.ts` — add Rust-backed request/event types matching `conversation/types.rs`.
- `src/libs/commands.ts` — add wrappers for new Rust conversation commands.
- `src/stores/chat.ts` — remove canonical state-machine responsibilities; become UI cache around Rust snapshots/events.
- `src/components/Chat.vue` — call new store actions; preserve current UI behavior.
- `src/components/MessageBubble.vue` — keep existing rendering; ensure tool calls from Rust typed JSON render the same.
- `src/composables/useOpenAI.ts` — stop being used by chat send/regenerate path after migration; keep for compatibility until removal.

---

## Task 1: Rust message payload builder foundation

**Files:**
- Create: `src-tauri/src/conversation/mod.rs`
- Create: `src-tauri/src/conversation/types.rs`
- Create: `src-tauri/src/conversation/payload.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: Rust unit tests inside `src-tauri/src/conversation/payload.rs`

**Interfaces:**
- Consumes: existing `crate::db::types::Message`, `MessageRole`, `ImageContent`.
- Produces:
  - `ConversationToolCall { id: String, name: String, arguments: serde_json::Value, result: Option<ConversationToolResult> }`
  - `ConversationToolResult { content: Vec<ConversationToolContent>, is_error: bool }`
  - `ConversationToolContent` enum matching current TypeScript `ToolCallContent`.
  - `build_openai_messages(messages: &[Message]) -> Result<Vec<async_openai::types::ChatCompletionRequestMessage>, ConversationPayloadError>`.

- [ ] **Step 1: Write failing tests for OpenAI payload conversion**

Add tests covering:

```rust
#[test]
fn builds_native_tool_history_for_second_round() {
    let assistant_tool_calls = serde_json::json!([
        {
            "id": "call_1",
            "name": "server:search",
            "arguments": { "query": "rust tauri" },
            "result": {
                "content": [{ "type": "text", "text": "result text" }],
                "isError": false
            }
        }
    ]).to_string();

    let messages = vec![
        message("user", "Search docs", None, None),
        message("bot", "", Some(assistant_tool_calls), None),
        message("tool", "result text", None, Some("call_1")),
    ];

    let converted = build_openai_messages(&messages).expect("payload builds");
    assert_eq!(converted.len(), 3);
    assert!(matches!(converted[0], ChatCompletionRequestMessage::User(_)));
    match &converted[1] {
        ChatCompletionRequestMessage::Assistant(msg) => {
            assert!(msg.content.is_none());
            let calls = msg.tool_calls.as_ref().expect("tool calls present");
            assert_eq!(calls[0].id, "call_1");
            assert_eq!(calls[0].function.name, "server:search");
            assert_eq!(calls[0].function.arguments, r#"{"query":"rust tauri"}"#);
        }
        other => panic!("expected assistant, got {other:?}"),
    }
    match &converted[2] {
        ChatCompletionRequestMessage::Tool(msg) => {
            assert_eq!(msg.tool_call_id, "call_1");
        }
        other => panic!("expected tool, got {other:?}"),
    }
}
```

Also add tests for:
- user image messages become multimodal user messages.
- normal assistant text remains assistant text.
- tool message without tool call id returns an error instead of silently becoming a user message.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml conversation::payload`

Expected: FAIL because `conversation` module and `build_openai_messages` do not exist.

- [ ] **Step 3: Implement `conversation/types.rs`**

Define serializable Rust types mirroring current frontend tool call JSON. Include serde aliases for current camelCase fields:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ConversationToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub result: Option<ConversationToolResult>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ConversationToolResult {
    #[serde(default)]
    pub content: Vec<ConversationToolContent>,
    #[serde(default, alias = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ConversationToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, #[serde(rename = "mimeType")] mime_type: String },
    #[serde(rename = "resource")]
    Resource { uri: String, #[serde(default, rename = "mimeType")] mime_type: Option<String>, #[serde(default)] text: Option<String>, #[serde(default)] blob: Option<String> },
}
```

- [ ] **Step 4: Implement `conversation/payload.rs`**

Implement:

```rust
pub fn build_openai_messages(messages: &[Message]) -> Result<Vec<ChatCompletionRequestMessage>, ConversationPayloadError>
```

Rules:
- `MessageRole::User` with images uses `ChatCompletionRequestUserMessageContent::Array`.
- `MessageRole::User` without images uses `Text`.
- `MessageRole::Assistant` parses `message.tool_calls` as `Vec<ConversationToolCall>` when present and maps to OpenAI `tool_calls`.
- Assistant content is `None` only when text is empty and tool calls exist.
- `MessageRole::Tool` must have a tool-call id; initially use the first parsed tool call id from the previous assistant if DB does not yet have a dedicated column. If it cannot be inferred, return `ConversationPayloadError::MissingToolCallId`.
- `MessageRole::System` maps to system.

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml conversation::payload`

Expected: PASS for payload tests.

---

## Task 2: Rust branch path selection

**Files:**
- Modify: `src-tauri/src/db/chat.rs`
- Modify: `src-tauri/src/db/threads.rs`
- Test: Rust unit tests in `src-tauri/src/db/chat.rs` or a new `src-tauri/src/db/tests.rs`

**Interfaces:**
- Consumes: existing `messages`, `threads`, and `conversations` tables.
- Produces:
  - `Chat::get_message_path_to(conversation_id: &str, leaf_message_id: &str) -> Result<Vec<Message>, ChatError>`
  - `Chat::get_default_leaf(conversation_id: &str) -> Result<Option<String>, ChatError>`

- [ ] Write failing tests for branch path selection with root → assistant A/B branches.
- [ ] Implement parent traversal from leaf to root using `Threads` parent relation.
- [ ] Reverse path before returning.
- [ ] Verify path includes exactly selected branch, not sibling branch messages.
- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml db::`.

---

## Task 3: Rust tool-call parser

**Files:**
- Create: `src-tauri/src/conversation/tool_parser.rs`
- Modify: `src-tauri/src/conversation/mod.rs`
- Test: Rust unit tests in `tool_parser.rs`

**Interfaces:**
- Consumes: assistant raw text.
- Produces:
  - `parse_tool_calls(text: &str) -> ParsedToolCalls`
  - `ParsedToolCalls { clean_text: String, calls: Vec<ConversationToolCall> }`

- [ ] Write failing tests for fenced JSON object with `tool_call`.
- [ ] Write failing tests for multiple tool calls in an array.
- [ ] Write failing tests for nested JSON arguments.
- [ ] Implement parser using brace-balanced extraction instead of regex-only parsing.
- [ ] Verify clean text removes only parsed tool call blocks.

---

## Task 4: Reusable Rust OpenAI streaming adapter

**Files:**
- Modify: `src-tauri/src/api.rs`
- Test: Rust unit tests for request construction where possible; manual compile check for stream function.

**Interfaces:**
- Consumes: `Vec<ChatCompletionRequestMessage>`, provider/model/parameters.
- Produces:
  - `stream_openai_chat(app_handle, messages, model, provider, parameters, event_prefix) -> Result<OpenAiStreamOutcome, Box<dyn Error>>`

- [ ] Extract current `ask_openai_stream` logic without changing legacy command behavior.
- [ ] Keep legacy `ask_openai_stream` calling the extracted function through `convert_messages`.
- [ ] Add event names for engine path: `conversation_stream_chunk`, `conversation_stream_reasoning`, `conversation_round_finished`, `conversation_error`.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml`.

---

## Task 5: Rust tool executor over existing MCP managers

**Files:**
- Create: `src-tauri/src/conversation/tool_executor.rs`
- Modify: `src-tauri/src/conversation/mod.rs`
- Test: Unit tests with a trait-backed fake executor.

**Interfaces:**
- Consumes: `ConversationToolCall.name` in current qualified format `server_id:tool_name`.
- Produces:
  - `execute_tool_call(app_data, call) -> Result<ConversationToolCall, ConversationEngineError>`

- [ ] Write failing test that `server_id:tool_name` splits at first `:` only.
- [ ] Write failing test that invalid names return typed errors.
- [ ] Implement transport dispatch: stdio uses `McpStdioManager`, http/sse uses `McpHttpManager` based on server config.
- [ ] Preserve processed content format compatible with current `ToolCallItem.result.content`.

---

## Task 6: Rust conversation engine send loop

**Files:**
- Create: `src-tauri/src/conversation/engine.rs`
- Create: `src-tauri/src/conversation/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/types.rs`
- Test: Unit tests for engine with fake LLM and fake tool executor.

**Interfaces:**
- Consumes command request:

```rust
pub struct ConversationSendRequest {
    pub conversation_id: String,
    pub parent_message_id: Option<String>,
    pub text: String,
    pub images: Option<Vec<ImageContent>>,
    pub model: String,
    pub provider: Provider,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    pub character_system_prompt: Option<String>,
    pub enabled_mcp_tools: Vec<String>,
}
```

- Produces events and persisted messages.

- [ ] Test simple text round: user + assistant persisted, stream chunks emitted.
- [ ] Test tool round: assistant with tool calls persisted, tool message persisted, second assistant receives native tool history.
- [ ] Test max 10 tool rounds stops with warning/error event and no infinite loop.
- [ ] Test tool error persists a tool result with `isError: true` and continues one more round.
- [ ] Implement command `conversation_send_message`.
- [ ] Register command in `lib.rs`.

---

## Task 7: Rust regenerate / derive / edit semantics

**Files:**
- Modify: `src-tauri/src/conversation/engine.rs`
- Modify: `src-tauri/src/conversation/commands.rs`
- Test: Rust engine tests with fake LLM.

**Interfaces:**
- Produces commands:
  - `conversation_regenerate_message(message_id, insert_guidance, model, provider, ...)`
  - `conversation_edit_and_regenerate(message_id, new_text, model, provider, ...)`
  - `conversation_derive_message(replaced_message_id, new_text, model, provider, ...)`

- [ ] Test regenerate creates sibling assistant under same parent and does not overwrite original branch.
- [ ] Test edit-and-regenerate updates user text then creates new assistant branch.
- [ ] Test derive creates sibling user message under old parent.
- [ ] Preserve `INTERFACE_REGENERATE_INSERT` equivalent in Rust prompt construction.

---

## Task 8: Frontend command wrappers and event bridge

**Files:**
- Modify: `src/libs/types.ts`
- Modify: `src/libs/commands.ts`
- Create: `src/composables/useConversationEvents.ts`
- Test: Vitest tests for event reducer if existing test setup supports it.

**Interfaces:**
- Consumes Rust events.
- Produces UI cache updates:
  - message created
  - message updated text/reasoning/toolCalls
  - message finished
  - error

- [ ] Add TypeScript types matching Rust request/event payloads.
- [ ] Add command wrappers for new Rust commands.
- [ ] Implement event subscription composable returning unlisten cleanup functions.
- [ ] Test reducer updates a message map from stream events.

---

## Task 9: Convert `src/stores/chat.ts` into UI cache

**Files:**
- Modify: `src/stores/chat.ts`
- Test: Vitest tests for store behavior if Pinia testing harness is available; otherwise targeted TypeScript compile plus manual event reducer tests.

**Interfaces:**
- Consumes new command wrappers and event bridge.
- Preserves public store API used by `Chat.vue` and `MessageBubble.vue`:
  - `sendMessage`
  - `regenerateMessage`
  - `deriveMessage`
  - `deleteMessage`
  - `loadConversation`
  - `displayedMessage`
  - `threadTreeDecisions`
  - `changeThreadTreeDecision`

- [ ] Replace frontend tool loop with `conversation_send_message`.
- [ ] Keep display tree and sibling navigation behavior from Rust-loaded messages/threads.
- [ ] Ensure UI still renders toolCalls on assistant messages and tool result bubbles.
- [ ] Remove direct `useOpenAI.streamResponse` from send/regenerate paths.

---

## Task 10: Compatibility cleanup and validation

**Files:**
- Modify: `src/composables/useOpenAI.ts` only if no longer used by chat path.
- Modify: docs if needed.
- Test: full available validation.

- [ ] Run `cargo test --manifest-path src-tauri/Cargo.toml`.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- [ ] Run `pnpm test`.
- [ ] Run `pnpm build`; if existing `McpServerDetails.vue` errors remain, report them as pre-existing unless this plan changed that file.
- [ ] Manually test: simple chat, MCP tool chat, image chat, regenerate, edit/resend, derive branch, sibling navigation, delete message, load conversation.

---

## Self-Review

- Spec coverage: The plan covers Rust-owned state machine, branch path selection, OpenAI payload construction, MCP tool loop, regenerate/edit/derive, frontend thin cache, and validation.
- Placeholder scan: No TBD/TODO placeholders remain; each task names concrete files and interfaces.
- Type consistency: `ConversationToolCall`, `ConversationToolResult`, and event/command boundaries are used consistently across tasks.
