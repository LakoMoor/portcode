//! Minimal MCP client over stdio (JSON-RPC 2.0, newline-delimited): `initialize`, `tools/list`, `tools/call`.
//! Lets PortCode drive external engines such as REA, decompiler bridges, browsers — configured under `"mcp"`.
use crate::config::McpEntry;
use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

pub struct McpServer {
    pub name: String,
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    next: std::sync::atomic::AtomicU64,
    _child: Child,
}

impl McpServer {
    pub async fn start(
        name: &str,
        e: &McpEntry,
        cwd: &std::path::Path,
    ) -> Result<Arc<Self>, String> {
        let (prog, args) = e.command.split_first().ok_or("mcp command is empty")?;
        let mut c = Command::new(prog);
        c.args(args)
            .envs(&e.env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = c.spawn().map_err(|er| format!("cannot start MCP server '{name}' ({prog}): {er}. Is it installed? See `portcode doctor`."))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let pending: Pending = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let p2 = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = lock(&p2).remove(&id) {
                        let _ = tx.send(v);
                    }
                }
            }
            lock(&p2).clear(); // server exited: waiting callers see a closed channel
        });
        let s = Arc::new(Self {
            name: name.into(),
            stdin: Mutex::new(stdin),
            pending,
            next: 1.into(),
            _child: child,
        });
        s.request("initialize", json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "portcode", "version": env!("CARGO_PKG_VERSION")}}), Duration::from_secs(60)).await?;
        s.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await?;
        Ok(s)
    }

    async fn send(&self, v: &Value) -> Result<(), String> {
        let mut line = v.to_string();
        line.push('\n');
        let mut w = self.stdin.lock().await;
        w.write_all(line.as_bytes())
            .await
            .map_err(|e| format!("MCP '{}' write failed: {e}", self.name))?;
        w.flush().await.map_err(|e| e.to_string())
    }

    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        let resp = match tokio::time::timeout(timeout, rx).await {
            Err(_) => {
                lock(&self.pending).remove(&id);
                return Err(format!(
                    "MCP '{}' {method} timed out after {}s",
                    self.name,
                    timeout.as_secs()
                ));
            }
            Ok(Err(_)) => return Err(format!("MCP server '{}' exited", self.name)),
            Ok(Ok(v)) => v,
        };
        if let Some(e) = resp.get("error") {
            return Err(format!(
                "MCP '{}' {method}: {}",
                self.name,
                e.get("message").and_then(Value::as_str).unwrap_or("error")
            ));
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Discover tools and wrap each as a PortCode [`Tool`] named `<server>_<tool>`.
    pub async fn tools(self: &Arc<Self>) -> Result<Vec<Arc<dyn Tool>>, String> {
        let r = self
            .request("tools/list", json!({}), Duration::from_secs(60))
            .await?;
        Ok(r.get("tools")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|t| {
                        let n = t.get("name")?.as_str()?.to_string();
                        Some(Arc::new(McpTool {
                            server: self.clone(),
                            tool: n,
                            desc: t
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            schema: t
                                .get("inputSchema")
                                .cloned()
                                .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                        }) as Arc<dyn Tool>)
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

pub struct McpTool {
    server: Arc<McpServer>,
    tool: String,
    desc: String,
    schema: Value,
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> String {
        format!("{}_{}", self.server.name, self.tool)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
    fn description(&self) -> String {
        format!(
            "[MCP {}] {}",
            self.server.name,
            self.desc.chars().take(600).collect::<String>()
        )
    }
    fn schema(&self) -> Value {
        self.schema.clone()
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.permit(
            "mcp",
            &format!("{}/{}", self.server.name, self.tool),
            Decision::Ask,
            "external MCP tool",
        )
        .await?;
        let r = self
            .server
            .request(
                "tools/call",
                json!({"name": self.tool, "arguments": args}),
                Duration::from_secs(300),
            )
            .await
            .map_err(ToolError)?;
        let text: String = r
            .get("content")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let text = if text.is_empty() { r.to_string() } else { text };
        if r.get("isError").and_then(Value::as_bool).unwrap_or(false) {
            return err(clip(&text, ctx.output_limit));
        }
        Ok(Out::new(self.tool.clone(), clip(&text, ctx.output_limit)))
    }
}
