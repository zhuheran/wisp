use crate::types::ConversationToolCall;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCalls {
    pub clean_text: String,
    pub calls: Vec<ConversationToolCall>,
    /// 存在 `<|tool_calls|>...<|/tool_calls|>` 标签、但 JSON 解析失败或
    /// 数组内没有任何有效调用时，记录这些块的原始内容（已从 clean_text 中剔除）。
    /// 调用方据此把"模型尝试了工具调用但格式错误"作为工具调用失败处理，
    /// 而不是把原始标签文本当作普通助手消息展示。
    pub failed_blocks: Vec<String>,
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
    let mut failed_blocks = Vec::new();
    let mut clean = String::new();
    let mut cursor = 0;

    while let Some(tag_start_rel) = text[cursor..].find("<|tool_calls|>") {
        let tag_start = cursor + tag_start_rel;
        clean.push_str(&text[cursor..tag_start]);

        let content_start = tag_start + "<|tool_calls|>".len();
        // 模型常把闭标签写成各种变体（`<|/tool_calls|>`、`</|tool_calls|>`、
        // `</tool_calls>`、`<|tool_calls_end|>` 等），取最早出现的那一个。
        let Some((closer, tag_end_rel)) = find_closing_tag(&text[content_start..]) else {
            // 没有任何可识别的闭标签：仍视为一次失败的工具调用尝试，
            // 剔除原始块并记为失败，而不是把 `<|tool_calls|>` 当普通文本展示。
            failed_blocks.push(text[content_start..].trim().to_string());
            cursor = text.len();
            break;
        };
        let tag_end = content_start + tag_end_rel;
        let inner = text[content_start..tag_end].trim();

        // 先用严格 JSON 解析；失败时（常见于模型在字符串值里直接写入原始换行等
        // 控制字符，而不是转义成 \n），对字符串字面量内部做一次控制字符转义后再重试。
        let parsed_array = serde_json::from_str::<Vec<serde_json::Value>>(inner).or_else(|_| {
            serde_json::from_str::<Vec<serde_json::Value>>(&sanitize_json_strings(inner))
        });

        match parsed_array {
            Ok(array) => {
                let parsed: Vec<_> = array
                    .into_iter()
                    .filter_map(normalize_tool_call)
                    .filter_map(|v| serde_json::from_value::<ConversationToolCall>(v).ok())
                    .collect();

                if parsed.is_empty() {
                    // 标签存在但没有任何有效调用：剔除原始块，记为失败。
                    failed_blocks.push(inner.to_string());
                } else {
                    calls.extend(parsed);
                }
            },
            Err(_) => {
                // JSON 解析失败：剔除原始块，记为失败。
                failed_blocks.push(inner.to_string());
            },
        }

        cursor = tag_end + closer.len();
    }

    clean.push_str(&text[cursor..]);

    ParsedToolCalls { clean_text: cleanup_markdown_fences(&clean), calls, failed_blocks }
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

/// 把 JSON 文本中**字符串字面量内部**的原始控制字符转义成合法的 JSON 转义序列。
///
/// 严格 JSON 解析器（`serde_json`）会拒绝字符串值里的原始换行等控制字符，
/// 但 LLM 经常在 `arguments.code` 这种字段里直接写出多行代码而不转义。
/// 这里只处理引号内部的内容，保留 JSON 结构与已存在的转义。
/// 在文本中查找最早出现的工具调用闭标签。模型常把闭标签写成各种变体，
/// 这里统一接受并返回匹配到的字面量及其起始字节偏移。
fn find_closing_tag(text: &str) -> Option<(&'static str, usize)> {
    const CLOSERS: &[&str] = &[
        "<|/tool_calls|>",
        "</|tool_calls|>",
        "<|tool_calls_end|>",
        "<|end_tool_calls|>",
        "</tool_calls>",
    ];
    CLOSERS
        .iter()
        .filter_map(|c| text.find(c).map(|pos| (*c, pos)))
        .min_by_key(|(_, pos)| *pos)
}

fn sanitize_json_strings(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if !in_string {
            out.push(ch);
            if ch == '"' {
                in_string = true;
            }
            continue;
        }

        // 在字符串内部
        match ch {
            // 反斜杠转义：连同下一个字符原样保留，避免误伤已存在的 \"、\\n 等
            '\\' => {
                out.push(ch);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            },
            // 字符串结束
            '"' => {
                out.push(ch);
                in_string = false;
            },
            // 常见控制字符 -> 对应转义
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // 其余控制字符 -> \uXXXX
            c if c.is_control() => {
                let code = c as u32;
                out.push_str(&format!("\\u{:04x}", code));
            },
            c => out.push(c),
        }
    }

    out
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

        // 标签存在但无有效调用：原始块从 clean_text 中剔除，记入 failed_blocks。
        assert_eq!(parsed.clean_text, "before  after");
        assert!(parsed.calls.is_empty());
        assert_eq!(parsed.failed_blocks.len(), 1);
        assert!(parsed.failed_blocks[0].contains("bad_tool"));
    }

    #[test]
    fn ignores_tool_call_with_empty_name() {
        let input = concat!("<|tool_calls|>", r#"[{"name":"","arguments":{}}]"#, "<|/tool_calls|>");
        let parsed = parse_tool_calls(input);

        assert!(parsed.calls.is_empty());
        assert_eq!(parsed.failed_blocks.len(), 1);
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
    fn unclosed_tag_is_treated_as_failed_block() {
        let input = "start <|tool_calls|>[{\"name\":\"x\",\"arguments\":{}}]";
        let parsed = parse_tool_calls(input);

        // 没有任何可识别闭标签：剔除原始块、记为失败，而不是当作普通文本展示。
        assert_eq!(parsed.clean_text, "start");
        assert!(parsed.calls.is_empty());
        assert_eq!(parsed.failed_blocks.len(), 1);
    }

    /// 模型把闭标签写成 `</|tool_calls|>`（斜杠在竖线外）也能被识别。
    #[test]
    fn accepts_slash_pipe_closing_variant() {
        let input = "<|tool_calls|> [{\"name\":\"wisp_js_exec\",\"arguments\":{\"code\":\"x\"}}] </|tool_calls|>";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "wisp_js_exec");
        assert_eq!(parsed.clean_text, "");
    }

    /// 模型在 arguments.code 里直接写入原始换行（而非转义的 \n），
    /// 严格 JSON 解析会失败。parser 应当容错处理这种情况。
    #[test]
    fn parses_tool_call_with_raw_newlines_in_string_value() {
        let input = "<|tool_calls|> [{\"name\":\"wisp_js_exec\",\"arguments\":{\"code\":\"// Test\nlet x = 1;\nreturn x;\"}}] <|/tool_calls|>";
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].name, "wisp_js_exec");
        let code = parsed.calls[0].arguments["code"].as_str().unwrap();
        assert!(code.contains("// Test\nlet x = 1;"));
    }

    /// 真正无法解析的标签块：记为失败，且原始块不出现在 clean_text 中。
    #[test]
    fn records_failed_block_and_strips_it_from_clean_text() {
        let input = "ok <|tool_calls|>[not valid json]<|/tool_calls|> tail";
        let parsed = parse_tool_calls(input);

        assert!(parsed.calls.is_empty());
        assert_eq!(parsed.failed_blocks, vec!["[not valid json]".to_string()]);
        assert_eq!(parsed.clean_text, "ok  tail");
    }

    /// 容错重试不得破坏已经正确转义的字符串，也不得破坏 JSON 结构。
    #[test]
    fn sanitize_preserves_already_escaped_strings() {
        let input = r#"<|tool_calls|> [{"name":"wisp_js_exec","arguments":{"code":"// a\n// b\tc"}}] <|/tool_calls|>"#;
        let parsed = parse_tool_calls(input);

        assert_eq!(parsed.calls.len(), 1);
        let code = parsed.calls[0].arguments["code"].as_str().unwrap();
        assert_eq!(code, "// a\n// b\tc");
    }
}
