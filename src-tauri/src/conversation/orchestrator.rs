use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Mutex;

use serde_json::Value;

use tauri::Manager;

use crate::configs::character::Character;
use crate::configs::provider::Provider;
use crate::conversation::director::{assemble_director_prompt, parse_director_response};
use crate::conversation::types::MessageSource;
use crate::db::types::Message;
use crate::types::AppData;

#[derive(Debug, Clone)]
pub struct PalReply {
    pub message_id: String,
    pub pal_id: String,
    pub pal_name: String,
    pub text: String,
    pub source: MessageSource,
}

/// Execute a multi-pal round:
/// 1. Sort target_pal_ids by input order
/// 2. For each pal, build context (previous messages + previous pal replies), call LLM
/// 3. After all pal replies, run director check
/// 4. Return all message IDs created
pub async fn orchestrate_multi_pal_round<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    conversation_id: &str,
    user_message_id: &str,
    target_pal_ids: Vec<String>,
    all_characters: &[Character],
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<Vec<PalReply>, String> {
    // Load conversation history from DB so all pals see prior messages
    let conversation_history = {
        let state_mutex = app_handle.state::<Mutex<AppData>>();
        let mut state = state_mutex
            .lock()
            .map_err(|e| format!("Failed to acquire app state: {}", e))?;
        state
            .chat
            .get_message_path_to(conversation_id, user_message_id)
            .map_err(|e| format!("Failed to get message path: {}", e))?
    };

    let mut replies = Vec::new();
    let mut unlocked_pal_ids: HashSet<String> = HashSet::new();

    for pal_id in &target_pal_ids {
        unlocked_pal_ids.insert(pal_id.clone());

        let pal = all_characters
            .iter()
            .find(|c| c.id == *pal_id)
            .ok_or_else(|| format!("Pal not found: {}", pal_id))?;

        // Build context: existing conversation + previous pal replies in this round
        let context = build_context_for_pal(pal, &replies, &conversation_history)?;

        // Call LLM with pal's model, prompt, params
        let reply_text =
            call_llm_with_pal_config(app_handle, &context, pal, provider, parameters).await?;

        // Store reply
        let reply = PalReply {
            message_id: format!("pal-{}-{}", pal_id, user_message_id),
            pal_id: pal_id.clone(),
            pal_name: pal.name.clone(),
            text: reply_text,
            source: MessageSource::UserPrompted,
        };
        replies.push(reply);
    }

    // Director check (after all pal replies)
    let director_reply = run_director_check(
        app_handle,
        conversation_id,
        user_message_id,
        &replies,
        all_characters,
        &unlocked_pal_ids,
        provider,
        parameters,
    )
    .await?;
    if let Some(reply) = director_reply {
        replies.push(reply);
    }

    Ok(replies)
}

/// Build the message context for a specific pal, including:
/// 1. The conversation history from DB (user messages, assistant replies, etc.)
/// 2. The pal's system prompt as a system message
/// 3. Previous pal replies in this round as assistant messages
pub fn build_context_for_pal(
    pal: &Character,
    previous_replies: &[PalReply],
    conversation_history: &[Message],
) -> Result<Vec<Message>, String> {
    let mut context = Vec::new();

    // 1. Prepend pal's system_prompt as a system message
    if !pal.system_prompt.is_empty() {
        context.push(Message {
            id: format!("{}-system", pal.id),
            text: pal.system_prompt.clone(),
            reasoning: None,
            sender: crate::db::types::MessageRole::System,
            timestamp: 0,
            tokens: None,
            embedding: None,
            images: None,
            tool_calls: None,
            source: MessageSource::UserPrompted,
            pal_id: None,
            pal_name: None,
        });
    }

    // 2. Append conversation history from DB
    for msg in conversation_history {
        context.push(msg.clone());
    }

    // 3. Append previous pal replies in this round as assistant messages
    for reply in previous_replies {
        context.push(Message {
            id: reply.message_id.clone(),
            text: reply.text.clone(),
            reasoning: None,
            sender: crate::db::types::MessageRole::Assistant,
            timestamp: 0,
            tokens: None,
            embedding: None,
            images: None,
            tool_calls: None,
            source: reply.source.clone(),
            pal_id: Some(reply.pal_id.clone()),
            pal_name: Some(reply.pal_name.clone()),
        });
    }

    Ok(context)
}

/// After all pal replies, run the director check to see if another
/// @mentioned pal should be invited into the conversation.
pub async fn run_director_check<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    conversation_id: &str,
    user_message_id: &str,
    pal_replies: &[PalReply],
    all_characters: &[Character],
    unlocked_pal_ids: &HashSet<String>,
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<Option<PalReply>, String> {
    // 1. Filter unlocked pal IDs to actual Character objects
    let available_pals: Vec<Character> = all_characters
        .iter()
        .filter(|c| unlocked_pal_ids.contains(&c.id))
        .cloned()
        .collect();

    if available_pals.is_empty() {
        return Ok(None);
    }

    // 2. Get recent conversation text (last 10 messages)
    const MAX_DIRECTOR_CONTEXT: usize = 10;
    let recent_messages: Vec<String> = pal_replies
        .iter()
        .rev()
        .take(MAX_DIRECTOR_CONTEXT)
        .rev()
        .map(|r| format!("{}: {}", r.pal_name, r.text))
        .collect();

    // 3. Assemble director prompt
    let prompt = assemble_director_prompt(&recent_messages, &available_pals);

    // 4. Call LLM
    let messages = vec![crate::db::types::Message {
        id: "director-prompt".to_string(),
        text: prompt,
        reasoning: None,
        sender: crate::db::types::MessageRole::User,
        timestamp: 0,
        tokens: None,
        embedding: None,
        images: None,
        tool_calls: None,
        source: MessageSource::UserPrompted,
        pal_id: None,
        pal_name: None,
    }];

    let outcome = call_llm_with_pal_config(
        app_handle,
        &messages,
        // Use first available pal's config for the director LLM call
        &available_pals[0],
        provider,
        parameters,
    )
    .await?;

    // 5. Parse JSON response
    let decision = parse_director_response(&outcome);

    // 6. If invoke, create PalReply with source: Directed
    if decision.should_invoke {
        if let Some(pal_id) = decision.target_pal_id {
            let pal = all_characters
                .iter()
                .find(|c| c.id == pal_id)
                .ok_or_else(|| format!("Director invoked unknown pal: {}", pal_id))?;

            // Load conversation history from DB
            let conversation_history = {
                let state_mutex = app_handle.state::<Mutex<AppData>>();
                let mut state = state_mutex
                    .lock()
                    .map_err(|e| format!("Failed to acquire app state: {}", e))?;
                state
                    .chat
                    .get_message_path_to(conversation_id, user_message_id)
                    .map_err(|e| format!("Failed to get message path: {}", e))?
            };

            // Existing conversation history + previous pal replies as context
            let context = build_context_for_pal(pal, pal_replies, &conversation_history)?;

            let reply_text =
                call_llm_with_pal_config(app_handle, &context, pal, provider, parameters).await?;

            return Ok(Some(PalReply {
                message_id: format!("directed-{}-{}", pal_id, user_message_id),
                pal_id: pal.id.clone(),
                pal_name: pal.name.clone(),
                text: reply_text,
                source: MessageSource::Directed,
            }));
        }
    }

    // 7. Return None if action: none
    Ok(None)
}

/// Call the LLM with a character's configuration.
async fn call_llm_with_pal_config<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    messages: &[Message],
    pal: &Character,
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<String, String> {
    use async_openai::types::ChatCompletionRequestMessage;

    use crate::api::{stream_openai_messages, OpenAiStreamEvents};

    // Convert Messages to ChatCompletionRequestMessage
    let api_messages: Vec<ChatCompletionRequestMessage> =
        crate::conversation::payload::build_openai_messages(messages);

    let outcome = stream_openai_messages(
        app_handle.clone(),
        api_messages,
        pal.model_id.clone(),
        provider.clone(),
        parameters.cloned(),
        OpenAiStreamEvents {
            content_chunk: "",
            reasoning_chunk: "",
            message_id: None,
        },
    )
    .await
    .map_err(|e: Box<dyn Error>| format!("LLM call failed: {}", e))?;

    Ok(outcome.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::cache::DiagramCache;
    use crate::configs::ConfigManager;
    use crate::configs::character::Character;
    use crate::db::create_memory_pool;
    use crate::db::chat::Chat;
    use crate::db::types::MessageRole;
    use crate::key_manager::KeyManager;
    use crate::mcp::commands::McpConfigManager;
    use crate::mcp_http::McpHttpManager;
    use crate::mcp_stdio::McpStdioManager;
    use crate::tool_registry::ToolRegistry;

    fn test_character(id: &str, name: &str, system_prompt: &str, role_bio: &str) -> Character {
        Character {
            id: id.to_string(),
            name: name.to_string(),
            alias: None,
            avatar: None,
            description: String::new(),
            system_prompt: system_prompt.to_string(),
            parameters: Vec::new(),
            model_id: "gpt-4".to_string(),
            created_at: 0,
            updated_at: 0,
            role_bio: role_bio.to_string(),
        }
    }

    fn test_reply(pal_id: &str, pal_name: &str, text: &str, source: MessageSource) -> PalReply {
        PalReply {
            message_id: format!("msg-{}", pal_id),
            pal_id: pal_id.to_string(),
            pal_name: pal_name.to_string(),
            text: text.to_string(),
            source,
        }
    }

    fn setup_app() -> (tauri::AppHandle<tauri::test::MockRuntime>, String) {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();

        let conversation_id = "test-conv".to_string();
        let mut chat = Chat::new_with_pool(create_memory_pool()).expect("chat");
        chat.create_conversation(&conversation_id, "Test", "desc")
            .expect("conversation created");

        let diagram_cache = DiagramCache::new().expect("diagram cache");
        let key_manager = KeyManager::new("test-wisp".to_string());
        let config_manager =
            ConfigManager::new(&handle).expect("config manager");
        let mcp_config_manager =
            McpConfigManager::new(&handle).expect("mcp config");
        let stdio_manager = Arc::new(McpStdioManager::new());
        let http_manager = Arc::new(McpHttpManager::new());
        let tool_registry = Arc::new(ToolRegistry::new(
            Arc::clone(&stdio_manager),
            Arc::clone(&http_manager),
        ));

        let app_data = AppData {
            chat,
            diagram_cache,
            key_manager,
            config_manager,
            mcp_config_manager,
            mcp_stdio_manager: stdio_manager,
            mcp_http_manager: http_manager,
            tool_registry,
            unlocked_pals: HashMap::new(),
        };

        handle.manage(Mutex::new(app_data));

        (handle, conversation_id)
    }

    // ── build_context_for_pal tests ────────────────────────────

    #[test]
    fn build_context_includes_system_prompt() {
        let pal = test_character("c1", "Code Reviewer", "You are a code reviewer.", "Reviews code");
        let replies = vec![];

        let context = build_context_for_pal(&pal, &replies, &[]).unwrap();

        assert_eq!(context.len(), 1);
        assert_eq!(context[0].sender, MessageRole::System);
        assert_eq!(context[0].text, "You are a code reviewer.");
    }

    #[test]
    fn build_context_includes_previous_pal_replies() {
        let pal = test_character("c1", "Code Reviewer", "You are a code reviewer.", "Reviews code");
        let replies = vec![
            test_reply("c2", "PM", "Let's focus on features.", MessageSource::UserPrompted),
            test_reply("c3", "Designer", "I can help with UI.", MessageSource::UserPrompted),
        ];

        let context = build_context_for_pal(&pal, &replies, &[]).unwrap();

        // System message + 2 pal replies = 3 messages
        assert_eq!(context.len(), 3);
        assert_eq!(context[0].sender, MessageRole::System);
        assert_eq!(context[1].sender, MessageRole::Assistant);
        assert_eq!(context[1].text, "Let's focus on features.");
        assert_eq!(context[1].pal_id, Some("c2".to_string()));
        assert_eq!(context[1].pal_name, Some("PM".to_string()));
        assert_eq!(context[2].sender, MessageRole::Assistant);
        assert_eq!(context[2].text, "I can help with UI.");
        assert_eq!(context[2].pal_id, Some("c3".to_string()));
        assert_eq!(context[2].pal_name, Some("Designer".to_string()));
    }

    #[test]
    fn build_context_without_system_prompt_omits_system_message() {
        let pal = test_character("c1", "Bot", "", "A simple bot");
        let replies = vec![];

        let context = build_context_for_pal(&pal, &replies, &[]).unwrap();

        assert_eq!(context.len(), 0);
    }

    #[test]
    fn build_context_preserves_reply_source_metadata() {
        let pal = test_character("c1", "Bot", "You are a bot.", "A bot");
        let replies = vec![
            test_reply("c2", "Other", "hello", MessageSource::Directed),
        ];

        let context = build_context_for_pal(&pal, &replies, &[]).unwrap();

        assert_eq!(context.len(), 2);
        assert_eq!(context[1].source, MessageSource::Directed);
    }

    #[test]
    fn build_context_includes_conversation_history() {
        let pal = test_character("c1", "Bot", "You are a bot.", "A bot");
        let replies = vec![test_reply("c2", "Other", "a pal reply", MessageSource::UserPrompted)];
        let history = vec![
            Message {
                id: "user-msg-1".to_string(),
                text: "Hello everyone!".to_string(),
                reasoning: None,
                sender: MessageRole::User,
                timestamp: 100,
                tokens: None,
                embedding: None,
                images: None,
                tool_calls: None,
                source: MessageSource::UserPrompted,
                pal_id: None,
                pal_name: None,
            },
            Message {
                id: "assistant-msg-1".to_string(),
                text: "I can help!".to_string(),
                reasoning: None,
                sender: MessageRole::Assistant,
                timestamp: 200,
                tokens: None,
                embedding: None,
                images: None,
                tool_calls: None,
                source: MessageSource::UserPrompted,
                pal_id: Some("c3".to_string()),
                pal_name: Some("Helper".to_string()),
            },
        ];

        let context = build_context_for_pal(&pal, &replies, &history).unwrap();

        // System prompt + 2 history messages + 1 pal reply = 4 messages
        assert_eq!(context.len(), 4);
        assert_eq!(context[0].sender, MessageRole::System);
        assert_eq!(context[0].text, "You are a bot.");
        // History messages come next, in order
        assert_eq!(context[1].sender, MessageRole::User);
        assert_eq!(context[1].text, "Hello everyone!");
        assert_eq!(context[2].sender, MessageRole::Assistant);
        assert_eq!(context[2].text, "I can help!");
        // Then pal replies
        assert_eq!(context[3].sender, MessageRole::Assistant);
        assert_eq!(context[3].text, "a pal reply");
    }

    // ── orchestrate_multi_pal_round tests ────────────────────
    //
    // These tests exercise the orchestrate_multi_pal_round function with
    // mocked dependencies. Tests that require actual LLM calls are marked
    // #[ignore] and serve as documentation stubs.

    /// Test helper: create a minimal Provider.
    fn test_provider() -> Provider {
        use crate::configs::model::{Model as ProviderModel, ModelInfo, ModelMetadata};

        Provider {
            name: "test-provider".to_string(),
            display_name: "Test Provider".to_string(),
            base_url: "http://localhost:9999".to_string(),
            models: vec![ProviderModel {
                metadata: ModelMetadata {
                    name: "gpt-4".to_string(),
                    display_name: "GPT-4".to_string(),
                    creator: None,
                    version: None,
                    description: None,
                },
                model_info: ModelInfo::TextGeneration {
                    parameters: Default::default(),
                    capabilities: vec![],
                    multimodal: None,
                },
                tokenizer: None,
                max_input_size: 8192,
                api_endpoint: None,
            }],
        }
    }

    #[tokio::test]
    async fn orchestrate_empty_target_pal_ids_returns_empty_replies() {
        // Step 1: Empty target_pal_ids → no pal replies, director has no
        // unlocked pals → returns empty vec (no LLM calls made).
        let (handle, conv_id) = setup_app();
        let provider = test_provider();
        let characters = vec![
            test_character("c1", "Alice", "You are Alice.", "Expert"),
        ];

        let result = orchestrate_multi_pal_round(
            &handle,
            &conv_id,
            "msg1",
            vec![],  // empty target_pal_ids
            &characters,
            &provider,
            None,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn orchestrate_pal_id_not_found_returns_error() {
        // Step 5: Pal ID not found in all_characters → error before any LLM call.
        let (handle, conv_id) = setup_app();
        let provider = test_provider();
        let characters = vec![
            test_character("c1", "Alice", "You are Alice.", "Expert"),
        ];

        let result = orchestrate_multi_pal_round(
            &handle,
            &conv_id,
            "msg1",
            vec!["nonexistent".to_string()],
            &characters,
            &provider,
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("nonexistent"), "error should mention the missing pal_id");
        assert!(err.contains("Pal not found"), "error should contain 'Pal not found'");
    }

    /// Stub: Single pal → one reply, then director check.
    /// Full test requires mocking the LLM call; preserved here as a
    /// documentation stub.
    #[tokio::test]
    #[ignore = "Requires mocking call_llm_with_pal_config to return a canned response"]
    async fn orchestrate_single_pal_produces_one_reply_then_director_check() {
        // TODO:
        // 1. Provide one pal in target_pal_ids
        // 2. Expect replies.len() == 1 (the pal's own reply)
        // 3. Verify director check ran but returned None (only one pal, no other to invoke)
    }

    /// Stub: Multiple pals in order → sequential replies.
    #[tokio::test]
    #[ignore = "Requires mocking call_llm_with_pal_config to return canned responses"]
    async fn orchestrate_multiple_pals_reply_in_order() {
        // TODO:
        // 1. Provide pals [c1, c2, c3] in target_pal_ids
        // 2. Verify each pal's message appears in order
        // 3. Verify each pal's context includes previous pals' replies
    }

    /// Stub: Director invokes a previously @mentioned pal.
    #[tokio::test]
    #[ignore = "Requires mocking the director LLM call to return an invoke decision"]
    async fn orchestrate_director_invokes_previously_mentioned_pal() {
        // TODO: Mock director LLM call to return {"action": "invoke", "pal_id": "c2"}.
        // 1. Provide pal c1 in target_pal_ids, with c2 also in all_characters
        // 2. After c1 replies, director decides to invoke c2
        // 3. Verify c2's PalReply is appended with MessageSource::Directed
    }

    /// Stub: Director decides not to invoke → no additional reply.
    #[tokio::test]
    #[ignore = "Requires mocking the director LLM call to return a 'none' decision"]
    async fn orchestrate_director_decides_not_to_invoke() {
        // TODO: Mock director LLM call to return {"action": "none"}.
        // 1. Provide pal c1 in target_pal_ids
        // 2. After c1 replies, director decides not to invoke anyone
        // 3. Verify no additional PalReply beyond c1's
    }
}
