use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use wisp_configs::{ConversationLoopConfig, PipelineConfig};

use crate::types::AppData;

// ========== Pipeline Config ==========

#[tauri::command]
pub async fn settings_get_pipeline_config(app_handle: AppHandle) -> Result<PipelineConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config_manager.get_pipeline_config())
}

#[tauri::command]
pub async fn settings_update_pipeline_config(
    app_handle: AppHandle,
    config: PipelineConfig,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state
        .config_manager
        .update_pipeline_config(config)
        .map_err(|e| e.to_string())
}

// ========== Conversation Config ==========

#[tauri::command]
pub async fn settings_get_conversation_config(
    app_handle: AppHandle,
) -> Result<ConversationLoopConfig, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config_manager.get_conversation_config())
}

#[tauri::command]
pub async fn settings_update_conversation_config(
    app_handle: AppHandle,
    config: ConversationLoopConfig,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().map_err(|e| e.to_string())?;
    state
        .config_manager
        .update_conversation_config(config)
        .map_err(|e| e.to_string())
}
