use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};

#[async_trait]
pub trait NativeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn requires_confirmation(&self) -> bool {
        false
    }
    fn default_allowed_pals(&self) -> Vec<String> {
        vec![]
    }
    async fn run(&self, args: Value) -> Result<ToolResult, ToolError>;

    /// Produce formatted markdown for a tool call result, suitable for
    /// frontend markdown rendering. Defaults to the generic formatter; native
    /// tools may override to customise how their output is displayed.
    fn format_to_markdown(
        &self,
        name: &str,
        arguments: &Value,
        result: Option<&ToolResult>,
    ) -> String {
        let _ = name;
        crate::format_result::default_format_to_markdown(name, arguments, result)
    }

    /// Produce LLM-friendly plain text for a tool call result, suitable for
    /// feeding back into a model context. Defaults to the generic formatter;
    /// native tools may override to customise how their output is presented to
    /// the model.
    fn format_to_text(
        &self,
        name: &str,
        arguments: &Value,
        result: Option<&ToolResult>,
    ) -> String {
        crate::format_result::default_format_to_text(name, arguments, result)
    }
}
