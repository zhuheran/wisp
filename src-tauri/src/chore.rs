use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, CreateChatCompletionRequestArgs,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Runtime};

pub fn parse_display_names(raw: &str) -> HashMap<String, String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            let start = trimmed.find('[');
            let end = trimmed.rfind(']');
            match (start, end) {
                (Some(s), Some(e)) if s < e => match serde_json::from_str(&trimmed[s..=e]) {
                    Ok(v) => v,
                    Err(_) => return HashMap::new(),
                },
                _ => return HashMap::new(),
            }
        }
    };

    let arr = match parsed.as_array() {
        Some(a) => a,
        None => return HashMap::new(),
    };

    let mut out = HashMap::new();
    for entry in arr {
        let name = entry.get("name").and_then(|v| v.as_str());
        let display = entry
            .get("display_name")
            .or_else(|| entry.get("displayName"))
            .and_then(|v| v.as_str());
        if let (Some(name), Some(display)) = (name, display) {
            if !name.is_empty() && !display.is_empty() {
                out.insert(name.to_string(), display.trim().to_string());
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDisplayNameInput {
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
}

pub async fn chore_complete<R: Runtime>(
    app_handle: &AppHandle<R>,
    system: &str,
    user: &str,
) -> Result<String, String> {
    use std::sync::Mutex;
    use crate::types::AppData;
    use crate::key_manager::KeyManager;

    let (provider, model, base_url) = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        let chore = state
            .config_manager
            .get_chore_llm()
            .ok_or_else(|| "Chore LLM not configured".to_string())?;
        let provider = state
            .config_manager
            .get_provider(&chore.provider)
            .ok_or_else(|| format!("Provider '{}' not found", chore.provider))?;
        let base_url = provider.base_url.clone();
        (provider, chore.model, base_url)
    };

    let key_manager = KeyManager::new("wisp".to_string());
    let api_key = key_manager
        .get_api_key(&provider.name)
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|e| format!("Failed to resolve API key: {e}"))?;

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);
    let client = Client::with_config(config);

    let messages: Vec<ChatCompletionRequestMessage> = vec![
        serde_json::from_value(serde_json::json!({
            "role": "system", "content": system
        }))
        .map_err(|e| format!("message build error: {e}"))?,
        serde_json::from_value(serde_json::json!({
            "role": "user", "content": user
        }))
        .map_err(|e| format!("message build error: {e}"))?,
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model(model.clone())
        .messages(messages)
        .temperature(0.0)
        .max_tokens(2048_u32)
        .build()
        .map_err(|e| format!("request build error: {e}"))?;

    let response = client
        .chat()
        .create(request)
        .await
        .map_err(|e| format!("chore completion failed for model '{model}': {e}"))?;

    let text = response
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();
    Ok(text)
}

const DISPLAY_NAME_SYSTEM: &str = "You generate concise, human-friendly display names for MCP tools. Reply ONLY with a JSON array, no prose. Each element: {\"name\": <original tool name>, \"display_name\": <string>}. The display_name MUST follow the structure: `<ServerName> <Verb> <Noun>` where ServerName is the provided server name (one word, Title Case), Verb is one word (Title Case), and Noun may be one or more words (Title Case). Example inputs server='filesystem', name='read_file' -> 'Filesystem Read File'.";

const DISPLAY_NAME_USER_TEMPLATE: &str = "Generate a display name for each tool. Tools:\n";

#[tauri::command]
pub async fn mcp_generate_tool_display_names(
    app_handle: AppHandle,
    tools: Vec<ToolDisplayNameInput>,
) -> Result<HashMap<String, String>, String> {
    if tools.is_empty() {
        return Ok(HashMap::new());
    }

    let payload: Vec<Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "server": t.server_name,
                "name": t.tool_name,
                "description": t.description.clone().unwrap_or_default(),
            })
        })
        .collect();

    let user = format!(
        "{}{}",
        DISPLAY_NAME_USER_TEMPLATE,
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let raw = match chore_complete(&app_handle, DISPLAY_NAME_SYSTEM, &user).await {
        Ok(text) => text,
        Err(e) => {
            eprintln!("[chore] display-name generation failed: {e}");
            return Ok(HashMap::new());
        }
    };

    Ok(parse_display_names(&raw))
}

#[cfg(test)]
mod tests {
    use super::parse_display_names;
    use std::collections::HashMap;

    #[test]
    fn parses_well_formed_array() {
        let raw = r#"[
            {"name":"read_file","display_name":"Filesystem Read File"},
            {"name":"create_issue","display_name":"Github Create Issue"}
        ]"#;
        let mut expected = HashMap::new();
        expected.insert("read_file".to_string(), "Filesystem Read File".to_string());
        expected.insert("create_issue".to_string(), "Github Create Issue".to_string());
        assert_eq!(parse_display_names(raw), expected);
    }

    #[test]
    fn strips_markdown_code_fences() {
        let raw = "```json\n[{\"name\":\"x\",\"display_name\":\"X Do Thing\"}]\n```";
        let map = parse_display_names(raw);
        assert_eq!(map.get("x"), Some(&"X Do Thing".to_string()));
    }

    #[test]
    fn returns_empty_for_malformed() {
        assert!(parse_display_names("not json at all").is_empty());
        assert!(parse_display_names("[{").is_empty());
    }

    #[test]
    fn skips_invalid_entries_keeps_valid() {
        let raw = r#"[
            {"name":"good","display_name":"Good Do Thing"},
            {"name":"bad"},
            {"display_name":"No Name"}
        ]"#;
        let map = parse_display_names(raw);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("good"), Some(&"Good Do Thing".to_string()));
    }

    #[test]
    fn accepts_camel_case_key() {
        let raw = r#"[{"name":"x","displayName":"X Do Thing"}]"#;
        let map = parse_display_names(raw);
        assert_eq!(map.get("x"), Some(&"X Do Thing".to_string()));
    }

    #[test]
    fn extracts_array_despite_leading_prose() {
        let raw = "Here are the display names:\n[\n  {\"name\":\"read_file\",\"display_name\":\"Filesystem Read File\"}\n]\nLet me know if you need more.";
        let map = parse_display_names(raw);
        assert_eq!(map.get("read_file"), Some(&"Filesystem Read File".to_string()));
    }

    #[test]
    fn handles_uppercase_code_fence() {
        let raw = "```JSON\n[{\"name\":\"x\",\"display_name\":\"X Do Thing\"}]\n```";
        let map = parse_display_names(raw);
        assert_eq!(map.get("x"), Some(&"X Do Thing".to_string()));
    }
}
