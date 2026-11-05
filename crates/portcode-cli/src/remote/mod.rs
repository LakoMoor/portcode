//! Remote control: a WebSocket server the browser page on GitHub Pages (or any client) connects to.
//! GitHub Pages only hosts the static page; the connection goes straight from the browser to this server,
//! through a tunnel you control (Tailscale, cloudflared, SSH). No third-party relay sees your session.
//!
//! Protocol (JSON text frames):
//!   server → `{"t":"hello","nonce":HEX}`; client → `{"t":"auth","id":ID,"mac":HMAC-SHA256(secret, nonce)}`
//!   server → `{"t":"ready","scope":..,"snapshot":{..}}`, then events `ui` / `perm` / `perm_done` / `state` / `done` / `info`
//!   client (scope full) → `send`, `interrupt`, `perm`, `agent`, `model`, `deep`, `clear`
pub mod keys;
pub mod tunnel;

use crate::tui::app::{Msg, Remote};
use futures_util::{SinkExt, StreamExt};
use portcode_agent::Reply;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

pub const PAGES_ORIGIN: &str = "https://lakomoor.github.io";
const MAX_CLIENTS: usize = 4;

#[derive(Clone)]
pub struct Hub {
    pub tx: broadcast::Sender<String>,
}
impl Hub {
    pub fn publish(&self, v: &Value) {
        let _ = self.tx.send(v.to_string());
    }
}

fn origin_ok(origin: Option<&str>) -> bool {
    let extra = std::env::var("PORTCODE_REMOTE_ORIGINS").unwrap_or_default();
    match origin {
        None => true, // not a browser; the key still decides
        Some(o) => {
            o == PAGES_ORIGIN
                || o.strip_prefix("http://").is_some_and(|h| {
                    let host = h.split([':', '/']).next().unwrap_or("");
                    host == "localhost" || host == "127.0.0.1"
                })
                || extra
                    .split(',')
                    .any(|e| !e.trim().is_empty() && e.trim() == o)
        }
    }
}

/// Start listening. Returns the hub for publishing events and the bound address.
pub async fn serve(
    addr: SocketAddr,
    msg_tx: mpsc::UnboundedSender<Msg>,
) -> Result<(Hub, SocketAddr), String> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    let (tx, _) = broadcast::channel(512);
    let hub = Hub { tx: tx.clone() };
    let clients = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            if clients.load(std::sync::atomic::Ordering::Relaxed) >= MAX_CLIENTS {
                continue;
            }
            let (rx, msg_tx, clients) = (tx.subscribe(), msg_tx.clone(), clients.clone());
            tokio::spawn(async move {
                clients.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let _ =
                    tokio::time::timeout(Duration::from_secs(24 * 3600), client(sock, rx, msg_tx))
                        .await;
                clients.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            });
        }
    });
    Ok((hub, bound))
}

#[allow(clippy::result_large_err)] // the handshake callback type is fixed by tungstenite
async fn client(
    sock: tokio::net::TcpStream,
    mut events: broadcast::Receiver<String>,
    msg_tx: mpsc::UnboundedSender<Msg>,
) {
    let cfg = WebSocketConfig::default()
        .max_message_size(Some(1 << 20))
        .max_frame_size(Some(1 << 20));
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let o = req.headers().get("origin").and_then(|v| v.to_str().ok());
        if origin_ok(o) {
            Ok(resp)
        } else {
            let mut r = ErrorResponse::new(Some(
                "origin not allowed; set PORTCODE_REMOTE_ORIGINS".into(),
            ));
            *r.status_mut() = tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
            Err(r)
        }
    };
    let Ok(ws) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::accept_hdr_async_with_config(sock, check, Some(cfg)),
    )
    .await
    .unwrap_or_else(|_| Err(tokio_tungstenite::tungstenite::Error::ConnectionClosed)) else {
        return;
    };
    let (mut out, mut inp) = ws.split();
    let Ok(nonce) = keys::random_hex(16) else {
        return;
    };
    if out
        .send(Message::text(
            json!({"t":"hello","v":1,"nonce":nonce}).to_string(),
        ))
        .await
        .is_err()
    {
        return;
    }
    // authenticate within 10 s
    let auth = tokio::time::timeout(Duration::from_secs(10), inp.next()).await;
    let key =
        match auth {
            Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<Value>(t.as_str())
                .ok()
                .and_then(|v| {
                    let id = v.get("id")?.as_str()?.to_string();
                    let mac = v.get("mac")?.as_str()?.to_string();
                    let k = keys::load().into_iter().find(|k| k.id == id)?;
                    keys::ct_eq(&keys::mac_for(&k.secret, &nonce), &mac).then_some(k)
                }),
            _ => None,
        };
    let Some(key) = key else {
        tokio::time::sleep(Duration::from_secs(1)).await; // slow down guessing
        let _ = out
            .send(Message::text(
                json!({"t":"error","msg":"authentication failed"}).to_string(),
            ))
            .await;
        return;
    };
    let (stx, srx) = oneshot::channel();
    let _ = msg_tx.send(Msg::Remote(Remote::Snapshot(stx)));
    let snapshot = tokio::time::timeout(Duration::from_secs(5), srx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or(Value::Null);
    if out
        .send(Message::text(
            json!({"t":"ready","scope":key.scope,"snapshot":snapshot}).to_string(),
        ))
        .await
        .is_err()
    {
        return;
    }
    let full = key.scope == "full";
    loop {
        tokio::select! {
            ev = events.recv() => match ev {
                Ok(s) => if out.send(Message::text(s)).await.is_err() { return },
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // missed events: resync the page from a fresh snapshot
                    let (stx, srx) = oneshot::channel();
                    let _ = msg_tx.send(Msg::Remote(Remote::Snapshot(stx)));
                    if let Ok(s) = srx.await {
                        let _ = out.send(Message::text(json!({"t":"resync","snapshot":s}).to_string())).await;
                    }
                }
                Err(_) => return,
            },
            m = inp.next() => match m {
                Some(Ok(Message::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    let cmd = match v.get("t").and_then(Value::as_str) {
                        Some("send") => v.get("text").and_then(Value::as_str).map(|s| Remote::Send(s.to_string())),
                        Some("interrupt") => Some(Remote::Interrupt),
                        Some("perm") => match v.get("reply").and_then(Value::as_str) {
                            Some("once") => Some(Remote::Perm(Reply::Once)),
                            Some("always") => Some(Remote::Perm(Reply::Always)),
                            Some("reject") => Some(Remote::Perm(Reply::Reject)),
                            _ => None,
                        },
                        Some("agent") => v.get("name").and_then(Value::as_str).map(|s| Remote::Agent(s.to_string())),
                        Some("model") => v.get("ref").and_then(Value::as_str).map(|s| Remote::Model(s.to_string())),
                        Some("deep") => v.get("on").and_then(Value::as_bool).map(Remote::Deep),
                        Some("clear") => Some(Remote::Clear),
                        Some("ping") => { let _ = out.send(Message::text(json!({"t":"pong"}).to_string())).await; None }
                        _ => None,
                    };
                    if let Some(c) = cmd {
                        if full {
                            let _ = msg_tx.send(Msg::Remote(c));
                        } else {
                            let _ = out.send(Message::text(json!({"t":"error","msg":"this key is view-only"}).to_string())).await;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_pages_localhost_and_configured_origins_may_connect() {
        assert!(origin_ok(Some(PAGES_ORIGIN)));
        assert!(origin_ok(Some("http://localhost:8000")));
        assert!(origin_ok(None));
        assert!(!origin_ok(Some("http://localhost.evil.example")));
        assert!(!origin_ok(Some("https://evil.example")));
        assert!(!origin_ok(Some("https://lakomoor.github.io.evil.example")));
    }
}
