use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use serde_json::{json, Value};
use anyhow::{Context, Result};
use reqwest::Client;
use futures::StreamExt;

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
}

impl McpHttpClient {
    pub async fn new_sse(
        server_id: String,
        url: String,
        headers: HashMap<String, String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            url,
            headers,
            transport: HttpTransport::Sse,
            session_id: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1i64)),
            server_id,
            sse_task: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn new_http(
        server_id: String,
        url: String,
        headers: HashMap<String, String>,
        session_id: Option<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            url,
            headers,
            transport: HttpTransport::Http,
            session_id: Arc::new(Mutex::new(session_id)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1i64)),
            server_id,
            sse_task: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn initialize(&self) -> Result<Value> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "wisp",
                "version": "0.1.0"
            }
        });

        // 关键：对于 SSE 传输，必须先启动 SSE 监听器再发起 initialize 请求，
        // 否则 initialize 的响应（通过 SSE 流返回）无人接收，会导致 60 秒超时失败。
        if matches!(self.transport, HttpTransport::Sse) {
            self.start_sse_listener().await?;
        }

        let result = self.call_with_timeout("initialize", params, Duration::from_secs(60)).await?;

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        match &self.transport {
            HttpTransport::Sse => {
                self.send_notification(&notification).await?;
            }
            HttpTransport::Http => {
                self.send_http_request(&notification).await?;
            }
        }

        Ok(result)
    }

    async fn start_sse_listener(&self) -> Result<()> {
        let client = self.client.clone();
        let url = self.url.clone();
        let headers = self.headers.clone();
        let pending = Arc::clone(&self.pending);
        let server_id = self.server_id.clone();

        let sse_url = format!("{}/sse", url.trim_end_matches('/'));
        
        let mut request = client.get(&sse_url);
        for (key, value) in &headers {
            request = request.header(key, value);
        }

        let task = tokio::spawn(async move {
            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let mut stream = response.bytes_stream();
                        let mut buffer = String::new();

                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if let Ok(text) = std::str::from_utf8(&chunk) {
                                        buffer.push_str(text);

                                        while let Some(pos) = buffer.find('\n') {
                                            let line: String = buffer.drain(..=pos).collect();

                                            let trimmed = line.trim();
                                            if trimmed.is_empty() {
                                                continue;
                                            }

                                            if trimmed.starts_with("data:") {
                                                let data = trimmed[5..].trim();
                                                if let Ok(msg) = serde_json::from_str::<Value>(data) {
                                                    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                                                        if let Some(tx) = pending.lock().await.remove(&id) {
                                                            let _ = tx.send(msg);
                                                        }
                                                    } else {
                                                        println!("[MCP-SSE:{}] Notification: {}", server_id, data);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[MCP-SSE:{}] Stream error: {}", server_id, e);
                                    break;
                                }
                            }
                        }
                    } else {
                        eprintln!("[MCP-SSE:{}] SSE connection failed: {}", server_id, response.status());
                    }
                }
                Err(e) => {
                    eprintln!("[MCP-SSE:{}] Failed to connect: {}", server_id, e);
                }
            }
        });

        *self.sse_task.lock().await = Some(task);
        Ok(())
    }

    async fn send_notification(&self, notification: &Value) -> Result<()> {
        let url = format!("{}/message", self.url.trim_end_matches('/'));
        
        let mut request = self.client.post(&url);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        request
            .json(notification)
            .send()
            .await
            .context("Failed to send notification")?;

        Ok(())
    }

    async fn send_http_request(&self, request_body: &Value) -> Result<Value> {
        let mut request = self.client.post(&self.url);
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let session_id = self.session_id.lock().await.clone();
        if let Some(sid) = session_id {
            request = request.header("X-Session-Id", &sid);
        }

        let response = request
            .json(request_body)
            .send()
            .await
            .context("Failed to send HTTP request")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP request failed with status: {}", response.status());
        }

        let json: Value = response
            .json()
            .await
            .context("Failed to parse HTTP response")?;

        Ok(json)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_with_timeout(method, params, Duration::from_secs(30)).await
    }

    pub async fn call_with_timeout(&self, method: &str, params: Value, timeout_duration: Duration) -> Result<Value> {
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
                    .context(format!("MCP request timed out after {:?} for method: {}", timeout_duration, method))?
                    .context("MCP response channel closed")?;

                if let Some(err) = response.get("error") {
                    anyhow::bail!("MCP Server Error: {}", err);
                }

                response.get("result").cloned().ok_or_else(|| anyhow::anyhow!("MCP response missing 'result'"))
            }
            HttpTransport::Http => {
                let response = self.send_http_request(&request).await?;

                if let Some(err) = response.get("error") {
                    anyhow::bail!("MCP Server Error: {}", err);
                }

                response.get("result").cloned().ok_or_else(|| anyhow::anyhow!("MCP response missing 'result'"))
            }
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
