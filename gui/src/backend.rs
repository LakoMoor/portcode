//! The agent runs on its own tokio thread; the window only exchanges messages with it.
//! Same core as the terminal app (`portcode_agent::Runner`), nothing agent-related is reimplemented here.
use crate::state::Perm;
use async_trait::async_trait;
use futures::channel::mpsc::UnboundedSender;
use portcode_agent::{Config, Gate, Outcome, Reply, Request, Runner, Ui, UiEvent};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

pub enum Cmd {
    Send(String),
    SetAgent(String),
    SetModel(String),
    SetDeep(bool),
    Tune(String, String),
    Compact,
    Clear,
}

pub struct Ready {
    pub agent: String,
    pub agents: Vec<String>,
    pub provider: String,
    pub model: String,
    pub window: u32,
    pub deep: bool,
    pub handle: Handle,
}

pub enum Msg {
    Ready(Ready),
    Failed(String),
    Ui(UiEvent),
    Perm(Perm),
    Done(Result<Outcome, String>),
    Info(String),
    Window(u32),
    Idle,
}

/// What the window keeps to talk to a running agent.
#[derive(Clone)]
pub struct Handle {
    cmd: mpsc::UnboundedSender<Cmd>,
    cancel: Arc<AtomicBool>,
    steer: Arc<Mutex<Vec<String>>>,
}

impl Handle {
    pub fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }
    pub fn interrupt(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn rearm(&self) {
        self.cancel.store(false, Ordering::Relaxed);
    }
    /// Text for the agent to read before its next step (the user typed while it works).
    pub fn steer(&self, t: String) {
        if let Ok(mut q) = self.steer.lock() {
            q.push(t);
        }
    }
    /// Steering messages the agent never got to because the turn ended first.
    pub fn take_steer(&self) -> Vec<String> {
        self.steer
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    }
}

struct Bridge(UnboundedSender<Msg>);
impl Ui for Bridge {
    fn event(&self, e: UiEvent) {
        let _ = self.0.unbounded_send(Msg::Ui(e));
    }
}

struct WindowGate(UnboundedSender<Msg>);
#[async_trait]
impl Gate for WindowGate {
    async fn ask(&self, r: &Request) -> Reply {
        let (tx, rx) = oneshot::channel();
        let p = Perm {
            req: r.clone(),
            reply: tx,
        };
        if self.0.unbounded_send(Msg::Perm(p)).is_err() {
            return Reply::Reject;
        }
        rx.await.unwrap_or(Reply::Reject)
    }
}

/// Start the agent for `root` on a background thread. The result arrives as `Msg::Ready` (or `Msg::Failed`).
pub fn spawn(root: PathBuf, deep: bool, tx: UnboundedSender<Msg>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.unbounded_send(Msg::Failed(e.to_string()));
                return;
            }
        };
        rt.block_on(run(root, deep, tx));
    });
}

async fn run(root: PathBuf, deep: bool, tx: UnboundedSender<Msg>) {
    let mut cfg = match Config::load(&root) {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.unbounded_send(Msg::Failed(e));
            return;
        }
    };
    if deep {
        cfg.mode = Some("deep".into());
    }
    let built = Runner::new(
        root,
        cfg,
        None,
        "build",
        Arc::new(WindowGate(tx.clone())),
        Arc::new(Bridge(tx.clone())),
    )
    .await;
    let mut r = match built {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.unbounded_send(Msg::Failed(e));
            return;
        }
    };
    let (ctx, mut rx) = mpsc::unbounded_channel::<Cmd>();
    let handle = Handle {
        cmd: ctx,
        cancel: r.cancel_flag(),
        steer: r.steer_handle(),
    };
    let agents = portcode_agent::agents::all()
        .into_iter()
        .filter(|a| a.primary)
        .map(|a| a.name.to_string())
        .collect();
    let _ = tx.unbounded_send(Msg::Ready(Ready {
        agent: r.agent.name.to_string(),
        agents,
        provider: r.resolved.provider.name.clone(),
        model: r.resolved.model.clone(),
        window: r.resolved.profile.context_window,
        deep: r.ctx.is_deep(),
        handle,
    }));
    // the real window comes from the server, not from a guess by model name
    if let Some(n) = r.detect_context().await {
        let _ = tx.unbounded_send(Msg::Window(n));
    }
    while let Some(c) = rx.recv().await {
        match c {
            Cmd::Send(t) => {
                let res = r.send(&t).await;
                let _ = tx.unbounded_send(Msg::Done(res));
            }
            Cmd::SetAgent(a) => {
                if let Err(e) = r.set_agent(&a) {
                    let _ = tx.unbounded_send(Msg::Info(e));
                }
            }
            Cmd::SetModel(m) => match r.set_model(&m) {
                Ok(()) => {
                    let _ = tx.unbounded_send(Msg::Window(r.resolved.profile.context_window));
                    if let Some(n) = r.detect_context().await {
                        let _ = tx.unbounded_send(Msg::Window(n));
                    }
                }
                Err(e) => {
                    let _ = tx.unbounded_send(Msg::Info(e));
                }
            },
            Cmd::SetDeep(d) => r.ctx.deep.store(d, Ordering::Relaxed),
            Cmd::Tune(what, val) => {
                let m = match r.tune(&what, &val) {
                    Ok(m) | Err(m) => m,
                };
                let _ = tx.unbounded_send(Msg::Info(m));
            }
            Cmd::Compact => {
                let m = match r.compact().await {
                    Ok(m) | Err(m) => m,
                };
                let _ = tx.unbounded_send(Msg::Info(m));
                let _ = tx.unbounded_send(Msg::Idle);
            }
            Cmd::Clear => r.clear(),
        }
    }
}
