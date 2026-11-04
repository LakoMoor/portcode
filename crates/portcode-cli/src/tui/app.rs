//! TUI state and update logic (no rendering, no terminal): driven by key events and runner messages.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portcode_agent::tool::Todo;
use portcode_agent::{Config, Outcome, Reply, Request, Status, UiEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub enum Msg {
    Ui(UiEvent),
    Perm(PermAsk),
    Done(Result<Outcome, String>),
    Info(String),
    Remote(Remote),
    Wb(super::workbench::WbMsg),
}

/// Commands from a remote client (already checked against the key's scope).
pub enum Remote {
    Snapshot(oneshot::Sender<serde_json::Value>),
    Send(String),
    Interrupt,
    Perm(Reply),
    Agent(String),
    Model(String),
    Deep(bool),
    Clear,
}
pub struct PermAsk {
    pub req: Request,
    pub reply: oneshot::Sender<Reply>,
}
pub enum Cmd {
    Send(String),
    SetAgent(String),
    SetModel(String),
    Clear,
    Resume,
    ListModels,
    Update,
    SetDeep(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Ok,
    Err,
}

pub enum Item {
    User(String),
    Assistant(String),
    Reasoning(String),
    Tool {
        name: String,
        args: String,
        state: ToolState,
        title: String,
        text: String,
    },
    Info(String),
    Error(String),
    Footer(String),
}

pub const SLASH: &[(&str, &str)] = &[
    ("agent", "switch agent: build · port · reverse · plan"),
    ("model", "switch model: provider/model"),
    ("models", "list models of this provider"),
    (
        "provider",
        "list providers or switch: /provider <name> [model]",
    ),
    ("key", "enter an API key (hidden); /key <provider>"),
    ("setup", "first-run guide: provider, key, model"),
    ("update", "check for a newer PortCode"),
    ("clear", "new session"),
    ("resume", "resume the latest session"),
    ("tools", "list tools"),
    ("ledger", "show the evidence ledger"),
    (
        "deep",
        "deep mode: may disassemble, decompile, emulate the original",
    ),
    (
        "clean",
        "clean-room mode: observe the original from outside only",
    ),
    (
        "remote",
        "start remote control (browser / phone); /remote key to pair",
    ),
    ("engines", "installed reverse-engineering engines"),
    (
        "create",
        "guided start: what to recreate, how closely to look, what to build with",
    ),
    (
        "workbench",
        "deep: functions, pseudo-C, disassembly, hex and evidence side by side",
    ),
    ("thinking", "show / hide reasoning"),
    ("sidebar", "toggle sidebar"),
    ("help", "keys and commands"),
    ("exit", "quit"),
];

pub struct App {
    pub items: Vec<Item>,
    pub input: String,
    pub cursor: usize,
    pub history: Vec<String>,
    hist_pos: Option<usize>,
    pub busy: bool,
    pub agent: String,
    pub agents: Vec<String>,
    pub model: String,
    pub provider: String,
    pub ctx_window: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub last_prompt: u64,
    pub todos: Vec<Todo>,
    pub perm: Option<PermAsk>,
    pub scroll: usize,
    pub show_reasoning: bool,
    pub sidebar: bool,
    pub ledger: (usize, usize, usize),
    pub unknowns: Vec<String>,
    pub cwd: String,
    pub title: Option<String>,
    pub spinner: usize,
    pub quit: bool,
    pub wb: Option<super::workbench::Wb>,
    pub wiz: Option<super::wizard::Wizard>,
    pub auto_allow: bool,
    /// Highlighted row of the `/` menu (arrows move it).
    pub slash_sel: usize,
    /// While set, the next input line is an API key for this provider (shown hidden).
    pub key_for: Option<String>,
    /// Deep mode (may look inside the original); default is clean-room.
    pub deep: bool,
    /// Remote control is active: events are copied to `outbox` for the server to publish.
    pub remote_on: bool,
    pub remote_addr: Option<String>,
    /// The user asked (`/remote`) to start the server; the event loop does it.
    pub remote_req: Option<String>,
    pub outbox: Vec<serde_json::Value>,
    /// Last parity verdict: (passed, summary).
    pub parity: Option<(bool, String)>,
    pub cmd_tx: mpsc::UnboundedSender<Cmd>,
    pub cancel: Arc<AtomicBool>,
    cfg: Config,
    root: std::path::PathBuf,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: Config,
        root: std::path::PathBuf,
        agent: &str,
        model: &str,
        provider: &str,
        ctx_window: u32,
        cwd_label: String,
        cmd_tx: mpsc::UnboundedSender<Cmd>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        let deep = cfg.mode.as_deref() == Some("deep");
        Self {
            items: vec![],
            input: String::new(),
            cursor: 0,
            history: vec![],
            hist_pos: None,
            busy: false,
            agent: agent.into(),
            agents: portcode_agent::agents::all()
                .iter()
                .filter(|a| a.primary)
                .map(|a| a.name.to_string())
                .collect(),
            model: model.into(),
            provider: provider.into(),
            ctx_window,
            tokens_in: 0,
            tokens_out: 0,
            last_prompt: 0,
            todos: vec![],
            perm: None,
            scroll: 0,
            show_reasoning: false,
            sidebar: true,
            ledger: (0, 0, 0),
            unknowns: vec![],
            cwd: cwd_label,
            title: None,
            spinner: 0,
            quit: false,
            wb: None,
            wiz: None,
            auto_allow: false,
            slash_sel: 0,
            key_for: None,
            deep,
            remote_on: false,
            remote_addr: None,
            remote_req: None,
            outbox: vec![],
            parity: None,
            cmd_tx,
            cancel,
            cfg,
            root,
        }
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if self.spinner % 20 == 0 {
            self.refresh_ledger();
        }
    }

    pub fn refresh_ledger(&mut self) {
        let l = portcode_evidence::Ledger::open(self.root.join(".portcode/evidence"));
        if let Ok(all) = l.all() {
            use portcode_evidence::Kind::*;
            self.ledger = (
                all.iter().filter(|e| e.kind == Observation).count(),
                all.iter().filter(|e| e.kind == Inference).count(),
                all.iter().filter(|e| e.kind == Unknown).count(),
            );
        }
        if let (Some(wb), Ok(all)) = (self.wb.as_mut(), l.all()) {
            use portcode_evidence::Kind;
            wb.notes = all
                .iter()
                .rev()
                .take(12)
                .map(|e| super::workbench::Note {
                    id: e.id.clone(),
                    kind: match e.kind {
                        Kind::Observation => "observation",
                        Kind::Inference => "inference",
                        Kind::Unknown => "unknown",
                    },
                    text: e.statement.clone(),
                })
                .collect();
        }
        self.unknowns = l
            .open_unknowns()
            .map(|u| {
                u.into_iter()
                    .map(|e| format!("{} {}", e.id, e.statement))
                    .collect()
            })
            .unwrap_or_default();
    }

    /// A tool context for the workbench's own calls: auto-allowed (the user opened the workbench), deep as the app is.
    pub fn tool_ctx(&self) -> (portcode_agent::tool::Ctx, std::path::PathBuf) {
        let c = portcode_agent::tool::Ctx::new(
            self.root.clone(),
            Arc::new(crate::term::TermGate { auto: true }),
            portcode_agent::Rules::from_config(&self.cfg.permission),
        );
        c.deep.store(self.deep, Ordering::Relaxed);
        (c, self.root.clone())
    }

    pub fn root_path(&self) -> &std::path::Path {
        &self.root
    }

    fn open_workbench(&mut self, arg: Option<&str>) {
        if !self.deep {
            self.items.push(Item::Error("the workbench looks inside the original, so it needs deep mode. Type /deep first (only on software you own or may examine).".into()));
            return;
        }
        let file = match arg
            .map(String::from)
            .or_else(|| super::workbench::find_binary(&self.root))
        {
            Some(f) => f,
            None => {
                self.items.push(Item::Error(
                    "no executable found in original/. Usage: /workbench path/to/binary".into(),
                ));
                return;
            }
        };
        self.wb = Some(super::workbench::Wb::new(&file));
        self.refresh_ledger();
    }

    pub fn context_pct(&self) -> u64 {
        if self.ctx_window == 0 {
            0
        } else {
            (self.last_prompt * 100 / u64::from(self.ctx_window)).min(999)
        }
    }

    fn push_text(&mut self, t: &str) {
        if let Some(Item::Assistant(s)) = self.items.last_mut() {
            s.push_str(t);
        } else {
            self.items.push(Item::Assistant(t.to_string()));
        }
    }

    pub fn on_msg(&mut self, m: Msg) {
        if self.remote_on {
            match &m {
                Msg::Ui(e) => self.outbox.push(crate::term::event_json(e)),
                Msg::Info(s) => self
                    .outbox
                    .push(serde_json::json!({"type":"info","text":s})),
                _ => {}
            }
        }
        match m {
            Msg::Wb(w) => {
                if let Some(wb) = self.wb.as_mut() {
                    wb.on_msg(w);
                }
            }
            Msg::Remote(r) => self.on_remote(r),
            Msg::Ui(e) => match e {
                UiEvent::Text(t) => self.push_text(&t),
                UiEvent::Reasoning(t) => {
                    if let Some(Item::Reasoning(s)) = self.items.last_mut() {
                        s.push_str(&t);
                    } else {
                        self.items.push(Item::Reasoning(t));
                    }
                }
                UiEvent::ToolStart(n) => self.items.push(Item::Tool {
                    name: n,
                    args: String::new(),
                    state: ToolState::Running,
                    title: String::new(),
                    text: String::new(),
                }),
                UiEvent::ToolCall { name, args } => {
                    let slot = self.items.iter_mut().rev().find_map(|i| match i {
                        Item::Tool {
                            name: n,
                            args: a,
                            state: ToolState::Running,
                            ..
                        } if *n == name && a.is_empty() => Some(a),
                        _ => None,
                    });
                    match slot {
                        Some(a) => *a = args,
                        None => self.items.push(Item::Tool {
                            name,
                            args,
                            state: ToolState::Running,
                            title: String::new(),
                            text: String::new(),
                        }),
                    }
                }
                UiEvent::ToolResult {
                    name,
                    title: t,
                    ok,
                    text: x,
                } => {
                    if let Some(Item::Tool { state, title, text, .. }) = self.items.iter_mut().rev().find(|i| matches!(i, Item::Tool { name: n, state: ToolState::Running, .. } if *n == name)) {
                        *state = if ok { ToolState::Ok } else { ToolState::Err }; *title = t.clone(); *text = x;
                    }
                    if name == "parity" {
                        self.parity = Some((ok && t.contains("PASS"), t.clone()));
                    }
                    self.refresh_ledger();
                }
                UiEvent::Todos(t) => self.todos = t,
                UiEvent::Compacted => self.items.push(Item::Info(
                    "older context summarised to fit the model's window".into(),
                )),
                UiEvent::Info(s) => self.items.push(Item::Info(s)),
                UiEvent::Usage(u) => {
                    self.tokens_in += u.prompt_tokens;
                    self.tokens_out += u.completion_tokens;
                    self.last_prompt = u.prompt_tokens;
                }
            },
            Msg::Perm(p) => {
                if self.auto_allow {
                    let _ = p.reply.send(Reply::Once);
                } else {
                    if self.remote_on {
                        self.outbox.push(serde_json::json!({"type":"perm","tool":p.req.tool,"target":p.req.target,"reason":p.req.reason}));
                    }
                    self.perm = Some(p);
                }
            }
            Msg::Info(s) => self.items.push(Item::Info(s)),
            Msg::Done(r) => {
                self.busy = false;
                match r {
                    Ok(o) => self.items.push(Item::Footer(format!(
                        "{} · {} · {} step{} · {} in / {} out{}",
                        self.agent,
                        self.model,
                        o.steps,
                        if o.steps == 1 { "" } else { "s" },
                        o.usage.prompt_tokens,
                        o.usage.completion_tokens,
                        match o.status {
                            Status::Done => "",
                            Status::MaxSteps => " · step limit",
                            Status::Cancelled => " · interrupted",
                            Status::Stuck => " · stopped: repeating",
                        }
                    ))),
                    Err(e) => self.items.push(Item::Error(e)),
                }
                if self.remote_on {
                    let line = match self.items.last() {
                        Some(Item::Footer(t)) | Some(Item::Error(t)) => t.clone(),
                        _ => String::new(),
                    };
                    self.outbox
                        .push(serde_json::json!({"type":"done","text":line}));
                }
            }
        }
    }

    /// Everything a newly connected remote client needs to draw the session.
    pub fn snapshot(&self) -> serde_json::Value {
        let items: Vec<serde_json::Value> = self.items.iter().map(|i| match i {
            Item::User(t) => serde_json::json!({"k":"user","text":t}),
            Item::Assistant(t) => serde_json::json!({"k":"assistant","text":t}),
            Item::Reasoning(t) => serde_json::json!({"k":"reasoning","text":t}),
            Item::Tool { name, args, state, title, text } => serde_json::json!({"k":"tool","name":name,"args":args,"state":match state { ToolState::Running => "running", ToolState::Ok => "ok", ToolState::Err => "err" },"title":title,"text":text}),
            Item::Info(t) => serde_json::json!({"k":"info","text":t}),
            Item::Error(t) => serde_json::json!({"k":"error","text":t}),
            Item::Footer(t) => serde_json::json!({"k":"footer","text":t}),
        }).collect();
        serde_json::json!({
            "state": self.state_json(),
            "items": items,
            "todos": self.todos,
            "perm": self.perm.as_ref().map(|p| serde_json::json!({"tool":p.req.tool,"target":p.req.target,"reason":p.req.reason})),
            "agents": self.agents,
        })
    }

    pub fn state_json(&self) -> serde_json::Value {
        serde_json::json!({"agent":self.agent,"model":self.model,"provider":self.provider,"deep":self.deep,"busy":self.busy,"cwd":self.cwd,"title":self.title,"context_pct":self.context_pct(),"ledger":self.ledger,"parity":self.parity.as_ref().map(|p| serde_json::json!({"ok":p.0,"text":p.1}))})
    }

    fn on_remote(&mut self, r: Remote) {
        match r {
            Remote::Snapshot(tx) => {
                let _ = tx.send(self.snapshot());
            }
            Remote::Send(t) => {
                if t.trim_start().starts_with('/') {
                    self.outbox.push(serde_json::json!({"type":"info","text":"slash commands are not sent remotely; use the agent/model/deep controls"}));
                } else if self.busy {
                    self.outbox.push(serde_json::json!({"type":"info","text":"the agent is still working; interrupt it first"}));
                } else {
                    self.input = t;
                    self.cursor = self.input.chars().count();
                    self.submit();
                    self.outbox.push(serde_json::json!({"type":"user","text":match self.items.iter().rev().find(|i| matches!(i, Item::User(_))) { Some(Item::User(t)) => t.clone(), _ => String::new() }}));
                }
            }
            Remote::Interrupt => self.cancel.store(true, Ordering::Relaxed),
            Remote::Perm(rep) => {
                if let Some(p) = self.perm.take() {
                    let _ = p.reply.send(rep);
                    self.outbox.push(serde_json::json!({"type":"perm_done"}));
                }
            }
            Remote::Agent(n) => self.slash(&format!("agent {n}")),
            Remote::Model(m) => self.slash(&format!("model {m}")),
            Remote::Deep(on) => self.slash(if on { "deep" } else { "clean" }),
            Remote::Clear => self.slash("clear"),
        }
    }

    // ---- input editing ----
    fn byte_at(&self, ci: usize) -> usize {
        self.input
            .char_indices()
            .nth(ci)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }
    fn len_chars(&self) -> usize {
        self.input.chars().count()
    }
    pub fn insert(&mut self, s: &str) {
        let b = self.byte_at(self.cursor);
        self.input.insert_str(b, s);
        self.slash_sel = 0;
        self.cursor += s.chars().count();
    }
    /// Called once at startup: a short pointer when nothing is set up yet.
    pub fn welcome(&mut self) {
        if self.key_missing(&self.provider.clone()) {
            self.items.push(Item::Info(format!(
                "No API key for {} yet. Type /key to enter it, /provider to pick another provider (local ones need no key), or /setup for the steps.",
                self.provider
            )));
        }
    }

    pub fn on_paste(&mut self, s: &str) {
        self.insert(&s.replace('\r', ""));
    }

    pub fn slash_matches(&self) -> Vec<(&'static str, &'static str)> {
        match self.input.strip_prefix('/') {
            Some(p) if !p.contains(char::is_whitespace) => SLASH
                .iter()
                .copied()
                .filter(|(n, _)| n.starts_with(p))
                .collect(),
            _ => vec![],
        }
    }

    /// True when the provider needs a key and none is stored, in env or in the config.
    pub fn key_missing(&self, prov: &str) -> bool {
        let Ok(r) = self.cfg.resolve(Some(&format!("{prov}/x"))) else {
            return false;
        };
        let p = r.provider;
        p.api_key.is_none()
            && p.api_key_env
                .as_deref()
                .is_some_and(|e| std::env::var(e).map_or(true, |v| v.is_empty()))
    }

    fn ask_key(&mut self, prov: &str) {
        self.key_for = Some(prov.to_string());
        self.input.clear();
        self.cursor = 0;
        self.items.push(Item::Info(format!(
            "Paste the API key for {prov} and press enter. It is hidden while you type, saved in your config folder (mode 600) and never sent anywhere but {prov}. esc cancels."
        )));
    }

    fn list_providers(&mut self) {
        let mut names: Vec<String> = portcode_llm::presets()
            .into_iter()
            .map(|p| p.name)
            .collect();
        for k in self.cfg.provider.keys() {
            if !names.contains(k) {
                names.push(k.clone());
            }
        }
        self.items.push(Item::Info(
            "providers (switch: /provider <name> [model], key: /key <name>):".into(),
        ));
        for n in names {
            let state = if self.key_missing(&n) {
                "needs a key → /key"
            } else if portcode_agent::config::stored_key(&n).is_some() {
                "key saved"
            } else {
                "ready"
            };
            let mark = if n == self.provider { "●" } else { "○" };
            self.items
                .push(Item::Info(format!("  {mark} {n:<11} {state}")));
        }
    }

    fn setup_guide(&mut self) {
        for l in [
            "First run, three steps:",
            "1. /provider  shows providers. /provider ollama qwen3:14b switches (local models need no key).",
            "2. /key  asks for the API key of the current provider (hidden). Or run `portcode auth set <provider>` in a shell.",
            "3. /models  lists what the provider serves. /model <provider>/<model> picks one. Then just type a task, or /create for a guided start.",
            "Check everything with `portcode doctor`. Updates: /update. More: /help.",
        ] {
            self.items.push(Item::Info(l.into()));
        }
    }

    fn complete_slash(&mut self) {
        let m = self.slash_matches();
        if let Some((n, _)) = m
            .get(self.slash_sel.min(m.len().saturating_sub(1)))
            .copied()
        {
            self.input = format!("/{n} ");
            self.cursor = self.len_chars();
            self.slash_sel = 0;
        }
    }

    fn cycle_agent(&mut self, dir: isize) {
        if self.busy || self.agents.is_empty() {
            return;
        }
        let i = self
            .agents
            .iter()
            .position(|a| *a == self.agent)
            .unwrap_or(0) as isize;
        let n = self.agents.len() as isize;
        let next = self
            .agents
            .get(((i + dir).rem_euclid(n)) as usize)
            .cloned()
            .unwrap_or_default();
        self.agent = next.clone();
        let _ = self.cmd_tx.send(Cmd::SetAgent(next));
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if let Some(p) = self.perm.take() {
            let reply = match k.code {
                KeyCode::Char('y') | KeyCode::Enter => Some(Reply::Once),
                KeyCode::Char('a') => Some(Reply::Always),
                KeyCode::Char('n') | KeyCode::Esc => Some(Reply::Reject),
                _ => None,
            };
            match reply {
                Some(r) => {
                    let _ = p.reply.send(r);
                    if self.remote_on {
                        self.outbox.push(serde_json::json!({"type":"perm_done"}));
                    }
                }
                None => self.perm = Some(p),
            }
            return;
        }
        if let Some(wz) = self.wiz.as_mut() {
            use super::wizard::Act;
            let allowed = self.cfg.allowed_dirs.clone();
            match wz.on_key(k, &self.root, &allowed) {
                Act::Cancel => self.wiz = None,
                Act::Start(l) => {
                    if self.busy {
                        wz.error = Some("The agent is still working. Press esc to close this, interrupt it, then try again.".into());
                    } else {
                        self.wiz = None;
                        if self.agent != l.agent {
                            self.agent = l.agent.into();
                            let _ = self.cmd_tx.send(Cmd::SetAgent(l.agent.into()));
                        }
                        self.deep = l.deep;
                        let _ = self.cmd_tx.send(Cmd::SetDeep(l.deep));
                        self.input = l.prompt;
                        self.cursor = self.len_chars();
                        self.submit();
                    }
                }
                Act::None => {}
            }
            return;
        }
        if let Some(wb) = self.wb.as_mut() {
            use super::workbench::{Act, Focus};
            if wb.focus == Focus::Chat {
                if k.code == KeyCode::Esc && !self.busy {
                    wb.focus = Focus::List;
                    return;
                }
            } else if !(k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
            {
                match wb.on_key(k) {
                    Act::Close => self.wb = None,
                    Act::Ask(q) => {
                        if self.busy {
                            self.items.push(Item::Info(
                                "still working — press esc to interrupt first".into(),
                            ));
                        } else {
                            wb.focus = Focus::Chat;
                            self.input = q;
                            self.cursor = self.len_chars();
                            self.submit();
                        }
                    }
                    Act::Chat | Act::None => {}
                }
                return;
            }
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match (k.code, ctrl, alt) {
            (KeyCode::Char('c'), true, _) => {
                if self.busy {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.items.push(Item::Info("interrupting…".into()));
                } else if !self.input.is_empty() {
                    self.input.clear();
                    self.cursor = 0;
                } else {
                    self.quit = true;
                }
            }
            (KeyCode::Char('d'), true, _) if self.input.is_empty() => self.quit = true,
            (KeyCode::Esc, ..) if self.key_for.is_some() => {
                self.key_for = None;
                self.input.clear();
                self.cursor = 0;
                self.items.push(Item::Info("key entry cancelled".into()));
            }
            (KeyCode::Esc, ..) => {
                if self.busy {
                    self.cancel.store(true, Ordering::Relaxed);
                } else {
                    self.scroll = 0;
                }
            }
            (KeyCode::Enter, _, true) | (KeyCode::Char('j'), true, _) => self.insert("\n"),
            (KeyCode::Enter, ..) => {
                // A partial command completes to the highlighted one; a full name runs.
                let m = self.slash_matches();
                let typed = self.input.trim().trim_start_matches('/');
                if !m.is_empty() && !m.iter().any(|(n, _)| *n == typed) {
                    self.complete_slash();
                } else {
                    self.submit();
                }
            }
            (KeyCode::Tab, ..) => {
                if !self.slash_matches().is_empty() {
                    self.complete_slash();
                } else {
                    self.cycle_agent(1);
                }
            }
            (KeyCode::BackTab, ..) => self.cycle_agent(-1),
            (KeyCode::Char('t'), true, _) => self.show_reasoning = !self.show_reasoning,
            (KeyCode::Char('b'), true, _) => self.sidebar = !self.sidebar,
            (KeyCode::Char('u'), true, _) => {
                let b = self.byte_at(self.cursor);
                self.input.replace_range(..b, "");
                self.cursor = 0;
            }
            (KeyCode::Char('w'), true, _) => {
                let chars: Vec<char> = self.input.chars().collect();
                let mut i = self.cursor;
                while i > 0 && chars.get(i - 1).is_some_and(|c| c.is_whitespace()) {
                    i -= 1;
                }
                while i > 0 && chars.get(i - 1).is_some_and(|c| !c.is_whitespace()) {
                    i -= 1;
                }
                let (a, b) = (self.byte_at(i), self.byte_at(self.cursor));
                self.input.replace_range(a..b, "");
                self.cursor = i;
            }
            (KeyCode::Char('a'), true, _) | (KeyCode::Home, ..) => self.cursor = 0,
            (KeyCode::Char('e'), true, _) | (KeyCode::End, ..) => self.cursor = self.len_chars(),
            (KeyCode::Left, ..) => self.cursor = self.cursor.saturating_sub(1),
            (KeyCode::Right, ..) => self.cursor = (self.cursor + 1).min(self.len_chars()),
            (KeyCode::Backspace, ..) => {
                if self.cursor > 0 {
                    let (a, b) = (self.byte_at(self.cursor - 1), self.byte_at(self.cursor));
                    self.input.replace_range(a..b, "");
                    self.cursor -= 1;
                }
            }
            (KeyCode::Delete, ..) => {
                if self.cursor < self.len_chars() {
                    let (a, b) = (self.byte_at(self.cursor), self.byte_at(self.cursor + 1));
                    self.input.replace_range(a..b, "");
                }
            }
            (KeyCode::Up, ..) if !self.slash_matches().is_empty() => {
                let n = self.slash_matches().len();
                self.slash_sel = (self.slash_sel.min(n - 1) + n - 1) % n;
            }
            (KeyCode::Down, ..) if !self.slash_matches().is_empty() => {
                let n = self.slash_matches().len();
                self.slash_sel = (self.slash_sel.min(n - 1) + 1) % n;
            }
            (KeyCode::Up, ..) => self.history_step(-1),
            (KeyCode::Down, ..) => self.history_step(1),
            (KeyCode::PageUp, ..) => self.scroll = self.scroll.saturating_add(10),
            (KeyCode::PageDown, ..) => self.scroll = self.scroll.saturating_sub(10),
            (KeyCode::Char(c), false, _) | (KeyCode::Char(c), true, true) => {
                self.insert(&c.to_string())
            }
            _ => {}
        }
    }

    pub fn on_scroll(&mut self, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_add(3)
        } else {
            self.scroll.saturating_sub(3)
        };
    }

    fn history_step(&mut self, d: isize) {
        if self.history.is_empty() || self.input.contains('\n') {
            return;
        }
        let pos = match (self.hist_pos, d) {
            (None, -1) => Some(self.history.len() - 1),
            (Some(p), -1) => Some(p.saturating_sub(1)),
            (Some(p), 1) if p + 1 < self.history.len() => Some(p + 1),
            _ => None,
        };
        self.hist_pos = pos;
        self.input = pos
            .and_then(|p| self.history.get(p).cloned())
            .unwrap_or_default();
        self.cursor = self.len_chars();
    }

    pub fn submit(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(prov) = self.key_for.take() {
            self.input.clear();
            self.cursor = 0;
            match portcode_agent::config::store_key(&prov, &text) {
                Ok(p) => {
                    self.items.push(Item::Info(format!(
                        "saved the key for {prov} in {} (mode 600). Not shown again.",
                        p.display()
                    )));
                    if prov == self.provider {
                        let _ = self
                            .cmd_tx
                            .send(Cmd::SetModel(format!("{prov}/{}", self.model)));
                    }
                }
                Err(e) => self
                    .items
                    .push(Item::Error(format!("could not save the key: {e}"))),
            }
            return;
        }
        if let Some(cmd) = text.strip_prefix('/') {
            self.input.clear();
            self.cursor = 0;
            self.slash(cmd);
            return;
        }
        if self.busy {
            self.items.push(Item::Info(
                "still working — press esc to interrupt first".into(),
            ));
            return;
        }
        self.history.push(text.clone());
        self.hist_pos = None;
        self.input.clear();
        self.cursor = 0;
        self.scroll = 0;
        self.title.get_or_insert_with(|| {
            let first = text.lines().next().unwrap_or("");
            if first.chars().count() > 44 {
                format!("{}…", first.chars().take(43).collect::<String>().trim_end())
            } else {
                first.to_string()
            }
        });
        self.items.push(Item::User(text.clone()));
        self.busy = true;
        self.cancel.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(Cmd::Send(text));
    }

    fn slash(&mut self, cmd: &str) {
        let mut p = cmd.split_whitespace();
        let (name, arg) = (p.next().unwrap_or(""), p.next());
        let arg2 = p.next();
        match (name, arg) {
            ("provider", None) => self.list_providers(),
            ("provider", Some(pv)) => {
                let model = arg2.map(String::from).or_else(|| {
                    if pv == self.provider {
                        Some(self.model.clone())
                    } else {
                        self.cfg
                            .model
                            .as_deref()
                            .and_then(|m| m.strip_prefix(&format!("{pv}/")))
                            .map(String::from)
                    }
                });
                match model {
                    Some(m) => {
                        self.slash(&format!("model {pv}/{m}"));
                        if self.provider == pv && self.key_missing(pv) {
                            self.ask_key(pv);
                        }
                    }
                    None => self.items.push(Item::Error(format!(
                        "give a model too: /provider {pv} <model>, for example /provider {pv} qwen3:14b"
                    ))),
                }
            }
            ("key", a) => {
                let pv = a.map(String::from).unwrap_or_else(|| self.provider.clone());
                self.ask_key(&pv);
            }
            ("setup", _) => self.setup_guide(),
            ("update", _) => {
                self.items.push(Item::Info("checking for updates…".into()));
                let _ = self.cmd_tx.send(Cmd::Update);
            }
            ("exit" | "quit" | "q", _) => self.quit = true,
            ("help", _) => self.items.push(Item::Info("/setup: first-run steps · /provider · /key · tab: switch agent · /workbench: deep code view · enter: send · alt+enter: newline · esc: interrupt · ctrl+t: thinking · ctrl+b: sidebar · pgup/pgdn: scroll · /: commands".into())),
            ("agent", Some(a)) => { if self.agents.iter().any(|x| x == a) { if !self.busy { self.agent = a.into(); let _ = self.cmd_tx.send(Cmd::SetAgent(a.into())); } } else { self.items.push(Item::Error(format!("unknown agent '{a}'. Try: {}", self.agents.join(", ")))); } }
            ("agent", None) => self.items.push(Item::Info(format!("agents: {} (tab cycles)", self.agents.join(", ")))),
            ("model", Some(m)) => match self.cfg.resolve(Some(m)) {
                Ok(r) => { self.model = r.model; self.provider = r.provider.name; self.ctx_window = r.profile.context_window; let _ = self.cmd_tx.send(Cmd::SetModel(m.into())); }
                Err(e) => self.items.push(Item::Error(e)),
            },
            ("model", None) => self.items.push(Item::Info(format!("{}/{} (context {} tokens)", self.provider, self.model, self.ctx_window))),
            ("models", _) => { let _ = self.cmd_tx.send(Cmd::ListModels); }
            ("clear", _) => { self.items.clear(); self.todos.clear(); self.title = None; self.tokens_in = 0; self.tokens_out = 0; self.last_prompt = 0; let _ = self.cmd_tx.send(Cmd::Clear); }
            ("resume", _) => { let _ = self.cmd_tx.send(Cmd::Resume); }
            ("deep", _) => { self.deep = true; let _ = self.cmd_tx.send(Cmd::SetDeep(true)); self.items.push(Item::Info("deep mode: disasm, emulate, decompile and hexdump are available. Use it only on software you own or may examine.".into())); }
            ("clean" | "clean-room", _) => { self.deep = false; let _ = self.cmd_tx.send(Cmd::SetDeep(false)); self.items.push(Item::Info("clean-room mode: the original is observed from outside only.".into())); }
            ("engines", _) => {
                let list: Vec<String> = portcode_agent::engines::ENGINES.iter().map(|e| format!("{} {}", if portcode_agent::engines::detect(e).is_some() { "✓" } else { "·" }, e.id)).collect();
                self.items.push(Item::Info(format!("engines: {} — install: portcode engines install <id>", list.join("  "))));
            }
            ("create" | "new", _) => self.wiz = Some(super::wizard::Wizard::default()),
            ("workbench" | "wb", a) => self.open_workbench(a),
            ("remote", a) => self.remote_req = Some(a.unwrap_or("").to_string()),
            ("thinking", _) => self.show_reasoning = !self.show_reasoning,
            ("sidebar", _) => self.sidebar = !self.sidebar,
            ("tools", _) => self.items.push(Item::Info("tools: read ls glob grep write edit bash todowrite webfetch skill task survey pin evidence_add evidence_list parity screenshot site_build scaffold; deep: re_status re_install disasm emulate decompile functions hexdump unity dotnet unity_assets (+ MCP)".into())),
            ("ledger", _) => {
                self.refresh_ledger();
                let (o, i, u) = self.ledger;
                self.items.push(Item::Info(format!("evidence: {o} observation · {i} inference · {u} unknown{}", self.unknowns.first().map(|x| format!(" — open: {x}")).unwrap_or_default())));
            }
            (other, _) => self.items.push(Item::Error(format!("unknown command /{other}. Press / to see commands"))),
        }
    }
}

/// Short human summary of a tool call's arguments (the preview may be truncated JSON, so extract keys textually).
pub fn summarize(name: &str, args: &str) -> String {
    let pick = |key: &str| -> Option<String> {
        let pat = format!("\"{key}\":\"");
        let i = args.find(&pat)? + pat.len();
        let rest = args.get(i..)?;
        let end = rest.find('"').unwrap_or(rest.len());
        Some(rest.get(..end)?.to_string())
    };
    if matches!(
        name,
        "disasm" | "emulate" | "hexdump" | "decompile" | "dotnet" | "unity_assets" | "unity"
    ) {
        let what = pick("file").unwrap_or_else(|| "hex bytes".into());
        let how: Vec<String> = ["arch", "mode", "query"]
            .iter()
            .filter_map(|k| pick(k))
            .collect();
        return if how.is_empty() {
            what
        } else {
            format!("{what} · {}", how.join(" "))
        };
    }
    let keys: &[&str] = match name {
        "bash" => &["description", "command"],
        "grep" | "glob" => &["pattern"],
        "webfetch" => &["url"],
        "skill" => &["name"],
        "task" => &["description", "prompt"],
        "screenshot" => &["name"],
        "evidence_add" => &["statement"],
        _ => &["path", "pattern", "command", "name", "subject"],
    };
    keys.iter().find_map(|k| pick(k)).unwrap_or_else(|| {
        if args.len() > 60 {
            format!("{}…", args.chars().take(60).collect::<String>())
        } else {
            args.to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        std::mem::forget(_rx);
        App::new(
            Config::default(),
            std::env::temp_dir(),
            "build",
            "m",
            "p",
            1000,
            "~".into(),
            tx,
            Arc::new(AtomicBool::new(false)),
        )
    }
    #[test]
    fn arrows_move_in_the_slash_menu_and_enter_completes() {
        let mut a = app();
        a.insert("/m");
        let n = a.slash_matches().len();
        assert!(n >= 2);
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.slash_sel, 1);
        a.on_key(key(KeyCode::Up));
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.slash_sel, n - 1);
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Down));
        let want = a.slash_matches()[1].0;
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.input, format!("/{want} "));
    }

    #[test]
    fn key_entry_is_hidden_and_cancellable() {
        let mut a = app();
        a.input = "/key ollama".into();
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.key_for.as_deref(), Some("ollama"));
        a.on_key(key(KeyCode::Esc));
        assert!(a.key_for.is_none() && a.input.is_empty());
        a.input = "/provider".into();
        a.on_key(key(KeyCode::Enter));
        assert!(a
            .items
            .iter()
            .any(|i| matches!(i, Item::Info(t) if t.contains("ollama"))));
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn typing_editing_and_submit() {
        let mut a = app();
        for c in "héllo".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        a.on_key(key(KeyCode::Left));
        a.on_key(key(KeyCode::Backspace));
        assert_eq!(a.input, "hélo");
        a.on_key(key(KeyCode::Enter));
        assert!(
            a.busy
                && matches!(a.items.first(), Some(Item::User(t)) if t == "hélo")
                && a.input.is_empty()
        );
    }
    #[test]
    fn tab_cycles_agents_and_slash_completes() {
        let mut a = app();
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.agent, "port");
        a.insert("/ag");
        assert_eq!(a.slash_matches().first().map(|m| m.0), Some("agent"));
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.input, "/agent ");
    }
    #[test]
    fn tool_events_update_in_place() {
        let mut a = app();
        a.on_msg(Msg::Ui(UiEvent::ToolStart("read".into())));
        a.on_msg(Msg::Ui(UiEvent::ToolCall {
            name: "read".into(),
            args: "{\"path\":\"a.rs\"}".into(),
        }));
        a.on_msg(Msg::Ui(UiEvent::ToolResult {
            name: "read".into(),
            title: "a.rs".into(),
            ok: true,
            text: "1\tx".into(),
        }));
        assert_eq!(a.items.len(), 1);
        assert!(
            matches!(&a.items[0], Item::Tool { state: ToolState::Ok, args, .. } if args.contains("a.rs"))
        );
    }
    #[test]
    fn permission_dialog_captures_keys() {
        let mut a = app();
        let (tx, mut rx) = oneshot::channel();
        a.on_msg(Msg::Perm(PermAsk {
            req: Request {
                tool: "bash".into(),
                target: "ls".into(),
                reason: String::new(),
            },
            reply: tx,
        }));
        a.on_key(key(KeyCode::Char('x'))); // ignored, dialog stays
        assert!(a.perm.is_some() && a.input.is_empty());
        a.on_key(key(KeyCode::Char('a')));
        assert_eq!(rx.try_recv().ok(), Some(Reply::Always));
    }
    #[test]
    fn summarize_handles_truncated_json() {
        assert_eq!(
            summarize("read", "{\"path\":\"src/main.rs\",\"offs"),
            "src/main.rs"
        );
        assert_eq!(
            summarize(
                "bash",
                "{\"command\":\"cargo test\",\"description\":\"run tests\"}"
            ),
            "run tests"
        );
    }

    #[test]
    fn remote_commands_drive_the_same_state_as_the_keyboard() {
        let mut a = app();
        a.remote_on = true;
        a.on_msg(Msg::Remote(Remote::Send("/exit".into())));
        assert!(!a.quit, "slash commands must not be accepted remotely");
        a.on_msg(Msg::Remote(Remote::Send("hello".into())));
        assert!(a.busy && matches!(a.items.last(), Some(Item::User(t)) if t == "hello"));
        assert!(a.outbox.iter().any(|v| v["type"] == "user"));
        let (tx, rx) = oneshot::channel();
        a.on_msg(Msg::Remote(Remote::Snapshot(tx)));
        let snap = rx.blocking_recv().unwrap();
        assert_eq!(snap["state"]["busy"], true);
        assert_eq!(snap["items"][0]["k"], "user");
        a.on_msg(Msg::Remote(Remote::Deep(true)));
        assert!(a.deep);
    }

    #[test]
    fn workbench_needs_deep_and_owns_the_keys_until_closed() {
        let mut a = app();
        a.input = "/workbench original/x".into();
        a.submit();
        assert!(a.wb.is_none(), "clean-room must not open the workbench");
        assert!(matches!(a.items.last(), Some(Item::Error(e)) if e.contains("deep")));
        a.deep = true;
        a.input = "/workbench original/x".into();
        a.submit();
        assert!(a.wb.is_some());
        a.on_key(key(KeyCode::Char('/')));
        a.on_key(key(KeyCode::Char('q')));
        assert!(
            a.wb.is_some() && a.input.is_empty(),
            "keys go to the filter, not the prompt or close"
        );
        a.on_key(key(KeyCode::Esc));
        a.on_key(key(KeyCode::Char('q')));
        assert!(a.wb.is_none());
    }
    #[test]
    fn remote_answers_permission_asks_once() {
        let mut a = app();
        a.remote_on = true;
        let (tx, mut rx) = oneshot::channel();
        a.on_msg(Msg::Perm(PermAsk {
            req: Request {
                tool: "bash".into(),
                target: "ls".into(),
                reason: String::new(),
            },
            reply: tx,
        }));
        assert!(a.outbox.iter().any(|v| v["type"] == "perm"));
        a.on_msg(Msg::Remote(Remote::Perm(Reply::Once)));
        assert!(matches!(rx.try_recv(), Ok(Reply::Once)));
        assert!(a.perm.is_none());
        a.on_msg(Msg::Remote(Remote::Perm(Reply::Always))); // nothing pending: ignored
    }
}
