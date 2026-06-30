# A un-official full version of Wisp.

An experimental LLM chatting interface which design for MCP agent.

Full version of Wisp. With plenty of BUGs :(

## Features
- all features of Wisp
- support for MCP (stdio + http/sse transports)
- pipeline support for long Base 64 picture or other big things
- context management (token-based sliding window, ~120k threshold, ~84k target)
- multi-tool call rendering in conversation (nested json safe, error state aware)
- tool call round limit to prevent infinite recursion (max 10 rounds)
- windows acrylic / macos vibrancy native window effect

## Recent fixes
- mcp http: start sse listener before `initialize` so the response can be received
- mcp http: sse buffer drain instead of clone-per-line (was o(n^2))
- mcp stdio (windows): use `raw_arg` for the command path so spaces in paths no longer break spawning
- mcp stdio: clean pending map on write failure / timeout / channel close (was leaking tx)
- db: propagate `update_parent` errors instead of silently dropping them
- db: log message-list errors when deleting a conversation instead of swallowing them
- db: use `unwrap_or_default` for system time (no panic on clock rollback)
- api: accept assistant messages whose `content` is null (openai allows this with tool_calls)
- commands: surface `update_reasoning` errors instead of `let _ =`
- mcp commands: replace `.expect()` on app data dir with proper `Result` return

# The old version of readme
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
