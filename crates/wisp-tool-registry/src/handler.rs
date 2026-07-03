use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError>;
}
