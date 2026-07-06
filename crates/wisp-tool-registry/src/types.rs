use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_input_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub requires_confirmation: bool,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAnnotations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

pub fn registered_name(server_id: &str, tool_name: &str) -> String {
    let clean = |s: &str| -> String {
        let mut out = String::new();
        let mut prev_underscore = false;
        for ch in s.chars() {
            let mapped = if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            };
            if mapped == '_' {
                if !prev_underscore {
                    out.push('_');
                    prev_underscore = true;
                }
            } else {
                out.push(mapped.to_ascii_lowercase());
                prev_underscore = false;
            }
        }
        out.trim_matches('_').to_string()
    };

    let left = clean(server_id);
    let right = clean(tool_name);
    match (left.is_empty(), right.is_empty()) {
        (true, true) => "mcp_".to_string(),
        (false, true) => format!("mcp_{left}"),
        (true, false) => format!("mcp_{right}"),
        (false, false) => format!("mcp_{left}_{right}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_name_normal() {
        assert_eq!(registered_name("server", "tool"), "mcp_server_tool");
    }

    #[test]
    fn registered_name_special_chars() {
        assert_eq!(registered_name("my-server", "my.tool"), "mcp_my_server_my_tool");
    }

    #[test]
    fn registered_name_uppercase() {
        assert_eq!(registered_name("ServerA", "ToolB"), "mcp_servera_toolb");
    }

    #[test]
    fn registered_name_empty() {
        assert_eq!(registered_name("", ""), "mcp_");
    }

    #[test]
    fn registered_name_empty_server() {
        assert_eq!(registered_name("", "tool"), "mcp_tool");
    }

    #[test]
    fn registered_name_empty_tool() {
        assert_eq!(registered_name("server", ""), "mcp_server");
    }
}
