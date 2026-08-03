mod abort;
mod cache;
mod chore;
mod commands;
mod conversation_commands;
mod image;
mod inet;
mod mcp_commands;
mod mcp_http_commands;
mod mcp_stdio_commands;
mod native_tools;
mod orchestrator;
mod registry_commands;
mod settings_commands;
mod skills_commands;
mod types;

use tauri::{Builder, Manager};

use crate::abort::AbortRegistry;

use crate::cache::DiagramCache;
use crate::types::AppData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wisp_configs::ConfigManager;
use wisp_db::chat::Chat;
use wisp_mcp::McpConfigManager;
use wisp_mcp::McpHttpManager;
use wisp_mcp::McpStdioManager;
use wisp_mcp::TransportConfig;
use wisp_software_tools::SoftwareToolRegistry;
use wisp_tool_registry::ToolRegistry;

#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    Builder::default()
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            #[cfg(target_os = "macos")]
            apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None)
                .expect("Unsupported platform! 'apply_vibrancy' is only supported on macOS");

            let config_manager = Arc::new(ConfigManager::new(app.handle())?);
            let mcp_config_manager = McpConfigManager::new(app.handle())?;
            let mcp_stdio_manager = std::sync::Arc::new(McpStdioManager::new());
            let mcp_http_manager = std::sync::Arc::new(McpHttpManager::new());

            // set all fields of AppData to default values if they are None
            config_manager.save().expect("Failed to save config");

            let servers = mcp_config_manager.get_all_servers();
            let stdio_manager = std::sync::Arc::clone(&mcp_stdio_manager);
            let http_manager = std::sync::Arc::clone(&mcp_http_manager);

            let tool_registry = std::sync::Arc::new(ToolRegistry::new());

            // Scan the skills directories (app-owned + ~/.agents/skills) and
            // register the load_skill tool before AppData is managed so the
            // conversation path sees them immediately. Newly scanned skills
            // default to enabled.
            let (skills, skill_errors) = {
                let dirs = skills_commands::skills_dirs(app.handle())?;
                skills_commands::load_skills_from_dirs(&dirs)
            };
            let enabled_skills: std::collections::HashSet<String> =
                skills.iter().map(|s| s.name.clone()).collect();
            skills_commands::resync_registry(&tool_registry, &skills, &enabled_skills);
            if !skill_errors.is_empty() {
                eprintln!("[skills] {} skill(s) failed to load:", skill_errors.len());
                for (name, err) in &skill_errors {
                    eprintln!("  - {name}: {err}");
                }
            }

            let software_registry = {
                let mut software_registry = SoftwareToolRegistry::new();
                software_registry.register(native_tools::ConfigRead::new(std::sync::Arc::clone(
                    &config_manager,
                )));
                software_registry.register(native_tools::ConfigWrite::new(std::sync::Arc::clone(
                    &config_manager,
                )));
                software_registry.register(wisp_software_tools::JsExec);
                software_registry.register_into(&tool_registry);
                std::sync::Arc::new(software_registry)
            };

            app.manage(Mutex::new(AppData {
                chat: Chat::new(app.handle())?,
                diagram_cache: DiagramCache::new()?,
                config_manager,
                mcp_config_manager,
                mcp_stdio_manager,
                mcp_http_manager,
                tool_registry,
                software_registry,
                unlocked_pals: HashMap::new(),
                skills,
                enabled_skills,
            }));

            app.manage(AbortRegistry::new());

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
                        },
                        TransportConfig::Sse { .. } | TransportConfig::Http { .. } => {
                            http_manager.connect_server(&server).await
                        },
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
            commands::get_thread_decisions,
            commands::set_thread_decisions,
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
            mcp_commands::mcp_save_session,
            mcp_commands::mcp_load_session,
            mcp_commands::mcp_delete_session,
            mcp_commands::mcp_list_sessions,
            // Settings commands
            settings_commands::settings_get_pipeline_config,
            settings_commands::settings_update_pipeline_config,
            settings_commands::settings_get_conversation_config,
            settings_commands::settings_update_conversation_config,
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
            conversation_commands::format_tool_call_markdown,
            abort::conversation_abort,
            // Skills commands
            skills_commands::skills_list,
            skills_commands::skills_refresh,
            skills_commands::skills_toggle,
            skills_commands::skills_open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
