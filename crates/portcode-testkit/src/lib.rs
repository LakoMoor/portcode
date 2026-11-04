//! Scripted OpenAI-compatible server for tests and reproducible demos. No model, no network beyond localhost.
//! Each `POST /v1/chat/completions` consumes the next scripted turn (streamed as SSE); `GET /v1/models` lists `mock-model`.
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Turn {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ScriptedCall>,
    /// Emit tool calls as Qwen/Hermes text instead of structured deltas (exercises the text fallback).
    pub as_text: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ScriptedCall {
    pub name: String,
    pub arguments: Value,
}

pub struct Mock {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Value>>>,
}

fn sse(v: Value) -> String {
    format!("data: {v}\n\n")
}

fn render(turn: &Turn, n: usize) -> String {
    let mut out = String::new();
    // split text into small chunks so streaming paths are exercised
    if !turn.reasoning.is_empty() {
        out += &sse(json!({"choices":[{"delta":{"reasoning_content": turn.reasoning}}]}));
    }
    let mut text = turn.text.clone();
    if turn.as_text {
        for c in &turn.tool_calls {
            text += &format!(
                "\n<tool_call>\n{}\n</tool_call>",
                json!({"name": c.name, "arguments": c.arguments})
            );
        }
    }
    let chars: Vec<char> = text.chars().collect();
    for ch in chars.chunks(24) {
        out += &sse(json!({"choices":[{"delta":{"content": ch.iter().collect::<String>()}}]}));
    }
    if !turn.as_text {
        for (i, c) in turn.tool_calls.iter().enumerate() {
            let args = c.arguments.to_string();
            out += &sse(
                json!({"choices":[{"delta":{"tool_calls":[{"index":i,"id":format!("call_{n}_{i}"),"function":{"name":c.name,"arguments":""}}]}}]}),
            );
            let (a, b) = args.split_at(args.len() / 2);
            for part in [a, b] {
                out += &sse(
                    json!({"choices":[{"delta":{"tool_calls":[{"index":i,"function":{"arguments":part}}]}}]}),
                );
            }
        }
    }
    let finish = if turn.tool_calls.is_empty() || turn.as_text {
        "stop"
    } else {
        "tool_calls"
    };
    out += &sse(
        json!({"choices":[{"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":120+n*40,"completion_tokens":30}}),
    );
    out + "data: [DONE]\n\n"
}

async fn handle(
    mut s: TcpStream,
    turns: Arc<Vec<Turn>>,
    idx: Arc<Mutex<usize>>,
    reqs: Arc<Mutex<Vec<Value>>>,
) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head_end, len) = loop {
        let Ok(n) = s.read(&mut tmp).await else {
            return;
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(tmp.get(..n).unwrap_or(&[]));
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(buf.get(..p).unwrap_or(&[])).to_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (p + 4, len);
        }
    };
    while buf.len() < head_end + len {
        let Ok(n) = s.read(&mut tmp).await else {
            return;
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(tmp.get(..n).unwrap_or(&[]));
    }
    let head = String::from_utf8_lossy(buf.get(..head_end).unwrap_or(&[])).to_string();
    let first = head.lines().next().unwrap_or("").to_string();
    let (status, ctype, body) = if first.starts_with("GET") && first.contains("/models") {
        (
            "200 OK",
            "application/json",
            json!({"data":[{"id":"mock-model"},{"id":"mock-flash"}]}).to_string(),
        )
    } else if first.starts_with("POST") && first.contains("/chat/completions") {
        if let Ok(v) =
            serde_json::from_slice::<Value>(buf.get(head_end..head_end + len).unwrap_or(&[]))
        {
            reqs.lock().unwrap_or_else(|e| e.into_inner()).push(v);
        }
        let n = {
            let mut i = idx.lock().unwrap_or_else(|e| e.into_inner());
            let n = *i;
            *i += 1;
            n
        };
        match turns.get(n) {
            Some(t) => ("200 OK", "text/event-stream", render(t, n)),
            None => (
                "200 OK",
                "text/event-stream",
                render(
                    &Turn {
                        text: "(script ended)".into(),
                        ..Default::default()
                    },
                    n,
                ),
            ),
        }
    } else {
        ("404 Not Found", "text/plain", "not found".into())
    };
    let resp = format!("HTTP/1.1 {status}\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
    let _ = s.write_all(resp.as_bytes()).await;
}

/// Start on `port` (0 = pick a free one). Returns the base URL ending in `/v1`.
pub async fn start(turns: Vec<Turn>, port: u16) -> std::io::Result<Mock> {
    let l = TcpListener::bind(("127.0.0.1", port)).await?;
    let url = format!("http://{}/v1", l.local_addr()?);
    let (turns, idx, requests) = (
        Arc::new(turns),
        Arc::new(Mutex::new(0usize)),
        Arc::new(Mutex::new(Vec::new())),
    );
    let r2 = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else { break };
            tokio::spawn(handle(s, turns.clone(), idx.clone(), r2.clone()));
        }
    });
    Ok(Mock { url, requests })
}
