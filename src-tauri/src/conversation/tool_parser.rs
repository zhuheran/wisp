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

    while let Some(start) = text[cursor..].find("{").map(|idx| cursor + idx) {
        clean.push_str(&text[cursor..start]);
        if let Some((end, value)) = parse_json_value_at(text, start) {
            let parsed_calls = calls_from_value(&value);
            if parsed_calls.is_empty() {
                clean.push_str(&text[start..end]);
            } else {
                calls.extend(parsed_calls);
            }
            cursor = end;
        } else {
            clean.push_str(&text[start..start + 1]);
            cursor = start + 1;
        }
    }
    clean.push_str(&text[cursor..]);

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
    if let Some(tool_call) = value.get("tool_call") {
        return serde_json::from_value::<ConversationToolCall>(normalize_tool_call(tool_call.clone()))
            .map(|call| vec![call])
            .unwrap_or_default();
    }

    if let Some(tool_calls) = value.get("tool_calls").and_then(|value| value.as_array()) {
        return tool_calls
            .iter()
            .filter_map(|call| serde_json::from_value::<ConversationToolCall>(normalize_tool_call(call.clone())).ok())
            .collect();
    }

    Vec::new()
}

fn normalize_tool_call(mut value: serde_json::Value) -> serde_json::Value {
    let Some(object) = value.as_object_mut() else {
        return value;
    };

    if !object.contains_key("id") {
        object.insert("id".to_string(), serde_json::Value::String(uuid::Uuid::new_v4().to_string()));
    }
    if !object.contains_key("arguments") {
        object.insert("arguments".to_string(), serde_json::json!({}));
    }

    value
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
    fn handles_nested_json_arguments() {
        let parsed = parse_tool_calls(
            "before {\"tool_call\":{\"id\":\"call_nested\",\"name\":\"s:nested\",\"arguments\":{\"filters\":{\"a\":1,\"b\":[true,{\"c\":\"d\"}]}}}} after",
        );

        assert_eq!(parsed.clean_text, "before  after");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].arguments["filters"]["b"][1]["c"], "d");
    }

    #[test]
    fn leaves_non_tool_json_in_clean_text() {
        let input = "Here is data {\"hello\":\"world\"}";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }
}
