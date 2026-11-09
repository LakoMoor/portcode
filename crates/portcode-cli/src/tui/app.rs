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
    /// A background job (compaction) finished; the prompt is free again.
    Idle,
    /// A session was loaded: id and its messages, to redraw.
    Resumed(String, Vec<portcode_llm::Message>),
    /// Result of a `/connect` test: the served models, or why it failed.
    Connected(Result<Vec<String>, String>),
    /// The context window (tokens) the runner is using for the current model.
    Window(u32),
    /// A new version was installed: leave and start it, resuming this session when there is one.
    Restart(Option<String>),
    /// Agent, provider, model, context window and deep flag restored with a resumed session.
    Restored(String, String, String, u32, bool),
    /// A guessed first message for the empty prompt (Tab inserts it).
    Suggest(String),
    /// Open the session picker.
    Pick(Vec<PickRow>),
    Models(Vec<String>),
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
    /// Summarise the conversation to free context (`/compact`).
    Compact,
    /// Open a session (id, prefix, number or `last`).
    Resume(String),
    Sessions,
    ListModels,
    Update,
    /// (`style` | `effort`, value) for the current model.
    Tune(String, String),
    SetDeep(bool),
    /// `/connect`: contact an endpoint (base_url, key) and list its models.
    ConnectTest(String, String),
    /// A provider was added: reload the configuration so `provider/model` resolves.
    Reload(Box<Config>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Ok,
    Err,
}

/// One row of the session picker.
pub struct PickRow {
    pub id: String,
    pub title: String,
    pub messages: usize,
    pub ago: String,
    pub current: bool,
}

pub struct Picker {
    pub rows: Vec<PickRow>,
    pub sel: usize,
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
    /// Aligned block: `# heading` lines and `key<TAB>description` rows.
    Note(String),
    Error(String),
    Footer(String),
}

pub struct MenuItem {
    pub label: String,
    pub desc: String,
    pub fill: String,
    pub exact: bool,
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
    (
        "connect",
        "add an OpenAI-compatible endpoint: address, key, model",
    ),
    ("setup", "first-run guide: provider, key, model"),
    (
        "settings",
        "all switches on one screen: mode, team, theme, context…",
    ),
    (
        "team",
        "multi-agent team (PM, tech lead, workers): on or off",
    ),
    ("venv", "Python virtual environments: on or off"),
    (
        "notify",
        "bell and desktop notice when a long task ends: on or off",
    ),
    (
        "context",
        "context window of this model: /context 32k, 128k, auto",
    ),
    (
        "yolo",
        "allow everything without asking (toggle, or on/off)",
    ),
    ("update", "install the newest PortCode release"),
    ("compact", "summarise the conversation to free up context"),
    (
        "export",
        "save a report of this session to .portcode/export/",
    ),
    ("new", "new session (same as /clear)"),
    ("clear", "new session"),
    ("agents", "list agents"),
    ("sessions", "list saved sessions of this project"),
    ("resume", "open a session: /resume <number|id|last>"),
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
    ("style", "prompt style: auto, compact, full, qwen38"),
    ("effort", "Qwen3.8 thinking: xhigh, medium, low, none"),
    ("theme", "dark or light: /theme light"),
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
    /// Ghost text in the empty prompt; Tab inserts it, Esc or typing drops it.
    pub suggestion: Option<String>,
    /// Full-screen list of saved sessions (`--resume`, `/resume`).
    pub picker: Option<Picker>,
    /// Further permission asks that arrived while one is open (parallel team workers).
    pub perm_queue: std::collections::VecDeque<PermAsk>,
    pub scroll: usize,
    pub show_reasoning: bool,
    /// Show every tool's output, not only the latest ones (ctrl+o).
    pub full_results: bool,
    /// Ring the terminal bell / send a desktop notification when a long turn ends or a permission is needed.
    pub notify_on: bool,
    /// Steering inbox of the running agent (set by the event loop).
    pub steer: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
    /// Text for the event loop to announce (`App` does no terminal I/O).
    pub bell: Option<String>,
    turn_started: Option<std::time::Instant>,
    pub sidebar: bool,
    pub ledger: (usize, usize, usize),
    pub unknowns: Vec<String>,
    pub cwd: String,
    pub title: Option<String>,
    pub spinner: usize,
    pub quit: bool,
    /// Set when an update was installed: the session to resume in the new process.
    pub restart: Option<Option<String>>,
    pub wb: Option<super::workbench::Wb>,
    pub wiz: Option<super::wizard::Wizard>,
    pub settings: Option<super::settings::Settings>,
    pub conn: Option<super::connect::Connect>,
    pub auto_allow: bool,
    /// Highlighted row of the `/` menu (arrows move it).
    pub slash_sel: usize,
    /// First of two escapes while a request runs; a second within 2 s cancels it.
    pub esc_at: Option<std::time::Instant>,
    /// Remember `/model` choices for the next start (only the real terminal UI turns this on).
    pub persist: bool,
    /// Model ids the provider reported (for `/model` completion).
    pub models: Vec<String>,
    /// One-line notice on the home screen (no API key yet).
    pub hint: Option<String>,
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
    pub(super) cfg: Config,
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
            agents: portcode_agent::agents::visible(cfg.team_enabled())
                .iter()
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
            full_results: false,
            notify_on: false,
            steer: None,
            bell: None,
            turn_started: None,
            sidebar: true,
            ledger: (0, 0, 0),
            unknowns: vec![],
            cwd: cwd_label,
            title: None,
            spinner: 0,
            quit: false,
            restart: None,
            wb: None,
            wiz: None,
            settings: None,
            conn: None,
            auto_allow: false,
            perm_queue: Default::default(),
            suggestion: None,
            picker: None,
            slash_sel: 0,
            esc_at: None,
            persist: false,
            models: vec![],
            hint: None,
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

    /// The user interrupted: queued messages go back into the prompt instead of being sent.
    fn unqueue_steer(&mut self) {
        let left: Vec<String> = match &self.steer {
            Some(q) => q
                .lock()
                .map(|mut q| q.drain(..).collect())
                .unwrap_or_default(),
            None => return,
        };
        if !left.is_empty() && self.input.is_empty() {
            self.input = left.join("\n\n");
            self.cursor = self.input.chars().count();
            self.items.push(Item::Info(
                "interrupted: your queued message is back in the prompt".into(),
            ));
        }
    }

    /// Messages queued for steering that the agent never got to (the turn ended first) go out as a new turn.
    fn flush_steer(&mut self) {
        let left: Vec<String> = match &self.steer {
            Some(q) => q
                .lock()
                .map(|mut q| q.drain(..).collect())
                .unwrap_or_default(),
            None => return,
        };
        if left.is_empty() {
            return;
        }
        // they are already on screen as user messages: send without echoing them again
        let text = left.join("\n\n");
        self.busy = true;
        self.turn_started = Some(std::time::Instant::now());
        self.cancel.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(Cmd::Send(text));
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
                    if self.perm.is_some() {
                        self.perm_queue.push_back(p);
                    } else {
                        if self.notify_on {
                            self.bell =
                                Some(format!("PortCode: {} needs your permission", p.req.tool));
                        }
                        self.perm = Some(p);
                    }
                }
            }
            Msg::Info(s) => self.items.push(Item::Info(s)),
            Msg::Suggest(s) => {
                if self.input.is_empty() && self.items.is_empty() {
                    self.suggestion = Some(s);
                }
            }
            Msg::Idle => {
                // the turn is over: a tool still shown as running (cancelled, or stopped) must not keep spinning
                for i in self.items.iter_mut() {
                    if let Item::Tool { state, .. } = i {
                        if *state == ToolState::Running {
                            *state = ToolState::Err;
                        }
                    }
                }
                self.busy = false;
                self.last_prompt = 0;
                self.flush_steer();
            }
            Msg::Pick(rows) => self.picker = Some(Picker { rows, sel: 0 }),
            Msg::Resumed(id, m) => self.load_history(&id, &m),
            Msg::Restart(sid) => {
                self.items
                    .push(Item::Info("restarting into the new version…".into()));
                self.restart = Some(sid);
                self.quit = true;
            }
            Msg::Window(n) => self.ctx_window = n,
            Msg::Connected(r) => {
                if let Some(c) = self.conn.as_mut() {
                    c.tested(r);
                }
            }
            Msg::Restored(agent, provider, model, window, deep) => {
                self.agent = agent;
                self.provider = provider;
                self.model = model;
                self.ctx_window = window;
                self.deep = deep;
            }
            Msg::Models(m) => self.models = m,
            Msg::Done(r) => {
                self.busy = false;
                let cancelled = matches!(&r, Ok(o) if o.status == Status::Cancelled);
                if self.notify_on
                    && self
                        .turn_started
                        .take()
                        .is_some_and(|t| t.elapsed().as_secs() >= 20)
                {
                    self.bell = Some("PortCode: the task is finished".into());
                }
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
                if cancelled {
                    self.unqueue_steer();
                } else {
                    self.flush_steer();
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
            Item::Info(t) | Item::Note(t) => serde_json::json!({"k":"info","text":t.replace('\t', "  ")}),
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

    /// A readable report of the session: goal, what was said and run, todo progress, evidence counts and the parity verdict.
    pub fn export_markdown(&self) -> String {
        let mut o = format!(
            "# {}\n\n`{}` · {}/{}\n\n",
            self.title
                .clone()
                .unwrap_or_else(|| "PortCode session".into()),
            self.agent,
            self.provider,
            self.model
        );
        if !self.todos.is_empty() {
            let done = self
                .todos
                .iter()
                .filter(|t| t.status == "completed")
                .count();
            o += &format!("## Checklist ({done} of {} done)\n\n", self.todos.len());
            for t in &self.todos {
                o += &format!(
                    "- [{}] {}\n",
                    if t.status == "completed" { "x" } else { " " },
                    t.content
                );
            }
            o += "\n";
        }
        let (seen, inferred, open) = self.ledger;
        o += &format!("## Evidence\n\n{seen} observed, {inferred} inferred, {open} unknown.\n\n");
        for u in &self.unknowns {
            o += &format!("- unknown: {u}\n");
        }
        o += "\n## Proof\n\n";
        match &self.parity {
            Some((ok, t)) => {
                o += &format!(
                    "{}\n\n```\n{}\n```\n\n",
                    if *ok { "PASS" } else { "FAIL" },
                    t.trim()
                )
            }
            None => {
                o += "No parity run yet. Nothing here claims the port matches the original.\n\n"
            }
        }
        o += "## Transcript\n\n";
        for it in &self.items {
            match it {
                Item::User(t) => o += &format!("**You:** {t}\n\n"),
                Item::Assistant(t) => o += &format!("{t}\n\n"),
                Item::Tool {
                    name, args, state, ..
                } => {
                    let mark = match state {
                        ToolState::Ok => "ok",
                        ToolState::Err => "failed",
                        ToolState::Running => "stopped",
                    };
                    let a: String = args.chars().take(120).collect();
                    o += &format!("- `{name}` {a} ({mark})\n");
                }
                Item::Error(t) => o += &format!("> error: {t}\n\n"),
                _ => {}
            }
        }
        o
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
                    self.perm = self.perm_queue.pop_front();
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
        let p = self.provider.clone();
        if self.key_missing(&p) {
            self.hint = Some(format!(
                "No API key for {p} yet: type /key to enter it, or /provider to pick another (local ones need no key)"
            ));
        }
    }

    pub fn on_paste(&mut self, s: &str) {
        if let Some(c) = self.conn.as_mut() {
            c.paste(s);
            return;
        }
        self.insert(&s.replace('\r', ""));
    }

    /// Rows of the pop-up above the prompt: commands while you type `/name`, then choices for the
    /// argument of `/provider`, `/key`, `/agent` and `/model`. `fill` is what tab or enter puts in the prompt.
    pub fn menu(&self) -> Vec<MenuItem> {
        let m = self.slash_matches();
        if !m.is_empty() {
            return m
                .into_iter()
                .map(|(n, d)| MenuItem {
                    label: format!("/{n}"),
                    desc: d.to_string(),
                    fill: format!("/{n} "),
                    exact: self.input.trim().trim_start_matches('/') == n,
                })
                .collect();
        }
        let Some(rest) = self.input.strip_prefix('/') else {
            return vec![];
        };
        let mut it = rest.splitn(2, ' ');
        let (cmd, arg) = (it.next().unwrap_or(""), it.next());
        let Some(arg) = arg.filter(|a| !a.contains(' ')) else {
            return vec![];
        };
        let rows: Vec<(String, String)> = match cmd {
            "provider" | "key" => {
                let mut names: Vec<String> = portcode_llm::presets()
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
                for k in self.cfg.provider.keys() {
                    if !names.contains(k) {
                        names.push(k.clone());
                    }
                }
                names
                    .into_iter()
                    .map(|n| {
                        let d = if let Some(cli) = portcode_agent::backend::cli_for(&n) {
                            if portcode_agent::backend::find_binary(cli).is_some() {
                                "subscription"
                            } else {
                                "CLI not installed"
                            }
                        } else if self.key_missing(&n) {
                            "needs a key"
                        } else if n == self.provider {
                            "current"
                        } else {
                            "ready"
                        };
                        (n, d.to_string())
                    })
                    .collect()
            }
            "agent" => self
                .agents
                .iter()
                .map(|n| (n.clone(), String::new()))
                .collect(),
            "model" => self
                .models
                .iter()
                .map(|n| (format!("{}/{n}", self.provider), String::new()))
                .collect(),
            _ => vec![],
        };
        rows.into_iter()
            .filter(|(n, _)| {
                n.starts_with(arg) || n.split_once('/').is_some_and(|(_, m)| m.starts_with(arg))
            })
            .map(|(n, d)| MenuItem {
                fill: format!("/{cmd} {n}{}", if cmd == "provider" { " " } else { "" }),
                exact: n == arg,
                label: n,
                desc: d,
            })
            .collect()
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

    /// `/connect` finished: keep the key private, record the address, reload the config and switch to it.
    fn connect_save(&mut self, s: super::connect::Save) {
        if self.busy {
            if let Some(c) = self.conn.as_mut() {
                c.error = Some(
                    "The agent is still working. Press esc to interrupt it, then save.".into(),
                );
            }
            return;
        }
        if !s.key.is_empty() {
            if let Err(e) = portcode_agent::config::store_key(&s.name, &s.key) {
                if let Some(c) = self.conn.as_mut() {
                    c.error = Some(format!("could not save the key: {e}"));
                }
                return;
            }
        }
        if let Err(e) = portcode_agent::config::save_provider(&s.name, &s.url) {
            if let Some(c) = self.conn.as_mut() {
                c.error = Some(e);
            }
            return;
        }
        match Config::load(&self.root) {
            Ok(cfg) => {
                self.cfg = cfg.clone();
                let _ = self.cmd_tx.send(Cmd::Reload(Box::new(cfg)));
            }
            Err(e) => {
                if let Some(c) = self.conn.as_mut() {
                    c.error = Some(e);
                }
                return;
            }
        }
        self.conn = None;
        self.items.push(Item::Info(format!(
            "connected {} ({}). Using {}/{}.",
            s.name, s.url, s.name, s.model
        )));
        self.slash(&format!("model {}/{}", s.name, s.model));
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
            let state = if let Some(cli) = portcode_agent::backend::cli_for(&n) {
                if portcode_agent::backend::find_binary(cli).is_some() {
                    "uses your subscription"
                } else {
                    "CLI not installed"
                }
            } else if self.key_missing(&n) {
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

    /// Replace the transcript with a loaded session (text and tool calls; reasoning is not kept on screen).
    pub fn load_history(&mut self, id: &str, msgs: &[portcode_llm::Message]) {
        use portcode_llm::Role;
        self.items.clear();
        self.todos = portcode_agent::run::last_todos(msgs);
        self.title = None;
        // up/down in the prompt walks what was typed in this session, not only since this launch
        self.history = msgs
            .iter()
            .filter(|m| m.role == Role::User)
            .filter_map(|m| m.content.as_deref())
            .map(str::trim)
            .filter(|t| {
                !t.is_empty()
                    && !t.starts_with('[')
                    && !["Not finished.", "Your reply ", "You stopped, but"]
                        .iter()
                        .any(|p| t.starts_with(p))
            })
            .map(String::from)
            .fold(Vec::new(), |mut v: Vec<String>, t| {
                if v.last() != Some(&t) {
                    v.push(t);
                }
                v
            });
        self.hist_pos = None;
        let mut results: std::collections::HashMap<&str, &str> = Default::default();
        for m in msgs.iter().filter(|m| m.role == Role::Tool) {
            if let (Some(i), Some(c)) = (m.tool_call_id.as_deref(), m.content.as_deref()) {
                results.insert(i, c);
            }
        }
        for m in msgs {
            match m.role {
                Role::User => {
                    if let Some(t) = m.content.as_deref().filter(|t| !t.trim().is_empty()) {
                        self.items.push(Item::User(t.to_string()));
                    }
                }
                Role::Assistant => {
                    if let Some(t) = m.content.as_deref().filter(|t| !t.trim().is_empty()) {
                        self.items.push(Item::Assistant(t.to_string()));
                    }
                    for c in &m.tool_calls {
                        let out = results.get(c.id.as_str()).copied().unwrap_or("");
                        self.items.push(Item::Tool {
                            name: c.function.name.clone(),
                            args: c.function.arguments.clone(),
                            state: if out.starts_with("Error") {
                                ToolState::Err
                            } else {
                                ToolState::Ok
                            },
                            title: String::new(),
                            text: out.chars().take(2000).collect(),
                        });
                    }
                }
                _ => {}
            }
        }
        if self.title.is_none() {
            self.title = msgs.iter().find_map(|m| {
                (m.role == Role::User)
                    .then(|| m.content.clone())
                    .flatten()
                    .filter(|t| !t.starts_with("[Summary of the earlier"))
                    .map(|t| {
                        t.split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .chars()
                            .take(50)
                            .collect()
                    })
            });
        }
        self.scroll = 0;
        self.items.push(Item::Info(format!(
            "resumed {id}: {} messages. Keep going below, or /sessions to switch.",
            msgs.len()
        )));
    }

    /// One escape was pressed within the last two seconds.
    pub fn esc_armed(&self) -> bool {
        self.esc_at
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(2))
    }

    fn complete_slash(&mut self) {
        let m = self.menu();
        if let Some(it) = m.get(self.slash_sel.min(m.len().saturating_sub(1))) {
            self.input = it.fill.clone();
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
            if k.code == KeyCode::Char('A') {
                // allow-all: approve this one and everything after it (same as /yolo on)
                let _ = p.reply.send(Reply::Once);
                if self.remote_on {
                    self.outbox.push(serde_json::json!({"type":"perm_done"}));
                }
                self.perm = None;
                self.slash("yolo on");
                return;
            }
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
                    self.perm = self.perm_queue.pop_front();
                }
                None => self.perm = Some(p),
            }
            return;
        }
        if self.settings.is_some() {
            self.settings_key(k);
            return;
        }
        if let Some(p) = self.picker.as_mut() {
            let n = p.rows.len().max(1);
            match k.code {
                KeyCode::Up | KeyCode::Char('k') => p.sel = (p.sel + n - 1) % n,
                KeyCode::Down | KeyCode::Char('j') => p.sel = (p.sel + 1) % n,
                KeyCode::PageUp => p.sel = p.sel.saturating_sub(8),
                KeyCode::PageDown => p.sel = (p.sel + 8).min(n - 1),
                KeyCode::Home => p.sel = 0,
                KeyCode::End => p.sel = n - 1,
                KeyCode::Enter => {
                    if let Some(row) = p.rows.get(p.sel) {
                        let _ = self.cmd_tx.send(Cmd::Resume(row.id.clone()));
                    }
                    self.picker = None;
                }
                KeyCode::Esc | KeyCode::Char('q') => self.picker = None,
                _ => {}
            }
            return;
        }
        if let Some(cn) = self.conn.as_mut() {
            use super::connect::Act;
            match cn.on_key(k) {
                Act::Cancel => self.conn = None,
                Act::Test(url, key) => {
                    let _ = self.cmd_tx.send(Cmd::ConnectTest(url, key));
                }
                Act::Save(s) => self.connect_save(s),
                Act::None => {}
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
                        if l.agent == "team" {
                            // same as /team on
                            if !self.agents.iter().any(|x| x == "team") {
                                self.agents.push("team".into());
                            }
                            let _ = self.cmd_tx.send(Cmd::Tune("team".into(), "on".into()));
                        }
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
                    if self.esc_armed() {
                        self.esc_at = None;
                        // one note per cancel: repeated presses while it winds down add nothing
                        if !self.cancel.swap(true, Ordering::Relaxed) {
                            self.items
                                .push(Item::Info("cancelling the request…".into()));
                        }
                    } else {
                        self.esc_at = Some(std::time::Instant::now());
                    }
                } else {
                    self.suggestion = None;
                    self.scroll = 0;
                }
            }
            (KeyCode::Enter, _, true) | (KeyCode::Char('j'), true, _) => self.insert("\n"),
            (KeyCode::Enter, ..) => {
                // A partial command completes to the highlighted one; a full name runs.
                let m = self.menu();
                if !m.is_empty() && !m.iter().any(|i| i.exact) {
                    self.complete_slash();
                } else {
                    self.submit();
                }
            }
            (KeyCode::Tab, ..) => {
                if !self.menu().is_empty() {
                    self.complete_slash();
                } else if let Some(s) = self
                    .suggestion
                    .take()
                    .filter(|_| self.input.is_empty() && !self.busy)
                {
                    self.input = s;
                    self.cursor = self.input.chars().count();
                } else {
                    self.cycle_agent(1);
                }
            }
            (KeyCode::BackTab, ..) => self.cycle_agent(-1),
            (KeyCode::Char('t'), true, _) => self.show_reasoning = !self.show_reasoning,
            (KeyCode::Char('o'), true, _) => self.full_results = !self.full_results,
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
            (KeyCode::Up, ..) if !self.menu().is_empty() => {
                let n = self.menu().len();
                self.slash_sel = (self.slash_sel.min(n - 1) + n - 1) % n;
            }
            (KeyCode::Down, ..) if !self.menu().is_empty() => {
                let n = self.menu().len();
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
        self.suggestion = None;
        if let Some(prov) = self.key_for.take() {
            self.input.clear();
            self.cursor = 0;
            match portcode_agent::config::store_key(&prov, &text) {
                Ok(p) => {
                    self.hint = None;
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
            // steering: the agent reads this before its next step, no interruption needed
            if let Some(q) = &self.steer {
                if let Ok(mut q) = q.lock() {
                    q.push(text.clone());
                }
                self.history.push(text.clone());
                self.hist_pos = None;
                self.input.clear();
                self.cursor = 0;
                self.items.push(Item::User(text));
                self.items.push(Item::Info(
                    "queued: the agent reads it before its next step (esc interrupts instead)"
                        .into(),
                ));
            } else {
                self.items.push(Item::Info(
                    "still working — press esc to interrupt first".into(),
                ));
            }
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
        self.turn_started = Some(std::time::Instant::now());
        self.cancel.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.send(Cmd::Send(text));
    }

    pub(super) fn run_slash(&mut self, cmd: &str) {
        self.slash(cmd);
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
            ("connect", a) => {
                self.conn = Some(match a.and_then(|n| {
                    self.cfg
                        .resolve(Some(&format!("{n}/x")))
                        .ok()
                        .map(|r| (n.to_string(), r.provider.base_url))
                }) {
                    Some((n, u)) => super::connect::Connect::for_provider(&n, &u),
                    None => super::connect::Connect::default(),
                });
            }
            ("setup", _) => self.setup_guide(),
            ("settings" | "config" | "preferences", _) => self.open_settings(),
            ("team", a) => match a {
                Some(v @ ("on" | "off")) => {
                    let on = v == "on";
                    if on && !self.agents.iter().any(|x| x == "team") {
                        self.agents.push("team".into());
                    }
                    if !on {
                        self.agents.retain(|x| x != "team");
                        if self.agent == "team" && !self.busy {
                            self.agent = "build".into();
                            let _ = self.cmd_tx.send(Cmd::SetAgent("build".into()));
                        }
                    }
                    let _ = self.cmd_tx.send(Cmd::Tune("team".into(), v.into()));
                    if on {
                        self.items.push(Item::Info("team mode on: press tab to the team agent. A project manager, tech lead and workers run in parallel; set \"team\" in portcode.json to keep it.".into()));
                    }
                }
                _ => self.items.push(Item::Info("/team on or /team off: the orchestrator agent with a PM, tech lead, researchers, developers, tester and reviewer (config \"team\")".into())),
            },
            ("context", a) => match a {
                Some("auto") => {
                    let _ = self.cmd_tx.send(Cmd::Tune("context".into(), "auto".into()));
                }
                Some(v) => match portcode_agent::config::parse_size(v) {
                    Some(n) => {
                        self.ctx_window = n;
                        let _ = self.cmd_tx.send(Cmd::Tune("context".into(), n.to_string()));
                    }
                    None => self.items.push(Item::Info("give a size like 32k, 128k, 1m or 65536 (at least 2048), or auto".into())),
                },
                None => self.items.push(Item::Info(format!(
                    "context window: {} tokens. Change it with /context 32k, /context 128k or /context auto (ask the server). Set the same size in the server too (Ollama num_ctx, LM Studio context length).",
                    self.ctx_window
                ))),
            },
            ("export", _) => {
                let dir = self.root.join(".portcode").join("export");
                let name = format!(
                    "session-{}.md",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                let path = dir.join(&name);
                match std::fs::create_dir_all(&dir)
                    .and_then(|_| std::fs::write(&path, self.export_markdown()))
                {
                    Ok(()) => self.items.push(Item::Info(format!(
                        "report saved: .portcode/export/{name}"
                    ))),
                    Err(e) => self.items.push(Item::Error(format!("could not write the report: {e}"))),
                }
            }
            ("notify", a) => {
                let on = match a {
                    Some("on") => true,
                    Some("off") => false,
                    _ => !self.notify_on,
                };
                self.notify_on = on;
                let _ = portcode_agent::config::save_pref("notify", if on { "on" } else { "off" });
                self.items.push(Item::Info(
                    if on {
                        "notifications on: a bell and a desktop notice when a task longer than 20 s ends or PortCode needs a permission"
                    } else {
                        "notifications off"
                    }
                    .into(),
                ));
            }
            ("venv", a) => match a {
                Some(v @ ("on" | "off")) => {
                    let _ = self.cmd_tx.send(Cmd::Tune("venv".into(), v.into()));
                }
                _ => self.items.push(Item::Info("/venv on or /venv off: lets the agent create and use a Python .venv (default on; config \"venv\")".into())),
            },
            ("yolo" | "allow-all", a) => {
                let on = match a {
                    Some("on") => true,
                    Some("off") => false,
                    _ => !self.auto_allow,
                };
                self.auto_allow = on;
                let _ = self
                    .cmd_tx
                    .send(Cmd::Tune("allow".into(), if on { "on" } else { "off" }.into()));
                if on {
                    if let Some(p) = self.perm.take() {
                        let _ = p.reply.send(Reply::Once);
                    }
                    for p in self.perm_queue.drain(..) {
                        let _ = p.reply.send(Reply::Once);
                    }
                    self.items.push(Item::Info("allow-all is ON: every permission request is approved without asking (deny rules still deny). Use it only on a machine and project you trust. /yolo off turns it off.".into()));
                } else {
                    self.items.push(Item::Info("allow-all is off: PortCode asks again.".into()));
                }
            }
            ("update", _) => {
                self.items.push(Item::Info("checking for updates…".into()));
                let _ = self.cmd_tx.send(Cmd::Update);
            }
            ("exit" | "quit" | "q", _) => self.quit = true,
            ("help", _) => self.items.push(Item::Note(help_text())),
            ("agent", Some(a)) => { if self.agents.iter().any(|x| x == a) { if !self.busy { self.agent = a.into(); let _ = self.cmd_tx.send(Cmd::SetAgent(a.into())); } } else { self.items.push(Item::Error(format!("unknown agent '{a}'. Try: {}", self.agents.join(", ")))); } }
            ("agents", _) | ("agent", None) => self.items.push(Item::Info(format!("agents: {} (tab cycles)", self.agents.join(", ")))),
            ("model", Some(m)) => match self.cfg.resolve(Some(m)) {
                Ok(r) => { self.model = r.model; self.provider = r.provider.name; self.ctx_window = r.profile.context_window; if self.persist && m.contains('/') { let _ = portcode_agent::config::save_last_model(m); } let _ = self.cmd_tx.send(Cmd::SetModel(m.into())); }
                Err(e) => self.items.push(Item::Error(e)),
            },
            ("model", None) => self.items.push(Item::Info(format!("{}/{} (context {} tokens)", self.provider, self.model, self.ctx_window))),
            ("models", _) => { let _ = self.cmd_tx.send(Cmd::ListModels); }
            ("clear" | "new", _) => { self.items.clear(); self.todos.clear(); self.title = None; self.tokens_in = 0; self.tokens_out = 0; self.last_prompt = 0; let _ = self.cmd_tx.send(Cmd::Clear); }
            ("compact" | "summarize", _) => {
                if self.busy {
                    self.items.push(Item::Error("wait for the running request (or press esc twice) before compacting".into()));
                } else if self.items.is_empty() {
                    self.items.push(Item::Info("nothing to compact yet".into()));
                } else {
                    self.busy = true;
                    self.items.push(Item::Info("compacting the conversation…".into()));
                    let _ = self.cmd_tx.send(Cmd::Compact);
                }
            }
            ("sessions", _) => { let _ = self.cmd_tx.send(Cmd::Sessions); }
            ("resume", a) => {
                if self.busy {
                    self.items.push(Item::Error("wait for the running request (or press esc twice) before switching sessions".into()));
                } else if let Some(w) = a {
                    let _ = self.cmd_tx.send(Cmd::Resume(w.to_string()));
                } else {
                    let _ = self.cmd_tx.send(Cmd::Sessions);
                }
            }
            ("deep", _) => { self.deep = true; let _ = self.cmd_tx.send(Cmd::SetDeep(true)); self.items.push(Item::Info("deep mode: disasm, emulate, decompile and hexdump are available. Use it only on software you own or may examine.".into())); }
            ("clean" | "clean-room", _) => { self.deep = false; let _ = self.cmd_tx.send(Cmd::SetDeep(false)); self.items.push(Item::Info("clean-room mode: the original is observed from outside only.".into())); }
            ("engines", _) => {
                let list: Vec<String> = portcode_agent::engines::ENGINES.iter().map(|e| format!("{} {}", if portcode_agent::engines::detect(e).is_some() { "✓" } else { "·" }, e.id)).collect();
                self.items.push(Item::Info(format!("engines: {} — install: portcode engines install <id>", list.join("  "))));
            }
            ("create", _) => {
                let mut w = super::wizard::Wizard::default();
                w.detect_here(&self.root, &self.cfg.allowed_dirs);
                self.wiz = Some(w);
            }
            ("workbench" | "wb", a) => self.open_workbench(a),
            ("remote", a) => self.remote_req = Some(a.unwrap_or("").to_string()),
            ("style" | "effort", a) => match a {
                Some(v) => { let _ = self.cmd_tx.send(Cmd::Tune(name.into(), v.into())); }
                None => self.items.push(Item::Info(if name == "style" {
                    "prompt style: /style auto|compact|full|qwen38 (auto picks qwen38 for Qwen3.8 models). Default for all models: \"prompt_style\" in portcode.json".into()
                } else {
                    "/effort xhigh|medium|low|none: thinking depth for Qwen3.8 (none = thinking off)".into()
                })),
            },
            ("theme", a) => {
                let light = match a {
                    Some("light") => true,
                    Some("dark") => false,
                    None => !super::theme::is_light(),
                    Some(o) => {
                        self.items.push(Item::Error(format!("unknown theme '{o}'. Use /theme dark or /theme light")));
                        return;
                    }
                };
                super::theme::set_light(light);
                if self.persist {
                    let _ = portcode_agent::config::save_pref("theme", if light { "light" } else { "dark" });
                }
                self.items.push(Item::Info(format!("theme: {}", if light { "light" } else { "dark" })));
            }
            ("thinking", _) => self.show_reasoning = !self.show_reasoning,
            ("sidebar", _) => self.sidebar = !self.sidebar,
            ("tools", _) => self.items.push(Item::Note(tools_text())),
            ("ledger", _) => {
                self.refresh_ledger();
                let (o, i, u) = self.ledger;
                self.items.push(Item::Info(format!("evidence: {o} observation · {i} inference · {u} unknown{}", self.unknowns.first().map(|x| format!(" — open: {x}")).unwrap_or_default())));
            }
            (other, _) => self.items.push(Item::Error(format!("unknown command /{other}. Press / to see commands"))),
        }
    }
}

fn help_text() -> String {
    let mut s = String::from("# Keys\n");
    for (k, d) in [
        ("enter", "send"),
        ("alt+enter", "new line"),
        ("tab", "switch agent, or complete a / command"),
        ("up / down", "prompt history, or move in the / menu"),
        ("esc esc", "cancel the running request"),
        ("ctrl+c", "clear the prompt, then quit"),
        ("ctrl+t", "show or hide the model's reasoning"),
        ("ctrl+o", "show or hide the output of older tool calls"),
        ("ctrl+b", "show or hide the sidebar"),
        ("pgup / pgdn", "scroll the transcript"),
    ] {
        s += &format!("{k}\t{d}\n");
    }
    s += "# Commands\n";
    for (n, d) in SLASH {
        s += &format!("/{n}\t{d}\n");
    }
    s.trim_end().to_string()
}

fn tools_text() -> String {
    let mut s = String::from("# Tools\n");
    for (k, d) in [
        ("files", "read  ls  glob  grep  write  edit"),
        ("run", "bash  venv  todowrite  webfetch  skill  task  team"),
        ("proof", "survey  pin  evidence_add  evidence_list  parity  screenshot  scaffold  site_build"),
        ("deep", "re_status  re_install  disasm  emulate  decompile  functions  hexdump  unity  dotnet  unity_assets"),
        ("more", "tools from MCP servers in portcode.json"),
    ] {
        s += &format!("{k}\t{d}\n");
    }
    s += "deep tools need /deep. Each agent sees only the tools it needs.";
    s
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
    fn resumed_session_fills_prompt_history() {
        use portcode_llm::Message;
        let mut a = app();
        a.load_history(
            "s-1",
            &[
                Message::user("first"),
                Message::user("Not finished.\nOpen items"),
                Message::user("second"),
            ],
        );
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input, "second");
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input, "first");
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

    #[test]
    fn argument_menu_lists_providers_and_completes() {
        let mut a = app();
        a.input = "/provider oll".into();
        let m = a.menu();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].label, "ollama");
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.input, "/provider ollama ");
        a.input = "/key ".into();
        assert!(a.menu().len() >= 5, "all providers are offered");
        a.input = "/provider ollama qwen".into();
        assert!(a.menu().is_empty(), "no menu once the model is being typed");
    }

    #[test]
    fn model_menu_uses_the_models_the_provider_reported() {
        let mut a = app();
        a.on_msg(Msg::Models(vec!["qwen3:14b".into(), "llama3".into()]));
        a.input = "/model q".into();
        let m = a.menu();
        assert_eq!(m.len(), 1);
        assert!(m[0].fill.ends_with("/qwen3:14b"));
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
    fn typing_while_busy_queues_a_steering_message() {
        let mut a = app();
        let q: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
        a.steer = Some(q.clone());
        a.busy = true;
        a.input = "use the other file".into();
        a.submit();
        assert_eq!(q.lock().unwrap().as_slice(), ["use the other file"]);
        assert!(a.input.is_empty());
        // the turn ends before the agent read it: it goes out as the next turn
        a.on_msg(Msg::Idle);
        assert!(a.busy && q.lock().unwrap().is_empty());
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
    fn export_has_checklist_and_honest_proof() {
        let mut a = app();
        a.items.push(Item::User("port it".into()));
        a.todos = vec![portcode_agent::tool::Todo {
            content: "parse".into(),
            status: "completed".into(),
        }];
        let m = a.export_markdown();
        assert!(m.contains("Checklist (1 of 1 done)") && m.contains("**You:** port it"));
        assert!(m.contains("No parity run yet"));
    }
    #[test]
    fn capital_a_allows_everything_from_now_on() {
        let mut a = app();
        let ask = |a: &mut App| {
            let (tx, rx) = oneshot::channel();
            a.on_msg(Msg::Perm(PermAsk {
                req: Request {
                    tool: "bash".into(),
                    target: "ls".into(),
                    reason: String::new(),
                },
                reply: tx,
            }));
            rx
        };
        let (mut first, mut second) = (ask(&mut a), ask(&mut a));
        a.on_key(key(KeyCode::Char('A')));
        assert_eq!(first.try_recv().ok(), Some(Reply::Once));
        assert_eq!(second.try_recv().ok(), Some(Reply::Once));
        assert!(a.auto_allow && a.perm.is_none());
        let mut third = ask(&mut a);
        assert_eq!(third.try_recv().ok(), Some(Reply::Once));
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
