use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;
use anyhow::Result;
use tauri::{AppHandle, Emitter};

use super::client::McpHttpClient;
use super::super::mcp::types::{ServerConfig, ConnectionStatus, TransportConfig};
use super::super::types::McpConnectionStatusEvent;


pub struct McpHttpManager {
    clients: Arc<Mutex<HashMap<String, McpHttpClient>>>,
    statuses: Arc<Mutex<HashMap<String, ConnectionStatus>>>,
    app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
}

impl McpHttpManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            statuses: Arc::new(Mutex::new(HashMap::new())),
            app_handle: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().unwrap() = Some(handle);
    }

    pub async fn connect_server(&self, config: &ServerConfig) -> Result<()> {
        {
            let clients = self.clients.lock().await;
            if clients.contains_key(&config.id) {
                return Ok(());
            }
        }

        self.update_status(&config.id, ConnectionStatus {
            server_id: config.id.clone(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
        }).await;

        self.emit_status(McpConnectionStatusEvent {
            server_id: config.id.clone(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
            transport_kind: "http".to_string(),
            source: "connecting".to_string(),
        }).await;

        let transport_kind = match &config.transport {
            TransportConfig::Sse { .. } => "sse",
            TransportConfig::Http { .. } => "http",
            _ => "http",
        };

        let connect_result = match &config.transport {
            TransportConfig::Sse { url, headers } => {
                McpHttpClient::new_sse(
                    config.id.clone(),
                    url.clone(),
                    headers.clone(),
                ).await
            }
            TransportConfig::Http { url, headers, session_id } => {
                McpHttpClient::new_http(
                    config.id.clone(),
                    url.clone(),
                    headers.clone(),
                    session_id.clone(),
                ).await
            }
            _ => {
                anyhow::bail!("Transport type not supported by HTTP manager")
            }
        };

        let client = match connect_result {
            Ok(client) => client,
            Err(e) => {
                self.emit_status(McpConnectionStatusEvent {
                    server_id: config.id.clone(),
                    connected: false,
                    last_ping_at: None,
                    reconnect_attempts: 0,
                    error: Some(e.to_string()),
                    transport_kind: transport_kind.to_string(),
                    source: "connect_failed".to_string(),
                }).await;
                return Err(e);
            }
        };

        if let Err(e) = client.initialize().await {
            self.emit_status(McpConnectionStatusEvent {
                server_id: config.id.clone(),
                connected: false,
                last_ping_at: None,
                reconnect_attempts: 0,
                error: Some(e.to_string()),
                transport_kind: transport_kind.to_string(),
                source: "connect_failed".to_string(),
            }).await;
            return Err(e);
        }
        println!("[McpHttpManager] Server {} initialized", config.id);

        {
            let mut clients = self.clients.lock().await;
            clients.insert(config.id.clone(), client);
        }

        self.update_status(&config.id, ConnectionStatus {
            server_id: config.id.clone(),
            connected: true,
            last_ping_at: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
            reconnect_attempts: 0,
            error: None,
        }).await;

        self.emit_status(McpConnectionStatusEvent {
            server_id: config.id.clone(),
            connected: true,
            last_ping_at: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()),
            reconnect_attempts: 0,
            error: None,
            transport_kind: transport_kind.to_string(),
            source: "connected".to_string(),
        }).await;

        Ok(())
    }

    pub async fn disconnect_server(&self, server_id: &str) -> Result<()> {
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.remove(server_id) {
            client.disconnect().await?;
        }

        self.update_status(server_id, ConnectionStatus {
            server_id: server_id.to_string(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
        }).await;

        self.emit_status(McpConnectionStatusEvent {
            server_id: server_id.to_string(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
            transport_kind: "http".to_string(),
            source: "disconnected".to_string(),
        }).await;

        Ok(())
    }

    pub async fn list_tools(&self, server_id: &str, cursor: Option<String>) -> Result<Value> {
        let clients = self.clients.lock().await;
        let client = clients.get(server_id)
            .ok_or_else(|| anyhow::anyhow!("Server {} not connected", server_id))?;

        client.list_tools(cursor).await
    }

    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<Value> {
        let clients = self.clients.lock().await;
        let client = clients.get(server_id)
            .ok_or_else(|| anyhow::anyhow!("Server {} not connected", server_id))?;

        client.call_tool(tool_name, arguments).await
    }

    pub async fn get_status(&self, server_id: &str) -> Option<ConnectionStatus> {
        let statuses = self.statuses.lock().await;
        statuses.get(server_id).cloned()
    }

    pub async fn get_all_statuses(&self) -> Vec<ConnectionStatus> {
        let statuses = self.statuses.lock().await;
        statuses.values().cloned().collect()
    }

    async fn emit_status(&self, event: McpConnectionStatusEvent) {
        if let Some(handle) = self.app_handle.lock().unwrap().as_ref() {
            let _ = handle.emit("mcp_status_updated", event);
        }
    }

    async fn update_status(&self, server_id: &str, status: ConnectionStatus) {
        let mut statuses = self.statuses.lock().await;
        statuses.insert(server_id.to_string(), status);
    }

    pub async fn is_connected(&self, server_id: &str) -> bool {
        let clients = self.clients.lock().await;
        clients.contains_key(server_id)
    }
}

impl Default for McpHttpManager {
    fn default() -> Self {
        Self::new()
    }
}
