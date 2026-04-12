use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;
use anyhow::Result;

use super::client::McpStdioClient;
use super::super::mcp::types::{ServerConfig, ConnectionStatus};

pub struct McpStdioManager {
    clients: Arc<Mutex<HashMap<String, McpStdioClient>>>,
    statuses: Arc<Mutex<HashMap<String, ConnectionStatus>>>,
}

impl McpStdioManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            statuses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect_server(&self, config: &ServerConfig) -> Result<()> {
        // 检查是否已连接
        {
            let clients = self.clients.lock().await;
            if clients.contains_key(&config.id) {
                return Ok(());
            }
        }

        // 更新状态为连接中
        self.update_status(&config.id, ConnectionStatus {
            server_id: config.id.clone(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
        }).await;

        // 根据传输类型创建客户端
        match &config.transport {
            super::super::mcp::types::TransportConfig::Stdio { command, args, env: _, cwd: _ } => {
                let args = args.clone();
                let mut client = McpStdioClient::spawn(
                    config.id.clone(),
                    command,
                    &args,
                ).await?;

                // 初始化 MCP 连接
                let _init_result = client.initialize().await?;
                println!("[McpStdioManager] Server {} initialized", config.id);

                // 保存客户端
                {
                    let mut clients = self.clients.lock().await;
                    clients.insert(config.id.clone(), client);
                }

                // 更新状态为已连接
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

                Ok(())
            }
            _ => {
                anyhow::bail!("Transport type not supported by stdio manager")
            }
        }
    }

    pub async fn disconnect_server(&self, server_id: &str) -> Result<()> {
        let mut clients = self.clients.lock().await;
        if let Some(mut client) = clients.remove(server_id) {
            client.kill().await?;
        }

        self.update_status(server_id, ConnectionStatus {
            server_id: server_id.to_string(),
            connected: false,
            last_ping_at: None,
            reconnect_attempts: 0,
            error: None,
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

    async fn update_status(&self, server_id: &str, status: ConnectionStatus) {
        let mut statuses = self.statuses.lock().await;
        statuses.insert(server_id.to_string(), status);
    }

    pub async fn is_connected(&self, server_id: &str) -> bool {
        let clients = self.clients.lock().await;
        clients.contains_key(server_id)
    }
}

impl Default for McpStdioManager {
    fn default() -> Self {
        Self::new()
    }
}
