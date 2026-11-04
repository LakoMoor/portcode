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
) -> Result<(App, mpsc::UnboundedReceiver<Msg>), String> {
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
    tokio::spawn(runner_task(runner, ctx_rx, mtx));
    Ok((app, mrx))
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
    let (mut app, mut rx) = start(cfg, &o, false).await?;
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
    let (mut app, mut rx) = start(cfg, &o, !s.until_permission).await?;
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
