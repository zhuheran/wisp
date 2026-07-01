use std::collections::{HashMap, HashSet};
use std::error::Error;

use serde_json::Value;
use tauri::AppHandle;

use crate::configs::character::Character;
use crate::configs::provider::Provider;
use crate::conversation::director::{assemble_director_prompt, parse_director_response};
use crate::conversation::types::MessageSource;
use crate::db::types::Message;

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
pub async fn orchestrate_multi_pal_round(
    app_handle: &AppHandle,
    conversation_id: &str,
    user_message_id: &str,
    target_pal_ids: Vec<String>,
    all_characters: &[Character],
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<Vec<PalReply>, String> {
    let mut replies = Vec::new();
    let mut unlocked_pal_ids: HashSet<String> = HashSet::new();

    for pal_id in &target_pal_ids {
        unlocked_pal_ids.insert(pal_id.clone());

        let pal = all_characters
            .iter()
            .find(|c| c.id == *pal_id)
            .ok_or_else(|| format!("Pal not found: {}", pal_id))?;

        // Build context: existing conversation + previous pal replies in this round
        let context = build_context_for_pal(conversation_id, &replies, pal)?;

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
/// 1. The pal's system prompt as a system message
/// 2. Previous pal replies in this round as assistant messages
///
/// Existing conversation history from DB will be loaded and prepended
/// in the full implementation (Task 4) when Chat is available.
pub fn build_context_for_pal(
    conversation_id: &str,
    previous_replies: &[PalReply],
    pal: &Character,
) -> Result<Vec<Message>, String> {
    let mut context = Vec::new();

    // Prepend pal's system_prompt as a system message
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

    // Append previous pal replies in this round as assistant messages
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

    let _ = conversation_id; // suppress unused warning

    Ok(context)
}

/// After all pal replies, run the director check to see if another
/// @mentioned pal should be invited into the conversation.
async fn run_director_check(
    app_handle: &AppHandle,
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

    // 2. Get recent conversation text (from pal replies + context)
    let recent_messages: Vec<String> = pal_replies
        .iter()
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

            // Existing conversation history + previous pal replies as context
            let context = build_context_for_pal(conversation_id, pal_replies, pal)?;

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
async fn call_llm_with_pal_config(
    app_handle: &AppHandle,
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
    use crate::configs::character::Character;
    use crate::db::types::MessageRole;

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

    // ── build_context_for_pal tests ────────────────────────────

    #[test]
    fn build_context_includes_system_prompt() {
        let pal = test_character("c1", "Code Reviewer", "You are a code reviewer.", "Reviews code");
        let replies = vec![];

        let context = build_context_for_pal("conv1", &replies, &pal).unwrap();

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

        let context = build_context_for_pal("conv1", &replies, &pal).unwrap();

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

        let context = build_context_for_pal("conv1", &replies, &pal).unwrap();

        assert_eq!(context.len(), 0);
    }

    #[test]
    fn build_context_preserves_reply_source_metadata() {
        let pal = test_character("c1", "Bot", "You are a bot.", "A bot");
        let replies = vec![
            test_reply("c2", "Other", "hello", MessageSource::Directed),
        ];

        let context = build_context_for_pal("conv1", &replies, &pal).unwrap();

        assert_eq!(context.len(), 2);
        assert_eq!(context[1].source, MessageSource::Directed);
    }
}
