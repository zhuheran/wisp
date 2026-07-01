use super::types::ConversationToolCall;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCalls {
    pub clean_text: String,
    pub calls: Vec<ConversationToolCall>,
}

pub fn parse_tool_calls(text: &str) -> ParsedToolCalls {
    let mut calls = Vec::new();
    let mut clean = String::new();
    let mut cursor = 0;

    while let Some(tag_start_rel) = text[cursor..].find("<|tool_call|>") {
        let tag_start = cursor + tag_start_rel;
        clean.push_str(&text[cursor..tag_start]);

        let content_start = tag_start + "<|tool_call|>".len();
        let Some(tag_end_rel) = text[content_start..].find("<|tool_call|>") else {
            clean.push_str(&text[tag_start..]);
            cursor = text.len();
            break;
        };
        let tag_end = content_start + tag_end_rel;
        let inner = text[content_start..tag_end].trim();

        match serde_json::from_str::<serde_json::Value>(inner) {
            Ok(value) => {
                let parsed_calls = calls_from_value(&value);
                if parsed_calls.is_empty() {
                    clean.push_str(&text[tag_start..tag_end + "<|tool_call|>".len()]);
                } else {
                    calls.extend(parsed_calls);
                }
            }
            Err(_) => {
                clean.push_str(&text[tag_start..tag_end + "<|tool_call|>".len()]);
            }
        }

        cursor = tag_end + "<|tool_call|>".len();
    }

    let remaining = &text[cursor..];
    let mut fallback_clean = String::new();
    let mut fallback_cursor = 0;

    while let Some(start) = remaining[fallback_cursor..].find("{").map(|idx| fallback_cursor + idx) {
        fallback_clean.push_str(&remaining[fallback_cursor..start]);
        if let Some((end, value)) = parse_json_value_at(remaining, start) {
            let parsed_calls = calls_from_value(&value);
            if parsed_calls.is_empty() {
                fallback_clean.push_str(&remaining[start..end]);
            } else {
                calls.extend(parsed_calls);
            }
            fallback_cursor = end;
        } else {
            fallback_clean.push_str(&remaining[start..start + 1]);
            fallback_cursor = start + 1;
        }
    }
    fallback_clean.push_str(&remaining[fallback_cursor..]);
    clean.push_str(&fallback_clean);

    ParsedToolCalls {
        clean_text: cleanup_markdown_json_fences(&clean),
        calls,
    }
}

fn parse_json_value_at(text: &str, start: usize) -> Option<(usize, serde_json::Value)> {
    let bytes = text.as_bytes();
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;

    for index in start..bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let end = index + 1;
                    let value = serde_json::from_str(&text[start..end]).ok()?;
                    return Some((end, value));
                }
            }
            _ => {}
        }
    }

    None
}

fn calls_from_value(value: &serde_json::Value) -> Vec<ConversationToolCall> {
    if value.get("name").is_some() {
        return normalize_tool_call(value.clone())
            .and_then(|normalized| serde_json::from_value::<ConversationToolCall>(normalized).ok())
            .map(|call| vec![call])
            .unwrap_or_default();
    }

    if let Some(tool_call) = value.get("tool_call") {
        return normalize_tool_call(tool_call.clone())
            .and_then(|normalized| serde_json::from_value::<ConversationToolCall>(normalized).ok())
            .map(|call| vec![call])
            .unwrap_or_default();
    }

    if let Some(tool_calls) = value.get("tool_calls").and_then(|value| value.as_array()) {
        return tool_calls
            .iter()
            .filter_map(|call| normalize_tool_call(call.clone()))
            .filter_map(|call| serde_json::from_value::<ConversationToolCall>(call).ok())
            .collect();
    }

    Vec::new()
}

fn normalize_tool_call(mut value: serde_json::Value) -> Option<serde_json::Value> {
    let object = value.as_object_mut()?;

    if !object.get("name").and_then(|value| value.as_str()).is_some_and(|name| !name.trim().is_empty()) {
        return None;
    }

    if !matches!(object.get("arguments"), Some(serde_json::Value::Object(_))) {
        return None;
    }

    if let Some(name) = object.get("name").cloned() {
        object.entry("qualified_name".to_string()).or_insert(name);
    }
    if !object.contains_key("id") {
        object.insert("id".to_string(), serde_json::Value::String(uuid::Uuid::new_v4().to_string()));
    }

    Some(value)
}

fn cleanup_markdown_json_fences(text: &str) -> String {
    text.replace("```json\n\n```", "")
        .replace("```json\n```", "")
        .replace("````json\n\n````", "")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_single_tool_call_and_removes_it_from_clean_text() {
        let parsed = parse_tool_calls(
            "I will search.\n```json\n{\"tool_call\":{\"name\":\"server:search\",\"arguments\":{\"query\":\"wisp\"}}}\n```",
        );

        assert_eq!(parsed.clean_text, "I will search.");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "server:search");
        assert_eq!(parsed.calls[0].arguments["query"], "wisp");
        assert!(!parsed.calls[0].id.is_empty());
    }

    #[test]
    fn parses_multiple_tool_calls_from_array_wrapper() {
        let parsed = parse_tool_calls(
            "{\"tool_calls\":[{\"id\":\"a\",\"name\":\"s:first\",\"arguments\":{}},{\"id\":\"b\",\"name\":\"s:second\",\"arguments\":{}}]}",
        );

        assert_eq!(parsed.clean_text, "");
        assert_eq!(parsed.calls.iter().map(|call| call.name.as_str()).collect::<Vec<_>>(), vec!["s:first", "s:second"]);
    }

    #[test]
    fn parses_direct_tagged_tool_call_object_and_removes_it_from_clean_text() {
        let parsed = parse_tool_calls(
            "before <|tool_call|> {\"name\": \"server:search\", \"arguments\": {\"query\": \"2025年春节日期\", \"max_results\": 3}} <|tool_call|> after",
        );

        assert_eq!(parsed.clean_text, "before  after");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "server:search");
        assert_eq!(parsed.calls[0].arguments["query"], "2025年春节日期");
        assert_eq!(parsed.calls[0].arguments["max_results"], 3);
        assert!(!parsed.calls[0].id.is_empty());
    }

    #[test]
    fn handles_nested_json_arguments() {
        let parsed = parse_tool_calls(
            "before {\"tool_call\":{\"id\":\"call_nested\",\"name\":\"s:nested\",\"arguments\":{\"filters\":{\"a\":1,\"b\":[true,{\"c\":\"d\"}]}}}} after",
        );

        assert_eq!(parsed.clean_text, "before  after");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].arguments["filters"]["b"][1]["c"], "d");
    }

    #[test]
    fn ignores_tool_call_without_arguments_object() {
        let input = "before <|tool_call|> {\"name\": \"mcp__tavily_search\"} <|tool_call|> after";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn leaves_non_tool_json_in_clean_text() {
        let input = "Here is data {\"hello\":\"world\"}";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }
}
