use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use wisp_common::{ToolContent, ToolError, ToolResult};

use crate::format_result::render_result_markdown_section;
use crate::trait_def::NativeTool;

#[derive(Deserialize, JsonSchema)]
pub struct JsExecArgs {
    /// JavaScript code to evaluate.
    pub code: String,
}

pub struct JsExec;

#[async_trait]
impl NativeTool for JsExec {
    fn name(&self) -> &str {
        "wisp_js_exec"
    }

    fn description(&self) -> &str {
        "Execute JavaScript code in a sandboxed QuickJS runtime. No filesystem or network access. Instruction: the code runs inside a function body — use a return statement to produce output. Objects and arrays are auto-serialized to JSON by the tool, so do not wrap results in JSON.stringify yourself."
    }

    fn schema(&self) -> Value {
        let schema = schemars::schema_for!(JsExecArgs);
        serde_json::to_value(&schema).unwrap_or_default()
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    /// Render the `code` argument as a JavaScript block (the generic formatter
    /// would mislabel it as JSON) and the result underneath.
    fn format_to_markdown(
        &self,
        _name: &str,
        arguments: &Value,
        result: Option<&ToolResult>,
    ) -> String {
        let code = arguments.get("code").and_then(|v| v.as_str()).unwrap_or("");
        let mut lines: Vec<String> = Vec::new();
        lines.push("**code**:".to_string());
        lines.push(format!("```js\n{code}\n```"));
        lines.push(String::new());
        lines.extend(render_result_markdown_section(result));
        lines.join("\n")
    }

    async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
        let args: JsExecArgs = serde_json::from_value(args)
            .map_err(|e| ToolError::ExecutionFailed(format!("invalid arguments: {e}")))?;

        let code = args.code;
        let result = tokio::task::spawn_blocking(move || -> std::result::Result<String, String> {
            let runtime = rquickjs::Runtime::new().map_err(|e| e.to_string())?;
            let ctx = rquickjs::Context::full(&runtime).map_err(|e| e.to_string())?;
            let output = ctx
                .with(|ctx| -> std::result::Result<String, String> {
                    let wrapper = format!(
                        "(function() {{\ntry {{\nvar __wisp_r = (function() {{\n{code}\n}})();\nif (typeof __wisp_r === 'object' && __wisp_r !== null) return JSON.stringify(__wisp_r);\nreturn String(__wisp_r);\n}} catch(e) {{\nreturn '__wisp_error__:' + (e && e.message ? e.message : String(e));\n}}\n}})()",
                    );
                    let result: String = ctx
                        .eval(wrapper)
                        .map_err(|e| format!("syntax/runtime error: {e}"))?;
                    Ok(result)
                })?;
            Ok(output)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed(format!("task join error: {e}")))
        .and_then(|r| r.map_err(|e| ToolError::ExecutionFailed(format!("JS error: {e}"))))?;

        let (text, is_error) = match result.strip_prefix("__wisp_error__:") {
            Some(msg) => (msg.to_string(), true),
            None => (result, false),
        };

        Ok(ToolResult { content: vec![ToolContent::Text { text }], is_error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_js_exec_markdown_renders_code_as_javascript() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return 1 + 2;"});
        let result = ToolResult {
            content: vec![ToolContent::Text { text: "3".to_string() }],
            is_error: false,
        };
        let out = tool.format_to_markdown("wisp_js_exec", &args, Some(&result));
        assert!(out.contains("```js"));
        assert!(!out.contains("```json"), "code must not be fenced as json");
        assert!(out.contains("return 1 + 2;"));
        assert!(out.contains("**Result**"));
        assert!(out.contains("3"));
    }

    #[tokio::test]
    async fn test_js_exec_simple_arithmetic() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return 1 + 2;"});
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content[0], ToolContent::Text { text: "3".to_string() });
    }

    #[tokio::test]
    async fn test_js_exec_string_manipulation() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return \"hello\".toUpperCase();"});
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content[0], ToolContent::Text { text: "HELLO".to_string() });
    }

    #[tokio::test]
    async fn test_js_exec_object_return() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return {a: 1, b: [2, 3]};"});
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("\"a\":1"));
        assert!(text.contains("\"b\":[2,3]"));
    }

    #[tokio::test]
    async fn test_js_exec_no_return_undefined() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "const x = 1;"});
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "undefined");
    }

    #[tokio::test]
    async fn test_js_exec_syntax_error_returns_error() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return function({"});
        let result = tool.run(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_js_exec_runtime_error_returns_specific_message() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return undefinedVar;"});
        let result = tool.run(args).await.unwrap();
        assert!(result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(
            text.contains("undefinedVar"),
            "error should mention the undefined variable, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_js_exec_type_error_message() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return null.foo;"});
        let result = tool.run(args).await.unwrap();
        assert!(result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text.as_str(),
            _ => panic!("expected text"),
        };
        assert!(
            text.contains("null") || text.contains("Cannot read"),
            "error should explain the type error, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_js_exec_requires_confirmation() {
        let tool = JsExec;
        assert!(tool.requires_confirmation());
    }

    #[tokio::test]
    async fn test_js_exec_multiline_with_variables() {
        let tool = JsExec;
        let args = serde_json::json!({
            "code": "const greeting = \"Hello\";\nconst nums = [1, 2, 3];\nreturn { message: greeting, doubled: nums.map(n => n * 2) };"
        });
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("\"message\":\"Hello\""));
        assert!(text.contains("\"doubled\":[2,4,6]"));
    }

    #[tokio::test]
    async fn test_js_exec_null_return() {
        let tool = JsExec;
        let args = serde_json::json!({"code": "return null;"});
        let result = tool.run(args).await.unwrap();
        assert!(!result.is_error);
        let text = match &result.content[0] {
            ToolContent::Text { text } => text,
            _ => panic!("expected text"),
        };
        assert_eq!(text, "null");
    }
}
