# Wisp

An unofficial, expanded fork of [Wisp](#the-original-wisp) — an experimental LLM chat interface rebuilt around MCP agents, multi-pal orchestration, native tool calling, and a pluggable backend.

> Full version of Wisp. With plenty of BUGs :(

## What this fork adds

This fork picks up where the original `mcp` integration landed and grows Wisp into a workspace-structured agent runtime. The work is split across standalone Rust crates under `crates/` and a Vue/Tauri frontend.

### MCP (Model Context Protocol)
- First-class MCP support with **stdio** and **http/sse** transports
- MCP handling moved entirely into Rust (`wisp-mcp`) behind a custom tool-call protocol
- Per-server **env vars** and **working directory** for `stdio` clients
- Reactive tool loading on server connect (no more one-shot poll racing the backend)
- Cached + eager LLM display-name enrichment shown in the tools table
- Robustness fixes: SSE listener started before `initialize`, SSE buffer drain (was O(n²)), `raw_arg` on Windows so spaces in paths don't break spawning, pending-map cleanup on write failure/timeout

### Multi-Pal orchestration
- **Orchestrator + director** modules route a single message to one or more "pals"
- `@`-mention autocomplete (Naive UI `NMention`) with `target_pal_ids` tracking
- Pal identity + source badge on message bubbles, `ChatPalBar` of active members
- Per-conversation **default responder** and `role_bio` field
- Multi-pal streaming: draft messages inserted up front, chunks streamed via events

### LLM backend rewrite
- New `wisp-llm` crate with a `LlmBackend` trait + backend factory
- `OpenAiCompatBackend` on raw `reqwest` + SSE (replaces `async-openai`)
- Per-backend **reasoning policy** (`Never` / `Always` / `ToolTurnsOnly`):
  - DeepSeek interleaved thinking (`thinking:{type:enabled}` injected into body)
  - OpenAiCompat `reasoning_details` pass-back for vLLM / MiniMax / Kimi
- Native tool calling with **delta merging** + automatic **text-protocol fallback** based on each model's `ToolUse` capability
- `tool_call_id` on messages + DB migration, correct OpenAI `tool_calls` reconstruction
- SSE hardening: trailing-newline stripping, explicit `:` comment-line handling, `[DONE]` detection

### Streaming control
- `stream_id` for concurrent-stream disambiguation
- `AbortRegistry` + `conversation_abort` command + frontend **Stop** button
- `conversation_stream_reset` emitted before each retry so the UI clears stale chunks

### Software / native tools
- New `wisp-software-tools` crate: `NativeTool` trait + `SoftwareToolRegistry`
- `js_exec` — sandboxed **QuickJS** (`rquickjs`) execution with try/catch error capture
- `config_read` / `config_write` for providers, characters, default responder, chore LLM, pipeline & conversation config
- Tool failures are surfaced to the LLM as error results instead of aborting the loop
- Server-side tool-result markdown formatting (`formatToolCallMarkdown`)

### Conversation loop & context
- `ConversationLoopConfig` wired into the loop: `max_tool_rounds` (clamped ≥ 1), sliding-window `sliding_ratio`, retry attempts + delay
- `context_trim` module: token estimation + sliding-window trimming driven by the model's intrinsic `context_window`
- `retry_with_backoff` utility + stream reset on retry
- Pipeline config for large payloads (e.g. base64 images)

### Config & settings
- `wisp-configs` with `ConfigManager` (Arc-shared with native tools)
- `PipelineConfig` + `ConversationLoopConfig` re-exported centrally
- Settings **normalized on load** (validate / clamp / default each field), defaults persisted on first load
- Debounced **autosave** with broadcast-aware dedup to prevent feedback loops
- Dedicated Settings view + Pinia store; chore-LLM selector for background display-name generation

### Persistence
- Message **thread tree** persisted in DB; `thread_decisions` (JSON) keeps branch selection across reloads
- `tool_call_id` column added via migration

### Workspace architecture
Inline `src-tauri/src/` modules migrated to crates:
`wisp-common`, `wisp-db`, `wisp-configs`, `wisp-keyring`, `wisp-mcp`, `wisp-llm`, `wisp-conversation`, `wisp-tool-registry`, `wisp-software-tools`.

### Platform
- Windows acrylic / macOS vibrancy native window effects

## Status

Work in progress. Expect rough edges — see the commit history for the full list of fixes.

---

# The original Wisp

## Wisp

An experimental LLM chatting interface designed to be fast, minimal yet powerful.

---

Work in Progress...

## Features

- Real-time chat interface with OpenAI integration
- Markdown, KaTeX and Mermaid rendering support
- Responsive design for various screen sizes
- Tauri-powered desktop application
- State management with Pinia
- Modern UI with Naive UI components
