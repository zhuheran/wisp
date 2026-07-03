mod cache;
mod commands;
mod chore;
mod image;
mod inet;
mod types;
mod conversation_commands;
mod orchestrator;
mod mcp_commands;
mod mcp_http_commands;
mod mcp_stdio_commands;
mod registry_commands;

use tauri::{Builder, Manager};

use wisp_db::chat::Chat;
use crate::cache::DiagramCache;
use wisp_keyring::KeyManager;
use wisp_configs::ConfigManager;
use wisp_mcp::McpConfigManager;
use wisp_mcp::TransportConfig;
use wisp_mcp::McpStdioManager;
use wisp_mcp::McpHttpManager;
use wisp_mcp::ToolRegistry;
use std::collections::HashMap;
use std::sync::Mutex;
use crate::types::AppData;


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

			let servers = mcp_config_manager.get_all_servers();
			let stdio_manager = std::sync::Arc::clone(&mcp_stdio_manager);
			let http_manager = std::sync::Arc::clone(&mcp_http_manager);

			        let tool_registry = std::sync::Arc::new(ToolRegistry::new(
			            std::sync::Arc::clone(&mcp_stdio_manager),
			            std::sync::Arc::clone(&mcp_http_manager),
			        ));

			                app.manage(Mutex::new(AppData {
			            chat: Chat::new(app.handle())?,
			            diagram_cache: DiagramCache::new()?,
			            key_manager: KeyManager::new("wisp".to_string()),
			            config_manager,
			            mcp_config_manager,
			            mcp_stdio_manager,
			            mcp_http_manager,
			            tool_registry,
			            unlocked_pals: HashMap::new(),
			        }));

			{
				let state = app.state::<Mutex<AppData>>();
				let state = state.lock().unwrap();
				state.mcp_stdio_manager.set_app_handle(app.handle().clone());
				state.mcp_http_manager.set_app_handle(app.handle().clone());
			}

			tauri::async_runtime::spawn(async move {
				for server in servers {
					let _ = match &server.transport {
						TransportConfig::Stdio { .. } => {
							stdio_manager.connect_server(&server).await
						}
						TransportConfig::Sse { .. } | TransportConfig::Http { .. } => {
							http_manager.connect_server(&server).await
						}
					};
				}
			});
			Ok(())
		})
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
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
			commands::conversation_set_default_responder,
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
			commands::configs_get_default_responder,
			commands::configs_set_default_responder,
			commands::configs_get_chore_llm,
			commands::configs_set_chore_llm,
			// MCP commands
			mcp_commands::mcp_get_servers,
			mcp_commands::mcp_get_server,
			mcp_commands::mcp_add_server,
			mcp_commands::mcp_update_server,
			mcp_commands::mcp_remove_server,
			mcp_commands::mcp_get_pipeline_config,
			mcp_commands::mcp_update_pipeline_config,
			mcp_commands::mcp_get_conversation_config,
			mcp_commands::mcp_update_conversation_config,
			mcp_commands::mcp_save_session,
			mcp_commands::mcp_load_session,
			mcp_commands::mcp_delete_session,
			mcp_commands::mcp_list_sessions,
			            // Registry commands
			            registry_commands::registry_list_tools,
			            registry_commands::registry_execute,
			            registry_commands::registry_set_enabled,
			            registry_commands::registry_refresh,
			chore::mcp_generate_tool_display_names,
			// Image commands
			image::compress_image,
			image::get_image_info,
			// MCP stdio commands
			mcp_stdio_commands::mcp_stdio_connect,
			mcp_stdio_commands::mcp_stdio_disconnect,
			mcp_stdio_commands::mcp_stdio_get_status,
			mcp_stdio_commands::mcp_stdio_get_all_statuses,
			mcp_stdio_commands::mcp_stdio_list_tools,
			mcp_stdio_commands::mcp_stdio_call_tool,
			mcp_stdio_commands::mcp_stdio_is_connected,
			// MCP http commands
			mcp_http_commands::mcp_http_connect,
			mcp_http_commands::mcp_http_disconnect,
			mcp_http_commands::mcp_http_get_status,
			mcp_http_commands::mcp_http_get_all_statuses,
			mcp_http_commands::mcp_http_list_tools,
			mcp_http_commands::mcp_http_call_tool,
			mcp_http_commands::mcp_http_is_connected,
			conversation_commands::conversation_send_message,
			conversation_commands::conversation_regenerate_message,
			conversation_commands::conversation_derive_message,
			conversation_commands::conversation_edit_and_regenerate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
