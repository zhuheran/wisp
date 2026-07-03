use reqwest_sse::Event;

use crate::error::LlmError;

pub fn parse_data_json(event: &Event) -> Result<serde_json::Value, LlmError> {
    if event.data == "[DONE]" {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&event.data).map_err(|e| LlmError::Sse(format!("JSON parse failed: {e}")))
}

pub fn is_done(event: &Event) -> bool {
    event.data == "[DONE]"
}
