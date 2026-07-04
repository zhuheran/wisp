use serde_json::Value;
use wisp_common::{ToolContent, ToolResult};

/// Escape a value for inline code / cell use: collapse newlines, escape
/// backticks (so inline code spans stay intact).
fn escape_inline(s: &str) -> String {
    s.replace("\r\n", " ")
        .replace('\n', " ")
        .replace('`', "\\`")
}

/// Render a scalar argument value as inline code. Returns `None` for objects
/// and arrays, which must be rendered as fenced blocks instead.
fn render_arg_inline(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("`null`".to_string()),
        Value::Bool(b) => Some(format!("`{b}`")),
        Value::Number(n) => Some(format!("`{n}`")),
        Value::String(s) => Some(format!("`{}`", escape_inline(s))),
        _ => None,
    }
}

/// Collect the textual pieces of a tool result, in order. Non-text content is
/// rendered as an italic placeholder. Empty text pieces are skipped.
pub(crate) fn collect_result_pieces(result: &ToolResult) -> Vec<String> {
    let mut pieces = Vec::new();
    for c in &result.content {
        match c {
            ToolContent::Text { text } if !text.is_empty() => pieces.push(text.clone()),
            ToolContent::Image { .. } => pieces.push("_[image]_".to_string()),
            ToolContent::Resource { uri, text, .. } => {
                pieces.push(text.clone().unwrap_or_else(|| format!("_[resource: {uri}]_")));
            }
            _ => {}
        }
    }
    pieces
}

/// Build the markdown lines that represent the result body: a `**Result**`
/// (or `> **Error**`) label followed by the content pieces. Multiple pieces
/// are separated by a horizontal rule; no trailing separator is emitted.
/// Missing results produce a `> No result` line.
pub fn render_result_markdown_section(result: Option<&ToolResult>) -> Vec<String> {
    let mut lines = Vec::new();
    let result = match result {
        Some(r) => r,
        None => {
            lines.push("> No result".to_string());
            return lines;
        }
    };
    let pieces = collect_result_pieces(result);
    if pieces.is_empty() {
        return lines;
    }
    if result.is_error {
        lines.push("> **Error**".to_string());
    } else {
        lines.push("**Result**".to_string());
    }
    lines.push(String::new());
    lines.push(pieces.join("\n\n---\n\n"));
    lines
}

/**
 * Default formatter for frontend markdown rendering.
 *
 * - Scalar arguments render as inline code; object/array arguments render as
 *   fenced JSON blocks.
 * - Result content pieces are separated by horizontal rules (no trailing
 *   rule). The header (tool name + status) is rendered by the surrounding UI,
 *   so it is not emitted here.
 */
pub fn default_format_to_markdown(name: &str, arguments: &Value, result: Option<&ToolResult>) -> String {
    let _ = name;
    let mut lines: Vec<String> = Vec::new();

    if let Some(obj) = arguments.as_object() {
        if !obj.is_empty() {
            for (key, value) in obj {
                match render_arg_inline(value) {
                    Some(inline) => lines.push(format!("**{}**: {}", escape_inline(key), inline)),
                    None => {
                        lines.push(format!("**{}**:", escape_inline(key)));
                        let json = serde_json::to_string_pretty(value).unwrap_or_default();
                        lines.push(format!("```json\n{json}\n```"));
                    }
                }
            }
            lines.push(String::new());
        }
    }

    lines.extend(render_result_markdown_section(result));
    lines.join("\n")
}

/// Extract the first non-empty text content piece of a result, if any.
pub fn first_text(result: &ToolResult) -> Option<&str> {
    for c in &result.content {
        if let ToolContent::Text { text } = c {
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/**
 * Default formatter for LLM-friendly plain text.
 *
 * Produces a compact, plain representation: a bracketed status header followed
 * by the result content. Arguments are intentionally omitted — the model
 * already has them from its own tool-call — to save tokens and avoid noise.
 */
pub fn default_format_to_text(name: &str, _arguments: &Value, result: Option<&ToolResult>) -> String {
    let status_label = match result {
        None => return format!("[{name}] no result"),
        Some(r) if r.is_error => "error",
        Some(_) => "success",
    };
    let result = result.expect("checked above");
    let body: Vec<String> = collect_result_pieces(result);
    if body.is_empty() {
        format!("[{name}] {status_label}")
    } else {
        format!("[{name}] {status_label}\n{}", body.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wisp_common::ToolResult;

    fn result(text: &str, is_error: bool) -> ToolResult {
        ToolResult {
            content: vec![ToolContent::Text { text: text.to_string() }],
            is_error,
        }
    }

    #[test]
    fn markdown_renders_scalar_argument_inline() {
        let args = serde_json::json!({"location": "Hangzhou"});
        let out = default_format_to_markdown("get_weather", &args, Some(&result("Sunny, 28°C", false)));
        assert!(out.contains("**location**: `Hangzhou`"));
        assert!(!out.contains("```json\nHangzhou\n```"), "scalars must not be fenced");
        assert!(out.contains("**Result**"));
        assert!(out.contains("Sunny, 28°C"));
    }

    #[test]
    fn markdown_renders_object_argument_as_fenced_json() {
        let args = serde_json::json!({"options": {"unit": "metric"}, "q": "weather"});
        let out = default_format_to_markdown("t", &args, Some(&result("ok", false)));
        assert!(out.contains("**options**:"));
        assert!(out.contains("```json"));
        assert!(out.contains("\"unit\": \"metric\""));
        assert!(out.contains("**q**: `weather`"));
    }

    #[test]
    fn markdown_renders_missing_result_as_no_result() {
        let args = serde_json::json!({"location": "Hangzhou"});
        let out = default_format_to_markdown("get_weather", &args, None);
        assert!(out.contains("> No result"));
        assert!(out.contains("`Hangzhou`"));
    }

    #[test]
    fn markdown_renders_image_and_resource_placeholders() {
        let r = ToolResult {
            content: vec![
                ToolContent::Text { text: "see".to_string() },
                ToolContent::Image {
                    data: "x".to_string(),
                    mime_type: "image/png".to_string(),
                },
                ToolContent::Resource {
                    uri: "file:///a".to_string(),
                    mime_type: None,
                    text: None,
                    blob: None,
                },
            ],
            is_error: false,
        };
        let out = default_format_to_markdown("t", &serde_json::json!({}), Some(&r));
        assert!(out.contains("_[image]_"));
        assert!(out.contains("_[resource: file:///a]_"));
    }

    #[test]
    fn markdown_does_not_emit_trailing_separator() {
        let args = serde_json::json!({"q": "x"});
        let out = default_format_to_markdown("t", &args, Some(&result("body", false)));
        assert!(!out.ends_with("---"));
        let trimmed = out.trim_end();
        assert!(!trimmed.ends_with("---"));
    }

    #[test]
    fn markdown_separates_multiple_result_pieces() {
        let r = ToolResult {
            content: vec![
                ToolContent::Text { text: "first".to_string() },
                ToolContent::Text { text: "second".to_string() },
            ],
            is_error: false,
        };
        let out = default_format_to_markdown("t", &serde_json::json!({}), Some(&r));
        assert!(out.contains("first\n\n---\n\nsecond"));
    }

    #[test]
    fn text_omits_arguments_and_uses_plain_header() {
        let args = serde_json::json!({"location": "Hangzhou"});
        let out = default_format_to_text("get_weather", &args, Some(&result("Sunny, 28°C", false)));
        assert!(out.starts_with("[get_weather] success"));
        assert!(out.contains("Sunny, 28°C"));
        assert!(!out.contains("Hangzhou"), "arguments must not be echoed");
        assert!(!out.contains("🧰"), "no emoji in LLM text");
    }

    #[test]
    fn text_renders_error_status() {
        let args = serde_json::json!({"x": 1});
        let out = default_format_to_text("broken_tool", &args, Some(&result("boom", true)));
        assert!(out.starts_with("[broken_tool] error"));
        assert!(out.contains("boom"));
    }

    #[test]
    fn text_renders_missing_result() {
        let out = default_format_to_text("noop", &serde_json::json!({}), None);
        assert_eq!(out, "[noop] no result");
    }
}
