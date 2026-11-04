//! Full-screen terminal UI in the style of OpenCode: home screen, transcript with tool blocks, prompt box,
//! sidebar (context, evidence, todo), slash-command popup, permission dialog. Real runs and headless snapshots
//! share the same `App` + `view`, so screenshots are produced by the real UI code.
pub mod app;
pub mod svg;
pub mod theme;
pub mod view;

use app::{App, Cmd, Msg, PermAsk};
use async_trait::async_trait;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use portcode_agent::{Config, Gate, Reply, Request, Runner, Ui, UiEvent};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::Terminal;
use std::io::stdout;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

struct Bridge(mpsc::UnboundedSender<Msg>);
impl Ui for Bridge {
    fn event(&self, e: UiEvent) {
        let _ = self.0.send(Msg::Ui(e));
    }
}

struct TuiGate(mpsc::UnboundedSender<Msg>);
#[async_trait]
impl Gate for TuiGate {
    async fn ask(&self, r: &Request) -> Reply {
        let (tx, rx) = oneshot::channel();
        if self
            .0
            .send(Msg::Perm(PermAsk {
                req: r.clone(),
                reply: tx,
            }))
            .is_err()
        {
            return Reply::Reject;
        }
        rx.await.unwrap_or(Reply::Reject)
    }
}

pub struct Opts {
    pub model: Option<String>,
    pub agent: String,
    pub cont: bool,
    pub show_reasoning: bool,
    pub cwd_label: Option<String>,
}

async fn runner_task(
    mut r: Runner,
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    tx: mpsc::UnboundedSender<Msg>,
) {
    while let Some(c) = rx.recv().await {
        match c {
            Cmd::Send(t) => {
                let res = r.send(&t).await;
                let _ = tx.send(Msg::Done(res));
            }
            Cmd::SetAgent(a) => {
                if let Err(e) = r.set_agent(&a) {
                    let _ = tx.send(Msg::Info(e));
                }
            }
            Cmd::SetModel(m) => {
                if let Err(e) = r.set_model(&m) {
                    let _ = tx.send(Msg::Info(e));
                }
            }
            Cmd::SetDeep(d) => r.ctx.deep.store(d, std::sync::atomic::Ordering::Relaxed),
            Cmd::Clear => r.clear(),
            Cmd::Resume => {
                let m = match r.latest_session() {
                    Some(id) => match r.resume(&id) {
                        Ok(n) => format!(
                            "resumed {id} ({n} messages); history is in context but not redrawn"
                        ),
                        Err(e) => e,
                    },
                    None => "no saved sessions".into(),
                };
                let _ = tx.send(Msg::Info(m));
            }
            Cmd::ListModels => {
                let m = match portcode_llm::Client::new(r.resolved.provider.clone()) {
                    Ok(c) => match c.list_models().await {
                        Ok(ids) if !ids.is_empty() => {
                            format!("models on {}: {}", r.resolved.provider.name, ids.join(", "))
                        }
                        Ok(_) => "no models reported".into(),
                        Err(e) => e.to_string(),
                    },
                    Err(e) => e.to_string(),
                };
                let _ = tx.send(Msg::Info(m));
            }
        }
    }
}

async fn start(
    cfg: Config,
    o: &Opts,
    gate_auto: bool,
) -> Result<
    (
        App,
        mpsc::UnboundedReceiver<Msg>,
        mpsc::UnboundedSender<Msg>,
    ),
    String,
> {
    let root = crate::root()?;
    let (mtx, mrx) = mpsc::unbounded_channel();
    let (ctx_tx, ctx_rx) = mpsc::unbounded_channel();
    let mut runner = Runner::new(
        root.clone(),
        cfg.clone(),
        o.model.as_deref(),
        &o.agent,
        Arc::new(TuiGate(mtx.clone())),
        Arc::new(Bridge(mtx.clone())),
    )
    .await?;
    if o.cont {
        if let Some(id) = runner.latest_session() {
            let _ = runner.resume(&id);
        }
    }
    let cancel = runner.cancel_flag();
    let label = o.cwd_label.clone().unwrap_or_else(|| {
        let p = root.display().to_string();
        match std::env::var("HOME") {
            Ok(h) if p.starts_with(&h) => format!("~{}", &p[h.len()..]),
            _ => p,
        }
    });
    let mut app = App::new(
        cfg,
        root,
        &o.agent,
        &runner.resolved.model,
        &runner.resolved.provider.name,
        runner.resolved.profile.context_window,
        label,
        ctx_tx,
        cancel,
    );
    app.show_reasoning = o.show_reasoning;
    app.auto_allow = gate_auto;
    app.refresh_ledger();
    tokio::spawn(runner_task(runner, ctx_rx, mtx.clone()));
    Ok((app, mrx, mtx))
}

/// Remote-control state owned by the event loop.
#[derive(Default)]
struct Remote {
    hub: Option<crate::remote::Hub>,
    last_state: String,
}

impl Remote {
    /// `/remote`, `/remote key [view|full]`.
    async fn command(&mut self, app: &mut App, mtx: &mpsc::UnboundedSender<Msg>, arg: &str) {
        let mut words = arg.split_whitespace();
        match words.next() {
            Some("key") => {
                match crate::remote::keys::create(words.next().unwrap_or("full")) {
                    Ok(k) => app.items.push(app::Item::Info(format!(
                        "new {} key (shown once, keep it private): {}",
                        k.scope,
                        crate::remote::keys::pairing(&k)
                    ))),
                    Err(e) => app.items.push(app::Item::Error(e)),
                }
                return;
            }
            Some(other) => {
                app.items.push(app::Item::Error(format!(
                    "unknown /remote argument '{other}'. Use /remote or /remote key [view|full]"
                )));
                return;
            }
            None => {}
        }
        if self.hub.is_some() {
            app.items.push(app::Item::Info(format!(
                "remote control is on: {}",
                app.remote_addr.clone().unwrap_or_default()
            )));
            return;
        }
        let addr: std::net::SocketAddr = ([127, 0, 0, 1], 7777).into();
        match crate::remote::serve(addr, mtx.clone()).await {
            Ok((hub, bound)) => {
                self.hub = Some(hub);
                app.remote_on = true;
                app.remote_addr = Some(format!("ws://{bound}"));
                let mut text = format!(
                    "remote control on ws://{bound} (this machine only). Open {}/portcode/remote/ and paste a key; to reach it from elsewhere use a tunnel (docs/REMOTE.md).",
                    crate::remote::PAGES_ORIGIN
                );
                if crate::remote::keys::load().is_empty() {
                    if let Ok(k) = crate::remote::keys::create("full") {
                        text += &format!(
                            " Your key (shown once): {}",
                            crate::remote::keys::pairing(&k)
                        );
                    }
                } else {
                    text += " Keys exist; make another with /remote key.";
                }
                app.items.push(app::Item::Info(text));
            }
            Err(e) => app.items.push(app::Item::Error(e)),
        }
    }

    /// Copy what changed in the app to connected clients.
    fn publish(&mut self, app: &mut App) {
        let Some(h) = &self.hub else {
            app.outbox.clear();
            return;
        };
        for v in app.outbox.drain(..) {
            h.publish(&v);
        }
        let st = app.state_json().to_string();
        if st != self.last_state {
            h.publish(&serde_json::json!({"type":"state","state":app.state_json()}));
            self.last_state = st;
        }
    }
}

/// `portcode serve`: the same session as the full-screen UI, without a terminal. Drive it from the web page.
pub async fn serve(cfg: Config, o: Opts, bind: std::net::SocketAddr) -> Result<i32, String> {
    let (mut app, mut rx, mtx) = start(cfg, &o, false).await?;
    if crate::remote::keys::load().is_empty() {
        let k = crate::remote::keys::create("full")?;
        println!(
            "created a key (shown once, keep it private):\n  {}",
            crate::remote::keys::pairing(&k)
        );
    }
    let (hub, bound) = crate::remote::serve(bind, mtx).await?;
    app.remote_on = true;
    app.remote_addr = Some(format!("ws://{bound}"));
    println!("portcode remote control listening on ws://{bound}");
    println!(
        "open {}/portcode/remote/ and paste a key. Ctrl+C to stop.",
        crate::remote::PAGES_ORIGIN
    );
    if !bound.ip().is_loopback() {
        println!("warning: {bound} is reachable by others and ws:// is not encrypted. Put it behind a TLS tunnel (docs/REMOTE.md) or bind 127.0.0.1.");
    }
    let mut remote = Remote {
        hub: Some(hub),
        last_state: String::new(),
    };
    loop {
        tokio::select! {
            m = rx.recv() => match m { Some(m) => app.on_msg(m), None => break },
            _ = tokio::time::sleep(Duration::from_millis(80)) => {}
            _ = tokio::signal::ctrl_c() => break,
        }
        remote.publish(&mut app);
    }
    Ok(0)
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            stdout(),
            crossterm::event::DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}

pub async fn run(cfg: Config, o: Opts) -> Result<i32, String> {
    let (mut app, mut rx, mtx) = start(cfg, &o, false).await?;
    let mut remote = Remote::default();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |i| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        prev(i);
    }));
    enable_raw_mode().map_err(|e| e.to_string())?;
    execute!(
        stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )
    .map_err(|e| e.to_string())?;
    let _guard = Guard;
    let mut term = Terminal::new(CrosstermBackend::new(stdout())).map_err(|e| e.to_string())?;
    loop {
        term.draw(|f| view::draw(f, &mut app))
            .map_err(|e| e.to_string())?;
        if event::poll(Duration::from_millis(60)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Paste(s) => app.on_paste(&s),
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollUp => app.on_scroll(true),
                    MouseEventKind::ScrollDown => app.on_scroll(false),
                    _ => {}
                },
                _ => {}
            }
        }
        while let Ok(m) = rx.try_recv() {
            app.on_msg(m);
        }
        if let Some(arg) = app.remote_req.take() {
            remote.command(&mut app, &mtx, &arg).await;
        }
        remote.publish(&mut app);
        app.tick();
        if app.quit {
            break;
        }
    }
    Ok(0)
}

pub struct Snap {
    pub out: String,
    pub w: u16,
    pub h: u16,
    pub prompt: Option<String>,
    pub until_permission: bool,
    pub typed: Option<String>,
}

/// Headless render of the real UI into an SVG file (no terminal needed). Drives the real runner if `prompt` is set.
pub async fn snapshot(cfg: Config, o: Opts, s: Snap) -> Result<i32, String> {
    let (mut app, mut rx, _mtx) = start(cfg, &o, !s.until_permission).await?;
    if let Some(p) = &s.prompt {
        app.input = p.clone();
        app.cursor = p.chars().count();
        app.submit();
        let deadline = Instant::now() + Duration::from_secs(90);
        'outer: loop {
            while let Ok(m) = rx.try_recv() {
                app.on_msg(m);
                if s.until_permission && app.perm.is_some() {
                    break 'outer;
                }
            }
            if !app.busy {
                break;
            }
            if Instant::now() > deadline {
                return Err("snapshot timed out".into());
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        while let Ok(m) = rx.try_recv() {
            app.on_msg(m);
        }
    }
    if let Some(t) = &s.typed {
        app.input = t.clone();
        app.cursor = t.chars().count();
    }
    app.refresh_ledger();
    app.spinner = 0;
    let mut term = Terminal::new(TestBackend::new(s.w, s.h)).map_err(|e| e.to_string())?;
    term.draw(|f| view::draw(f, &mut app))
        .map_err(|e| e.to_string())?;
    let svg = svg::buffer_to_svg(
        term.backend().buffer(),
        &format!(
            "portcode — {}",
            app.title.clone().unwrap_or_else(|| "home".into())
        ),
    );
    std::fs::write(&s.out, svg).map_err(|e| format!("{}: {e}", s.out))?;
    println!("wrote {}", s.out);
    Ok(0)
}
