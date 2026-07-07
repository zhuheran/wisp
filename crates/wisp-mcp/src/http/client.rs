use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;

pub enum HttpTransport {
    Sse,
    Http,
}

pub struct McpHttpClient {
    client: Client,
    url: String,
    headers: HashMap<String, String>,
    transport: HttpTransport,
    session_id: Arc<Mutex<Option<String>>>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    next_id: Arc<Mutex<i64>>,
    server_id: String,
    sse_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    message_url: Arc<Mutex<String>>,
}

impl McpHttpClient {
    fn build_client() -> Result<Client> {
        Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("wisp-mcp-client/0.1.0")
            .build()
            .context("Failed to create HTTP client")
    }

    pub async fn new_sse(
        server_id: String,
        url: String,
        headers: HashMap<String, String>,
    ) -> Result<Self> {
        let message_url = format!("{}/message", url.trim_end_matches('/'));

        Ok(Self {
            client: Self::build_client()?,
            url,
            headers,
            transport: HttpTransport::Sse,
            session_id: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1i64)),
            server_id,
            sse_task: Arc::new(Mutex::new(None)),
            message_url: Arc::new(Mutex::new(message_url)),
        })
    }

    pub async fn new_http(
        server_id: String,
        url: String,
        headers: HashMap<String, String>,
        session_id: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            client: Self::build_client()?,
            url,
            headers,
            transport: HttpTransport::Http,
            session_id: Arc::new(Mutex::new(session_id)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1i64)),
            server_id,
            sse_task: Arc::new(Mutex::new(None)),
            message_url: Arc::new(Mutex::new(String::new())),
        })
    }

    pub async fn initialize(&self) -> Result<Value> {
        let protocol_version = match &self.transport {
            HttpTransport::Sse => "2024-11-05",
            HttpTransport::Http => "2025-03-26",
        };

        let params = json!({
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {
                "name": "wisp",
                "version": "0.1.0"
            }
        });

        if matches!(self.transport, HttpTransport::Sse) {
            self.start_sse_listener().await?;
        }

        let result = self
            .call_with_timeout("initialize", params, Duration::from_secs(60))
            .await?;

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        match &self.transport {
            HttpTransport::Sse => {
                self.send_notification(&notification).await?;
            },
            HttpTransport::Http => {
                self.send_http_notification(&notification).await?;
            },
        }

        Ok(result)
    }

    async fn start_sse_listener(&self) -> Result<()> {
        let client = self.client.clone();
        let url = self.url.clone();
        let headers = self.headers.clone();
        let pending = Arc::clone(&self.pending);
        let server_id = self.server_id.clone();
        let message_url = Arc::clone(&self.message_url);

        let sse_url = format!("{}/sse", url.trim_end_matches('/'));

        let mut request = client.get(&sse_url).header("Accept", "text/event-stream");
        for (key, value) in &headers {
            request = request.header(key, value);
        }

        let task = tokio::spawn(async move {
            match request.send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        eprintln!(
                            "[MCP-SSE:{}] SSE connection failed: {}",
                            server_id,
                            response.status()
                        );
                        return;
                    }

                    let mut stream = response.bytes_stream();
                    let mut buffer = String::new();
                    let mut current_event = String::new();

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(chunk) => {
                                if let Ok(text) = std::str::from_utf8(&chunk) {
                                    buffer.push_str(text);

                                    while let Some(pos) = buffer.find('\n') {
                                        let line: String = buffer.drain(..=pos).collect();
                                        let trimmed = line.trim();

                                        if trimmed.is_empty() {
                                            current_event.clear();
                                            continue;
                                        }

                                        if let Some(value) = trimmed.strip_prefix("event:") {
                                            current_event = value.trim().to_string();
                                            continue;
                                        }

                                        if let Some(data) = trimmed.strip_prefix("data:") {
                                            let data = data.trim();

                                            if current_event == "endpoint" {
                                                let endpoint = data.trim().to_string();
                                                println!(
                                                    "[MCP-SSE:{}] Received endpoint: {}",
                                                    server_id, endpoint
                                                );
                                                *message_url.lock().await = endpoint;
                                                current_event.clear();
                                                continue;
                                            }

                                            if let Ok(msg) = serde_json::from_str::<Value>(data) {
                                                if let Some(id) =
                                                    msg.get("id").and_then(|v| v.as_i64())
                                                {
                                                    if let Some(tx) =
                                                        pending.lock().await.remove(&id)
                                                    {
                                                        let _ = tx.send(msg);
                                                    }
                                                } else {
                                                    println!(
                                                        "[MCP-SSE:{}] Notification: {}",
                                                        server_id, data
                                                    );
                                                }
                                            }
                                        }

                                        current_event.clear();
                                    }
                                }
                            },
                            Err(e) => {
                                eprintln!("[MCP-SSE:{}] Stream error: {}", server_id, e);
                                break;
                            },
                        }
                    }
                },
                Err(e) => {
                    eprintln!("[MCP-SSE:{}] Failed to connect: {}", server_id, e);
                },
            }
        });

        *self.sse_task.lock().await = Some(task);
        Ok(())
    }

    async fn send_notification(&self, notification: &Value) -> Result<()> {
        let url = self.message_url.lock().await.clone();
        if url.is_empty() {
            anyhow::bail!("[MCP-SSE:{}] No message endpoint available", self.server_id);
        }

        let mut request = self
            .client
            .post(&url)
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request
            .json(notification)
            .send()
            .await
            .context("Failed to send notification")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "[MCP-SSE:{}] Notification POST failed with status: {} body: {}",
                self.server_id, status, body
            );
        }

        Ok(())
    }

    async fn send_http_notification(&self, notification: &Value) -> Result<()> {
        let mut request = self
            .client
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let session_id = self.session_id.lock().await.clone();
        if let Some(sid) = session_id {
            request = request.header("Mcp-Session-Id", &sid);
        }

        let response = request
            .json(notification)
            .send()
            .await
            .context("Failed to send notification")?;

        if let Some(new_sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(s) = new_sid.to_str() {
                *self.session_id.lock().await = Some(s.to_string());
            }
        }

        let status = response.status();
        if status != reqwest::StatusCode::ACCEPTED && !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Notification POST failed with status: {} body: {}",
                status, body
            );
        }

        Ok(())
    }

    async fn send_http_request(&self, request_body: &Value) -> Result<Value> {
        let mut request = self
            .client
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream");
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let session_id = self.session_id.lock().await.clone();
        if let Some(sid) = session_id {
            request = request.header("Mcp-Session-Id", &sid);
        }

        let response = request
            .json(request_body)
            .send()
            .await
            .context("Failed to send HTTP request")?;

        if let Some(new_sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(s) = new_sid.to_str() {
                *self.session_id.lock().await = Some(s.to_string());
            }
        }

        let status = response.status();
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(json!({ "result": {} }));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "HTTP request failed with status: {} body: {}",
                status, body
            );
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if content_type.contains("text/event-stream") {
            let expected_id = request_body
                .get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let body_bytes = response
                .bytes()
                .await
                .context("Failed to read SSE response body")?;
            let body_str = String::from_utf8_lossy(&body_bytes);
            parse_sse_response(&body_str, expected_id)
        } else {
            let json: Value = response
                .json()
                .await
                .context("Failed to parse HTTP response")?;
            Ok(json)
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_with_timeout(method, params, Duration::from_secs(30))
            .await
    }

    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout_duration: Duration,
    ) -> Result<Value> {
        let id = {
            let mut id_lock = self.next_id.lock().await;
            let id = *id_lock;
            *id_lock += 1;
            id
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        match &self.transport {
            HttpTransport::Sse => {
                let (tx, rx) = oneshot::channel();
                self.pending.lock().await.insert(id, tx);

                self.send_notification(&request).await?;

                let response = timeout(timeout_duration, rx)
                    .await
                    .context(format!(
                        "MCP request timed out after {:?} for method: {}",
                        timeout_duration, method
                    ))?
                    .context("MCP response channel closed")?;

                if let Some(err) = response.get("error") {
                    anyhow::bail!("MCP Server Error: {}", err);
                }

                response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("MCP response missing 'result'"))
            },
            HttpTransport::Http => {
                let response = self.send_http_request(&request).await?;

                if let Some(err) = response.get("error") {
                    anyhow::bail!("MCP Server Error: {}", err);
                }

                response
                    .get("result")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("MCP response missing 'result'"))
            },
        }
    }

    pub async fn list_tools(&self, cursor: Option<String>) -> Result<Value> {
        let params = if let Some(c) = cursor {
            json!({ "cursor": c })
        } else {
            json!({})
        };
        self.call("tools/list", params).await
    }

    pub async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<Value> {
        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or(json!({}))
        });
        self.call("tools/call", params).await
    }

    pub async fn disconnect(&self) -> Result<()> {
        if let Some(task) = self.sse_task.lock().await.take() {
            task.abort();
        }
        Ok(())
    }
}

/// Parse a Server-Sent Events body and return the JSON-RPC message whose `id`
/// matches `expected_id`. Falls back to the first message if no id matches.
fn parse_sse_response(body: &str, expected_id: i64) -> Result<Value> {
    let mut data_lines: Vec<String> = Vec::new();
    let mut results: Vec<Value> = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            let data = data_lines.join("\n");
            if !data.is_empty() {
                if let Ok(msg) = serde_json::from_str::<Value>(&data) {
                    results.push(msg);
                }
            }
            data_lines.clear();
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("event:") {
            let _ = rest;
        } else if let Some(rest) = trimmed.strip_prefix("data:") {
            data_lines.push(rest.trim().to_string());
        }
    }

    if !data_lines.is_empty() {
        let data = data_lines.join("\n");
        if let Ok(msg) = serde_json::from_str::<Value>(&data) {
            results.push(msg);
        }
    }

    for msg in &results {
        if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
            if id == expected_id {
                return Ok(msg.clone());
            }
        }
    }

    results
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No JSON-RPC response found in SSE body"))
}
