use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

pub struct AbortRegistry {
    tokens: Mutex<HashMap<String, CancellationToken>>,
}

impl AbortRegistry {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, stream_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.tokens
            .lock()
            .unwrap()
            .insert(stream_id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, stream_id: &str) -> bool {
        if let Some(token) = self.tokens.lock().unwrap().remove(stream_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn unregister(&self, stream_id: &str) {
        self.tokens.lock().unwrap().remove(stream_id);
    }
}

impl Default for AbortRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
pub async fn conversation_abort(
    app_handle: AppHandle,
    stream_id: String,
) -> Result<bool, String> {
    let registry = app_handle.state::<AbortRegistry>();
    Ok(registry.cancel(&stream_id))
}
