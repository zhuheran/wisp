use crate::configs::character::Character;

#[derive(Debug, Clone, PartialEq)]
pub struct DirectorDecision {
    pub should_invoke: bool,
    pub target_pal_id: Option<String>,
}

/// Assemble the director prompt that asks the LLM whether another character
/// should join the conversation.
pub fn assemble_director_prompt(
    recent_messages: &[String],
    available_pals: &[Character],
) -> String {
    if available_pals.is_empty() {
        return r#"You are a dialogue director. Your job is to determine if another character should join the conversation.

Available characters (only those already @mentioned by the user):
(none)

Recent conversation:
"#.to_string()
            + &recent_messages.join("\n")
            + "\n\n"
            + "If another character's expertise or perspective would add value, respond with a JSON object:\n"
            + "{\"action\": \"invoke\", \"pal_id\": \"<character_id>\"}\n"
            + "\n"
            + "Otherwise respond with:\n"
            + "{\"action\": \"none\"}\n"
            + "\n"
            + "Director response: ";
    }

    let pals_desc: Vec<String> = available_pals
        .iter()
        .map(|p| format!("- {}: {}", p.name, p.role_bio))
        .collect();

    format!(
        r#"You are a dialogue director. Your job is to determine if another character should join the conversation.

Available characters (only those already @mentioned by the user):
{}

Recent conversation:
{}

If another character's expertise or perspective would add value, respond with a JSON object:
{{"action": "invoke", "pal_id": "<character_id>"}}

Otherwise respond with:
{{"action": "none"}}

Director response: "#,
        pals_desc.join("\n"),
        recent_messages.join("\n"),
    )
}

/// Parse the director's response text into a DirectorDecision.
pub fn parse_director_response(response: &str) -> DirectorDecision {
    let trimmed = response.trim();

    // Try to find a JSON block within the response
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed[start..].rfind('}') {
            &trimmed[start..=start + end]
        } else {
            return DirectorDecision {
                should_invoke: false,
                target_pal_id: None,
            };
        }
    } else {
        return DirectorDecision {
            should_invoke: false,
            target_pal_id: None,
        };
    };

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(val) => {
            let action = val.get("action").and_then(|a| a.as_str()).unwrap_or("none");
            if action == "invoke" {
                let pal_id = val.get("pal_id").and_then(|p| p.as_str()).map(String::from);
                DirectorDecision {
                    should_invoke: true,
                    target_pal_id: pal_id,
                }
            } else {
                DirectorDecision {
                    should_invoke: false,
                    target_pal_id: None,
                }
            }
        }
        Err(_) => DirectorDecision {
            should_invoke: false,
            target_pal_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_character(id: &str, name: &str, role_bio: &str) -> Character {
        Character {
            id: id.to_string(),
            name: name.to_string(),
            alias: None,
            avatar: None,
            description: String::new(),
            system_prompt: String::new(),
            parameters: Vec::new(),
            model_id: "gpt-4".to_string(),
            created_at: 0,
            updated_at: 0,
            role_bio: role_bio.to_string(),
        }
    }

    // ── assemble_director_prompt tests ──────────────────────────────

    #[test]
    fn assemble_director_prompt_includes_pal_names_and_bios() {
        let pals = vec![
            test_character("c1", "Code Reviewer", "Expert in Rust and system design"),
            test_character("c2", "PM", "Product strategy"),
        ];
        let messages = vec!["User: how do I refactor this?".to_string()];

        let prompt = assemble_director_prompt(&messages, &pals);

        assert!(prompt.contains("Code Reviewer"), "should include pal name");
        assert!(
            prompt.contains("Expert in Rust and system design"),
            "should include pal bio"
        );
        assert!(prompt.contains("PM"), "should include second pal name");
        assert!(prompt.contains("Product strategy"), "should include second pal bio");
        assert!(
            prompt.contains("User: how do I refactor this?"),
            "should include recent messages"
        );
        assert!(
            prompt.contains("\"action\": \"invoke\""),
            "should describe the invoke format"
        );
    }

    #[test]
    fn assemble_director_prompt_includes_recent_messages() {
        let messages = vec![
            "User: hello".to_string(),
            "Assistant: hi there".to_string(),
            "User: can you help?".to_string(),
        ];
        let pals = vec![test_character("c1", "Helper", "Helps with things")];

        let prompt = assemble_director_prompt(&messages, &pals);

        assert!(prompt.contains("User: hello"));
        assert!(prompt.contains("Assistant: hi there"));
        assert!(prompt.contains("User: can you help?"));
    }

    #[test]
    fn empty_pals_list_produces_prompt_with_no_options() {
        let messages = vec!["User: hello".to_string()];
        let pals: Vec<Character> = vec![];

        let prompt = assemble_director_prompt(&messages, &pals);

        assert!(prompt.contains("(none)"), "should indicate no pals available");
        assert!(!prompt.contains("- "), "should not list any pals");
    }

    #[test]
    fn empty_messages_produces_prompt_with_no_conversation() {
        let messages: Vec<String> = vec![];
        let pals = vec![test_character("c1", "Bot", "A bot")];

        let prompt = assemble_director_prompt(&messages, &pals);

        assert!(prompt.contains("- Bot:"));
        assert!(
            prompt.ends_with("Director response: "),
            "should end with the prompt suffix"
        );
    }

    // ── parse_director_response tests ───────────────────────────────

    #[test]
    fn parse_director_response_invoke_with_pal_id() {
        let response = r#"{"action": "invoke", "pal_id": "c1"}"#;

        let decision = parse_director_response(response);

        assert!(decision.should_invoke);
        assert_eq!(decision.target_pal_id, Some("c1".to_string()));
    }

    #[test]
    fn parse_director_response_none_action() {
        let response = r#"{"action": "none"}"#;

        let decision = parse_director_response(response);

        assert!(!decision.should_invoke);
        assert_eq!(decision.target_pal_id, None);
    }

    #[test]
    fn parse_director_response_with_extra_text_before_json() {
        let response = "Let me think...\n{\"action\": \"invoke\", \"pal_id\": \"c2\"}\nThat's my decision.";

        let decision = parse_director_response(response);

        assert!(decision.should_invoke);
        assert_eq!(decision.target_pal_id, Some("c2".to_string()));
    }

    #[test]
    fn parse_director_response_invalid_json_returns_none() {
        let response = "I don't think anyone else needs to chime in.";

        let decision = parse_director_response(response);

        assert!(!decision.should_invoke);
        assert_eq!(decision.target_pal_id, None);
    }

    #[test]
    fn parse_director_response_missing_action_returns_none() {
        let response = r#"{"foo": "bar"}"#;

        let decision = parse_director_response(response);

        assert!(!decision.should_invoke);
        assert_eq!(decision.target_pal_id, None);
    }

    #[test]
    fn parse_director_response_invoke_without_pal_id_returns_invoke_with_none() {
        let response = r#"{"action": "invoke"}"#;

        let decision = parse_director_response(response);

        assert!(decision.should_invoke);
        assert_eq!(decision.target_pal_id, None);
    }

    #[test]
    fn parse_director_response_empty_string_returns_none() {
        let response = "";

        let decision = parse_director_response(response);

        assert!(!decision.should_invoke);
        assert_eq!(decision.target_pal_id, None);
    }
}
