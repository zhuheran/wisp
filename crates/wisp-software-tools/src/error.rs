#[derive(Debug, thiserror::Error)]
pub enum NativeToolError {
    #[error("argument validation failed: {0}")]
    Validation(String),
    #[error("execution error: {0}")]
    Runtime(String),
    #[error("permission denied: {0}")]
    Permission(String),
}

impl From<NativeToolError> for wisp_common::ToolError {
    fn from(e: NativeToolError) -> Self {
        wisp_common::ToolError::ExecutionFailed(e.to_string())
    }
}
