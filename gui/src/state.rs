//! What the window shows, without any drawing: the transcript, the sidebar numbers, the permission question.
//! Kept free of GPUI so it can be tested like the terminal app's state.
use portcode_agent::tool::Todo;
use portcode_agent::{Outcome, Reply, Request, Status, UiEvent};
use tokio::sync::oneshot;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolState {
    Running,
    Ok,
    Err,
}

#[derive(Debug)]
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
        open: bool,
    },
    Info(String),
    Error(String),
    Footer(String),
}

pub struct Perm {
    pub req: Request,
    pub reply: oneshot::Sender<Reply>,
}

#[derive(Default)]
pub struct State {
    pub items: Vec<Item>,
    pub busy: bool,
    pub agent: String,
    pub agents: Vec<String>,
    pub provider: String,
    pub model: String,
    pub deep: bool,
    pub auto_allow: bool,
    pub ctx_window: u32,
    pub last_prompt: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub todos: Vec<Todo>,
    /// (observed, inferred, unknown)
    pub ledger: (usize, usize, usize),
    pub unknowns: Vec<String>,
    pub parity: Option<(bool, String)>,
    pub title: Option<String>,
    pub perm: Option<Perm>,
    pub perm_queue: std::collections::VecDeque<Perm>,
    pub show_reasoning: bool,
    pub ready: bool,
}

impl State {
    pub fn on_ui(&mut self, e: UiEvent) {
        match e {
            UiEvent::Text(t) => match self.items.last_mut() {
                Some(Item::Assistant(s)) => s.push_str(&t),
                _ => self.items.push(Item::Assistant(t)),
            },
            UiEvent::Reasoning(t) => match self.items.last_mut() {
                Some(Item::Reasoning(s)) => s.push_str(&t),
                _ => self.items.push(Item::Reasoning(t)),
            },
            UiEvent::ToolStart(name) => self.items.push(Item::Tool {
                name,
                args: String::new(),
                state: ToolState::Running,
                title: String::new(),
                text: String::new(),
                open: false,
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
                        open: false,
                    }),
                }
            }
            UiEvent::ToolResult {
                name,
                title,
                ok,
                text,
            } => {
                let open_it = !ok || matches!(name.as_str(), "edit" | "write" | "parity");
                if let Some(Item::Tool {
                    state,
                    title: t,
                    text: x,
                    open,
                    ..
                }) = self.items.iter_mut().rev().find(|i| {
                    matches!(i, Item::Tool { name: n, state: ToolState::Running, .. } if *n == name)
                }) {
                    *state = if ok { ToolState::Ok } else { ToolState::Err };
                    *t = title.clone();
                    *x = text;
                    *open = open_it;
                }
                if name == "parity" {
                    self.parity = Some((ok && title.contains("PASS"), title));
                }
            }
            UiEvent::Todos(t) => self.todos = t,
            UiEvent::Compacted => self.items.push(Item::Info(
                "older context summarised to fit the window".into(),
            )),
            UiEvent::Info(s) => self.items.push(Item::Info(s)),
            UiEvent::Usage(u) => {
                self.tokens_in += u.prompt_tokens;
                self.tokens_out += u.completion_tokens;
                self.last_prompt = u.prompt_tokens;
            }
        }
    }

    pub fn on_done(&mut self, r: Result<Outcome, String>) {
        self.busy = false;
        self.settle_tools();
        match r {
            Ok(o) => self.items.push(Item::Footer(format!(
                "{} · {} step{} · {} in / {} out{}",
                self.agent,
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

    /// A tool still shown as running when the turn is over was cancelled: it must not keep spinning.
    pub fn settle_tools(&mut self) {
        for i in &mut self.items {
            if let Item::Tool { state, .. } = i {
                if *state == ToolState::Running {
                    *state = ToolState::Err;
                }
            }
        }
    }

    pub fn ask(&mut self, p: Perm) {
        if self.auto_allow {
            let _ = p.reply.send(Reply::Once);
        } else if self.perm.is_some() {
            self.perm_queue.push_back(p);
        } else {
            self.perm = Some(p);
        }
    }

    pub fn answer(&mut self, r: Reply) {
        if let Some(p) = self.perm.take() {
            let _ = p.reply.send(r);
        }
        self.perm = self.perm_queue.pop_front();
    }

    /// Approve this one and everything after it.
    pub fn allow_all(&mut self) {
        self.auto_allow = true;
        self.answer(Reply::Once);
        while let Some(p) = self.perm.take() {
            let _ = p.reply.send(Reply::Once);
            self.perm = self.perm_queue.pop_front();
        }
    }

    pub fn context_pct(&self) -> u64 {
        if self.ctx_window == 0 {
            0
        } else {
            (self.last_prompt * 100 / u64::from(self.ctx_window)).min(999)
        }
    }

    /// (done, all, percent) of the agent's checklist.
    pub fn todo_progress(&self) -> (usize, usize, usize) {
        let all = self.todos.len();
        let done = self
            .todos
            .iter()
            .filter(|t| t.status == "completed")
            .count();
        (done, all, done * 100 / all.max(1))
    }

    /// (matching, all) cases of the last parity run, from its summary line.
    pub fn parity_counts(&self) -> Option<(u32, u32)> {
        let (_, t) = self.parity.as_ref()?;
        let line = t.lines().rev().find(|l| l.contains("parity: ok="))?;
        let n = |k: &str| -> Option<u32> {
            line.split(k)
                .nth(1)?
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        };
        let (ok, d, m, e) = (n("ok=")?, n("diff=")?, n("missing=")?, n("extra=")?);
        let all = ok + d + m + e;
        (all > 0).then_some((ok, all))
    }

    pub fn refresh_ledger(&mut self, root: &std::path::Path) {
        let l = portcode_evidence::Ledger::open(root.join(".portcode/evidence"));
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
            .map(|u| u.into_iter().take(3).map(|e| e.statement.clone()).collect())
            .unwrap_or_default();
    }
}

/// One line about a tool call: the part of its arguments a person wants to see.
pub fn summary(name: &str, args: &str) -> String {
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
        "evidence_add" => &["statement"],
        "disasm" | "emulate" | "hexdump" | "decompile" | "dotnet" | "unity_assets" | "unity" => {
            &["file"]
        }
        _ => &["path", "pattern", "command", "name", "subject"],
    };
    keys.iter().find_map(|k| pick(k)).unwrap_or_else(|| {
        if args.chars().count() > 60 {
            format!("{}…", args.chars().take(60).collect::<String>())
        } else {
            args.to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_result(name: &str, ok: bool) -> UiEvent {
        UiEvent::ToolResult {
            name: name.into(),
            title: String::new(),
            ok,
            text: "line 1\nline 2".into(),
        }
    }

    #[test]
    fn streamed_text_extends_the_last_reply() {
        let mut s = State::default();
        s.on_ui(UiEvent::Text("Hel".into()));
        s.on_ui(UiEvent::Text("lo".into()));
        assert!(matches!(s.items.as_slice(), [Item::Assistant(t)] if t == "Hello"));
    }

    #[test]
    fn a_tool_goes_from_running_to_done_and_failures_open_themselves() {
        let mut s = State::default();
        s.on_ui(UiEvent::ToolStart("read".into()));
        s.on_ui(UiEvent::ToolCall {
            name: "read".into(),
            args: "{\"path\":\"a.rs\"}".into(),
        });
        s.on_ui(tool_result("read", true));
        assert!(
            matches!(&s.items[0], Item::Tool { state: ToolState::Ok, args, open: false, .. } if args.contains("a.rs"))
        );
        s.on_ui(UiEvent::ToolStart("bash".into()));
        s.on_ui(tool_result("bash", false));
        assert!(matches!(
            &s.items[1],
            Item::Tool {
                state: ToolState::Err,
                open: true,
                ..
            }
        ));
    }

    #[test]
    fn a_cancelled_tool_stops_spinning() {
        let mut s = State::default();
        s.on_ui(UiEvent::ToolStart("bash".into()));
        s.busy = true;
        s.on_done(Err("cancelled".into()));
        assert!(!s.busy);
        assert!(matches!(
            &s.items[0],
            Item::Tool {
                state: ToolState::Err,
                ..
            }
        ));
    }

    #[test]
    fn allow_all_answers_this_and_every_queued_question() {
        let mut s = State::default();
        let mut rxs = vec![];
        for _ in 0..3 {
            let (tx, rx) = oneshot::channel();
            rxs.push(rx);
            s.ask(Perm {
                req: Request {
                    tool: "bash".into(),
                    target: "ls".into(),
                    reason: String::new(),
                },
                reply: tx,
            });
        }
        s.allow_all();
        for mut rx in rxs {
            assert_eq!(rx.try_recv().ok(), Some(Reply::Once));
        }
        assert!(s.perm.is_none() && s.auto_allow);
    }

    #[test]
    fn progress_and_parity_numbers() {
        let mut s = State {
            todos: vec![
                Todo {
                    content: "a".into(),
                    status: "completed".into(),
                },
                Todo {
                    content: "b".into(),
                    status: "pending".into(),
                },
            ],
            ..State::default()
        };
        assert_eq!(s.todo_progress(), (1, 2, 50));
        s.parity = Some((
            false,
            "DIFF x\nparity: ok=3 diff=1 missing=0 extra=0\n".into(),
        ));
        assert_eq!(s.parity_counts(), Some((3, 4)));
    }

    #[test]
    fn summaries_survive_truncated_json() {
        assert_eq!(
            summary("read", "{\"path\":\"src/main.rs\",\"offs"),
            "src/main.rs"
        );
        assert_eq!(
            summary(
                "bash",
                "{\"command\":\"cargo test\",\"description\":\"run tests\"}"
            ),
            "run tests"
        );
    }
}
