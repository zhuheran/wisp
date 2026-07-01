use super::types::ConversationToolCall;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCalls {
    pub clean_text: String,
    pub calls: Vec<ConversationToolCall>,
}

/// 只解析 `<|tool_calls|>JSON_ARRAY<|/tool_calls|>` 格式
///
/// 示例:
///   <|tool_calls|>
///   [{"name":"search","arguments":{"q":"weather"}},{"name":"read","arguments":{"path":"/tmp/x"}}]
///   <|/tool_calls|>
///
/// 数组中每个元素必须有 name 和 arguments（object），否则被忽略。
pub fn parse_tool_calls(text: &str) -> ParsedToolCalls {
    let mut calls = Vec::new();
    let mut clean = String::new();
    let mut cursor = 0;

    while let Some(tag_start_rel) = text[cursor..].find("<|tool_calls|>") {
        let tag_start = cursor + tag_start_rel;
        clean.push_str(&text[cursor..tag_start]);

        let content_start = tag_start + "<|tool_calls|>".len();
        let Some(tag_end_rel) = text[content_start..].find("<|/tool_calls|>") else {
            // 缺少闭标签，保留原文
            clean.push_str(&text[tag_start..]);
            cursor = text.len();
            break;
        };
        let tag_end = content_start + tag_end_rel;
        let inner = text[content_start..tag_end].trim();

        match serde_json::from_str::<Vec<serde_json::Value>>(inner) {
            Ok(array) => {
                let parsed: Vec<_> = array
                    .into_iter()
                    .filter_map(normalize_tool_call)
                    .filter_map(|v| serde_json::from_value::<ConversationToolCall>(v).ok())
                    .collect();

                if parsed.is_empty() {
                    // 数组为空或全部无效，保留原文
                    clean.push_str(&text[tag_start..tag_end + "<|/tool_calls|>".len()]);
                } else {
                    calls.extend(parsed);
                }
            }
            Err(_) => {
                // JSON 解析失败，保留原文
                clean.push_str(&text[tag_start..tag_end + "<|/tool_calls|>".len()]);
            }
        }

        cursor = tag_end + "<|/tool_calls|>".len();
    }

    clean.push_str(&text[cursor..]);

    ParsedToolCalls {
        clean_text: cleanup_markdown_fences(&clean),
        calls,
    }
}

fn normalize_tool_call(mut value: serde_json::Value) -> Option<serde_json::Value> {
    let object = value.as_object_mut()?;

    let name = object.get("name")?.as_str()?;
    if name.trim().is_empty() {
        return None;
    }

    if !matches!(object.get("arguments"), Some(serde_json::Value::Object(_))) {
        return None;
    }

    // 没有 id 则自动生成
    if !object.contains_key("id") {
        object.insert(
            "id".to_string(),
            serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
        );
    }

    Some(value)
}

fn cleanup_markdown_fences(text: &str) -> String {
    text.replace("```json\n\n```", "")
        .replace("```json\n```", "")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tool_call_array() {
        let parsed = parse_tool_calls(
            "before <|tool_calls|>[{\"name\":\"search\",\"arguments\":{\"q\":\"weather\"}}]<|/tool_calls|> after",
        );

        assert_eq!(parsed.clean_text, "before  after");
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "search");
        assert_eq!(parsed.calls[0].arguments["q"], "weather");
        assert!(!parsed.calls[0].id.is_empty());
    }

    #[test]
    fn parses_multiple_tool_calls_in_array() {
        let input = concat!(
            "text ",
            "<|tool_calls|>",
            r#"[{"name":"a","arguments":{"x":1}},{"name":"b","arguments":{"y":2}}]"#,
            "<|/tool_calls|>",
            " end"
        );
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, "text  end");
        assert_eq!(parsed.calls.len(), 2);
        assert_eq!(parsed.calls[0].name, "a");
        assert_eq!(parsed.calls[0].arguments["x"], 1);
        assert_eq!(parsed.calls[1].name, "b");
        assert_eq!(parsed.calls[1].arguments["y"], 2);
    }

    #[test]
    fn ignores_invalid_tool_call_without_arguments_object() {
        let input = concat!(
            "before ",
            "<|tool_calls|>",
            r#"[{"name":"bad_tool"}]"#,
            "<|/tool_calls|>",
            " after"
        );
        let parsed = parse_tool_calls(input);

        // 数组内全部无效，保留原文
        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn ignores_tool_call_with_empty_name() {
        let input = concat!(
            "<|tool_calls|>",
            r#"[{"name":"","arguments":{}}]"#,
            "<|/tool_calls|>"
        );
        let parsed = parse_tool_calls(input);

        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn keeps_non_tool_text_unchanged() {
        let input = "just plain text with no tags";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }

    #[test]
    fn handles_multiple_sequential_tag_blocks() {
        let input = concat!(
            "a ",
            "<|tool_calls|>",
            r#"[{"name":"first","arguments":{"n":1}}]"#,
            "<|/tool_calls|>",
            " b ",
            "<|tool_calls|>",
            r#"[{"name":"second","arguments":{"n":2}}]"#,
            "<|/tool_calls|>",
            " c"
        );
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, "a  b  c");
        assert_eq!(parsed.calls.len(), 2);
        assert_eq!(parsed.calls[0].name, "first");
        assert_eq!(parsed.calls[1].name, "second");
    }

    #[test]
    fn mixes_valid_and_invalid_in_same_array() {
        let input = concat!(
            "<|tool_calls|>",
            r#"[{"name":"valid","arguments":{"ok":true}},{"name":""}]"#,
            "<|/tool_calls|>"
        );
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "valid");
    }

    #[test]
    fn assigns_auto_id_when_missing() {
        let input = concat!(
            "<|tool_calls|>",
            r#"[{"name":"auto","arguments":{"x":1}}]"#,
            "<|/tool_calls|>"
        );
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls.len(), 1);
        assert!(!parsed.calls[0].id.is_empty());
    }

    #[test]
    fn preserves_provided_id() {
        let input = concat!(
            "<|tool_calls|>",
            r#"[{"id":"my_id","name":"with_id","arguments":{"x":1}}]"#,
            "<|/tool_calls|>"
        );
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls[0].id, "my_id");
    }

    #[test]
    fn unclosed_tag_keeps_original_text() {
        let input = "start <|tool_calls|>[{\"name\":\"x\",\"arguments\":{}}]";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.clean_text, input);
        assert!(parsed.calls.is_empty());
    }
}
