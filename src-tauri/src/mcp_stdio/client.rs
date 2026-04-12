use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use serde_json::{json, Value};
use anyhow::{Context, Result};

pub struct McpStdioClient {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    next_id: Arc<Mutex<i64>>,
    reader_handle: tokio::task::JoinHandle<()>,
    child: Child,
    server_id: String,
}

impl McpStdioClient {
    pub async fn spawn(server_id: String, cmd: &str, args: &[String]) -> Result<Self> {
        println!("[MCP:{}] Spawning process: {} {:?}", server_id, cmd, args);
        
        let mut child = if cfg!(target_os = "windows") {
            let full_args = vec![cmd.to_string()];
            let mut all_args = full_args;
            all_args.extend(args.iter().cloned());
            
            Command::new("cmd")
                .args(["/C", &all_args.join(" ")])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context(format!("Failed to start MCP server process: cmd /C {} {}", cmd, args.join(" ")))?
        } else {
            Command::new(cmd)
                .args(args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context(format!("Failed to start MCP server process: {} {:?}", cmd, args))?
        };

        let stdin = child.stdin.take().context("Failed to acquire stdin")?;
        let stdout = child.stdout.take().context("Failed to acquire stdout")?;
        let stderr = child.stderr.take();
        
        let server_id_for_stderr = server_id.clone();
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            eprintln!("[MCP:{}:stderr] {}", server_id_for_stderr, line.trim());
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let next_id = Arc::new(Mutex::new(1i64));
        let pending_clone = Arc::clone(&pending);
        let server_id_clone = server_id.clone();

        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        println!("[MCP:{}] Process exited (EOF)", server_id_clone);
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() { continue; }

                        if let Ok(msg) = serde_json::from_str::<Value>(trimmed) {
                            if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                                if let Some(tx) = pending_clone.lock().await.remove(&id) {
                                    let _ = tx.send(msg);
                                }
                            } else {
                                println!("[MCP:{}] Notification: {}", server_id_clone, trimmed);
                            }
                        } else {
                            eprintln!("[MCP:{}] Invalid JSON: {}", server_id_clone, trimmed);
                        }
                    }
                    Err(e) => {
                        eprintln!("[MCP:{}] Stdio read error: {}", server_id_clone, e);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id,
            reader_handle,
            child,
            server_id,
        })
    }

    /// 发送 JSON-RPC 请求并等待响应（带超时）
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_with_timeout(method, params, Duration::from_secs(30)).await
    }

    /// 发送 JSON-RPC 请求并等待响应（自定义超时）
    pub async fn call_with_timeout(&self, method: &str, params: Value, timeout_duration: Duration) -> Result<Value> {
        let id = {
            let mut id_lock = self.next_id.lock().await;
            let id = *id_lock;
            *id_lock += 1;
            id
        };

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        // 写入 stdio
        let mut stdin = self.stdin.lock().await;
        let line = serde_json::to_string(&request).context("Failed to serialize request")? + "\n";
        stdin.write_all(line.as_bytes()).await.context("Failed to write to MCP stdin")?;
        stdin.flush().await.context("Failed to flush MCP stdin")?;

        // 等待响应（带超时）
        let response = timeout(timeout_duration, rx)
            .await
            .context(format!("MCP request timed out after {:?} for method: {}", timeout_duration, method))?
            .context("MCP response channel closed")?;

        // 处理 MCP 错误响应
        if let Some(err) = response.get("error") {
            anyhow::bail!("MCP Server Error: {}", err);
        }

        // 返回 result 字段
        response.get("result").cloned().ok_or_else(|| anyhow::anyhow!("MCP response missing 'result'"))
    }

    /// 初始化 MCP 连接（发送 initialize 请求）
    pub async fn initialize(&self) -> Result<Value> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "wisp",
                "version": "0.1.0"
            }
        });
        
        // 使用更长的超时时间进行初始化
        let result = self.call_with_timeout("initialize", params, Duration::from_secs(60)).await?;
        
        // 发送 initialized 通知
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        
        let mut stdin = self.stdin.lock().await;
        let line = serde_json::to_string(&notification).context("Failed to serialize notification")? + "\n";
        stdin.write_all(line.as_bytes()).await.context("Failed to write notification")?;
        stdin.flush().await.context("Failed to flush notification")?;
        
        Ok(result)
    }

    /// 获取工具列表
    pub async fn list_tools(&self, cursor: Option<String>) -> Result<Value> {
        let params = if let Some(c) = cursor {
            json!({ "cursor": c })
        } else {
            json!({})
        };
        self.call("tools/list", params).await
    }

    /// 调用工具
    pub async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<Value> {
        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or(json!({}))
        });
        self.call("tools/call", params).await
    }

    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
        }
    }

    pub async fn kill(&mut self) -> Result<()> {
        self.reader_handle.abort();
        self.child.kill().await.context("Failed to kill MCP process")?;
        Ok(())
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.reader_handle.abort();
        let _ = self.child.start_kill();
    }
}
