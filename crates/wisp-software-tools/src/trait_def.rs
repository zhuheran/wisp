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
}
