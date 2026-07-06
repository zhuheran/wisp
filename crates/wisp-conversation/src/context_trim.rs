use wisp_db::types::Message;

const IMAGE_TOKEN_ESTIMATE: usize = 1000;

pub fn estimate_tokens(messages: &[Message]) -> usize {
    let mut total: usize = 0;
    for msg in messages {
        let text_len = msg.text.chars().count();
        let reasoning_len = msg
            .reasoning
            .as_ref()
            .map(|r| r.chars().count())
            .unwrap_or(0);
        let tool_calls_len = msg
            .tool_calls
            .as_ref()
            .map(|tc| tc.chars().count())
            .unwrap_or(0);
        let char_total = text_len + reasoning_len + tool_calls_len;
        total += char_total / 4;

        if let Some(images) = &msg.images {
            total += images.len() * IMAGE_TOKEN_ESTIMATE;
        }
    }
    total
}

pub fn trim_context(messages: Vec<Message>, max_tokens: usize, sliding_ratio: f32) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    let total = estimate_tokens(&messages);
    if total <= max_tokens {
        return messages;
    }

    let target = (max_tokens as f32 * sliding_ratio) as usize;
    let n = messages.len();

    if n == 1 {
        return messages;
    }

    let first = messages.first().cloned().unwrap();
    let last = messages.last().cloned().unwrap();

    let middle = &messages[1..n.saturating_sub(1)];

    let mut recent: Vec<Message> = Vec::new();
    for msg in middle.iter().rev() {
        recent.push(msg.clone());
        let mut probe = Vec::with_capacity(recent.len() + 2);
        probe.push(first.clone());
        probe.extend(recent.iter().rev().cloned());
        probe.push(last.clone());
        let current_tokens = estimate_tokens(&probe);
        if current_tokens >= target {
            break;
        }
    }

    recent.reverse();

    let mut kept = Vec::with_capacity(recent.len() + 2);
    kept.push(first);
    kept.extend(recent);
    kept.push(last);

    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_db::types::{ImageContent, MessageRole};

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
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn estimate_tokens_text_message_uses_chars_div_4() {
        let msg = text_message("hello world!"); // 12 chars
        let tokens = estimate_tokens(&[msg]);
        assert_eq!(tokens, 3); // 12 / 4 = 3
    }

    #[test]
    fn estimate_tokens_includes_reasoning() {
        let mut msg = text_message("hi"); // 2 chars
        msg.reasoning = Some("thinking deeply".to_string()); // 15 chars
        let tokens = estimate_tokens(&[msg]);
        assert_eq!(tokens, 4); // (2+15)/4 = 4
    }

    #[test]
    fn estimate_tokens_counts_images_at_fixed_estimate() {
        let msg = image_message(3);
        let tokens = estimate_tokens(&[msg]);
        assert_eq!(tokens, 3 * IMAGE_TOKEN_ESTIMATE);
    }

    #[test]
    fn estimate_tokens_includes_tool_calls_json() {
        let mut msg = text_message("");
        msg.tool_calls = Some(r#"[{"name":"tool","arguments":{}}]"#.to_string()); // 33 chars
        let tokens = estimate_tokens(&[msg]);
        assert!(tokens >= 8); // ~33/4 = 8
    }

    #[test]
    fn trim_context_under_limit_returns_unchanged() {
        let msgs = vec![text_message("short"), text_message("also short")];
        let result = trim_context(msgs.clone(), 10000, 0.7);
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
        let result = trim_context(msgs, 500, 0.7);
        assert!(result.len() < 20);
        assert_eq!(result.first().unwrap().id, "m0");
        assert_eq!(result.last().unwrap().id, "m19");
    }

    #[test]
    fn trim_context_never_returns_empty_for_nonempty_input() {
        let msgs = vec![text_message(&"x".repeat(10000))];
        let result = trim_context(msgs, 100, 0.7);
        assert!(!result.is_empty());
    }

    #[test]
    fn trim_context_single_message_always_kept() {
        let msgs = vec![text_message(&"x".repeat(100000))];
        let result = trim_context(msgs, 100, 0.7);
        assert_eq!(result.len(), 1);
    }
}
