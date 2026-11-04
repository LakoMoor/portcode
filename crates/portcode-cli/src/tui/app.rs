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
    ("clear", "new session"),
    ("resume", "resume the latest session"),
    ("tools", "list tools"),
    ("ledger", "show the evidence ledger"),
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
    pub auto_allow: bool,
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
            auto_allow: false,
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
        self.unknowns = l
            .open_unknowns()
            .map(|u| {
                u.into_iter()
                    .map(|e| format!("{} {}", e.id, e.statement))
                    .collect()
            })
            .unwrap_or_default();
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
        match m {
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
                        *state = if ok { ToolState::Ok } else { ToolState::Err }; *title = t; *text = x;
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
            }
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
        self.cursor += s.chars().count();
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
                }
                None => self.perm = Some(p),
            }
            return;
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
            (KeyCode::Esc, ..) => {
                if self.busy {
                    self.cancel.store(true, Ordering::Relaxed);
                } else {
                    self.scroll = 0;
                }
            }
            (KeyCode::Enter, _, true) | (KeyCode::Char('j'), true, _) => self.insert("\n"),
            (KeyCode::Enter, ..) => self.submit(),
            (KeyCode::Tab, ..) => {
                if let Some((n, _)) = self.slash_matches().first().copied() {
                    self.input = format!("/{n} ");
                    self.cursor = self.len_chars();
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
        match (name, arg) {
            ("exit" | "quit" | "q", _) => self.quit = true,
            ("help", _) => self.items.push(Item::Info("tab: switch agent · enter: send · alt+enter: newline · esc: interrupt · ctrl+t: thinking · ctrl+b: sidebar · pgup/pgdn: scroll · /: commands".into())),
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
            ("thinking", _) => self.show_reasoning = !self.show_reasoning,
            ("sidebar", _) => self.sidebar = !self.sidebar,
            ("tools", _) => self.items.push(Item::Info("tools: read ls glob grep write edit bash todowrite webfetch skill task survey pin evidence_add evidence_list parity screenshot site_build scaffold (+ MCP)".into())),
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
}
