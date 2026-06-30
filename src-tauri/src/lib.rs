mod api;
mod cache;
mod commands;
mod configs;
mod db;
mod utils;
mod inet;
mod key_manager;
mod mcp;
mod mcp_stdio;
mod mcp_http;
mod image;
mod conversation;
use tauri::{Builder, Manager};
use db::chat::Chat;
use cache::DiagramCache;
use key_manager::KeyManager;
use configs::ConfigManager;
use mcp::commands::McpConfigManager;
use mcp_stdio::McpStdioManager;
use mcp_http::McpHttpManager;
use std::sync::Mutex;
mod types;
use types::AppData;

#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    Builder::default()
		.setup(|app| {
			let window = app.get_webview_window("main").unwrap();
			#[cfg(target_os = "macos")]
			apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None).expect("Unsupported platform! 'apply_vibrancy' is only supported on macOS");

			let config_manager = ConfigManager::new(app.handle())?;
			let mcp_config_manager = McpConfigManager::new(app.handle())?;
			let mcp_stdio_manager = std::sync::Arc::new(McpStdioManager::new());
			let mcp_http_manager = std::sync::Arc::new(McpHttpManager::new());

			// set all fields of AppData to default values if they are None
			config_manager.save().expect("Failed to save config");

			app.manage(Mutex::new(AppData {
				chat: Chat::new(app.handle())?,
				diagram_cache: DiagramCache::new()?,
				key_manager: KeyManager::new("wisp".to_string()),
				config_manager,
				mcp_config_manager,
				mcp_stdio_manager,
				mcp_http_manager,
			}));
			Ok(())
		})
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ask_openai_stream,
            // commands::get_cached_render,
            commands::hash_content,
            commands::put_cached_diagram,
			commands::get_cached_diagram,
			commands::clear_diagram_cache,
            commands::create_conversation,
            commands::add_message,
			commands::get_message,
			commands::update_message,
			commands::delete_message,
            commands::get_all_message_involved,
			commands::get_thread_tree,
            commands::delete_conversation,
            commands::list_conversations,
			commands::update_conversation_entry_id,
			commands::update_conversation,
			commands::get_url,
			commands::post_url,
			commands::set_api_key,
            commands::get_api_key,
            commands::delete_api_key,
            commands::configs_get_providers,
			commands::configs_get_provider,
			commands::configs_create_provider,
			commands::configs_update_provider,
			commands::configs_delete_provider,
			commands::configs_add_model,
			commands::configs_get_model,
			commands::configs_update_model,
			commands::configs_delete_model,
			commands::configs_get_characters,
			commands::configs_get_character,
			commands::configs_create_character,
			commands::configs_update_character,
			commands::configs_delete_character,
			// MCP commands
			mcp::mcp_get_servers,
			mcp::mcp_get_server,
			mcp::mcp_add_server,
			mcp::mcp_update_server,
			mcp::mcp_remove_server,
			mcp::mcp_get_pipeline_config,
			mcp::mcp_update_pipeline_config,
			mcp::mcp_get_conversation_config,
			mcp::mcp_update_conversation_config,
			mcp::mcp_save_session,
			mcp::mcp_load_session,
			mcp::mcp_delete_session,
			mcp::mcp_list_sessions,
			// Image commands
			image::compress_image,
			image::get_image_info,
			// MCP stdio commands
			mcp_stdio::mcp_stdio_connect,
			mcp_stdio::mcp_stdio_disconnect,
			mcp_stdio::mcp_stdio_get_status,
			mcp_stdio::mcp_stdio_get_all_statuses,
			mcp_stdio::mcp_stdio_list_tools,
			mcp_stdio::mcp_stdio_call_tool,
			mcp_stdio::mcp_stdio_is_connected,
			// MCP http commands
			mcp_http::mcp_http_connect,
			mcp_http::mcp_http_disconnect,
			mcp_http::mcp_http_get_status,
			mcp_http::mcp_http_get_all_statuses,
			mcp_http::mcp_http_list_tools,
			mcp_http::mcp_http_call_tool,
			mcp_http::mcp_http_is_connected,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
