use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wisp_common::{ToolError, ToolResult};
use wisp_tool_registry::{ToolDefinition, ToolHandler, ToolRegistry};

use crate::trait_def::NativeTool;

struct NativeToolAdapter {
    tool: Arc<dyn NativeTool>,
}

#[async_trait]
impl ToolHandler for NativeToolAdapter {
    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        self.tool.run(args).await
    }
}

pub struct SoftwareToolRegistry {
    tools: HashMap<String, Arc<dyn NativeTool>>,
}

impl SoftwareToolRegistry {
    pub fn new() -> Self {
        SoftwareToolRegistry {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: NativeTool + 'static>(&mut self, tool: T) -> &mut Self {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn NativeTool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| build_definition(t)).collect()
    }

    pub fn register_into(&self, registry: &ToolRegistry) {
        for tool in self.tools.values() {
            let definition = build_definition(tool);
            let handler = Arc::new(NativeToolAdapter { tool: Arc::clone(tool) }) as Arc<dyn ToolHandler>;
            registry.register(definition, handler, tool.default_allowed_pals());
        }
    }
}

fn build_definition(tool: &Arc<dyn NativeTool>) -> ToolDefinition {
    ToolDefinition {
        name: tool.name().to_string(),
        description: Some(tool.description().to_string()),
        input_schema: tool.schema(),
        annotations: None,
        metadata: std::collections::HashMap::from([(
            "provider".to_string(),
            Value::String("native".to_string()),
        )]),
        requires_confirmation: tool.requires_confirmation(),
    }
}

impl Default for SoftwareToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use wisp_common::ToolContent;

    struct EchoTool;

    #[derive(Deserialize, JsonSchema)]
    struct EchoArgs {
        msg: String,
    }

    #[async_trait]
    impl NativeTool for EchoTool {
        fn name(&self) -> &str {
            "test_echo"
        }
        fn description(&self) -> &str {
            "Echo back the message"
        }
        fn schema(&self) -> serde_json::Value {
            schemars::schema_for!(EchoArgs).into()
        }
        async fn run(&self, args: Value) -> Result<ToolResult, ToolError> {
            let args: EchoArgs =
                serde_json::from_value(args).map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            Ok(ToolResult {
                content: vec![ToolContent::Text { text: args.msg }],
                is_error: false,
            })
        }
    }

    #[test]
    fn test_register_and_list_definitions() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "test_echo");
        assert!(!defs[0].requires_confirmation);
    }

    #[test]
    fn test_register_into_populates_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        assert!(registry.get_tool("test_echo").is_some());
        assert!(registry.enabled_set().contains("test_echo"));
    }

    #[tokio::test]
    async fn test_execute_via_registry() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let registry = ToolRegistry::new();
        sw.register_into(&registry);
        let result = registry
            .execute("test_echo", serde_json::json!({"msg": "hello"}), None)
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.is_error);
        assert_eq!(
            result.content[0],
            ToolContent::Text { text: "hello".to_string() }
        );
    }

    #[test]
    fn test_definition_has_schema() {
        let mut sw = SoftwareToolRegistry::new();
        sw.register(EchoTool);
        let defs = sw.list_definitions();
        let schema = &defs[0].input_schema;
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("msg").is_some());
    }
}
