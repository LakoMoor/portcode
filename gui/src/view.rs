//! The window: transcript, composer, ledger sidebar, permission question.
use crate::backend::{self, Cmd, Handle, Msg};
use crate::markdown::{self, Block, Mark};
use crate::state::{summary, Item, Perm, State, ToolState};
use crate::theme::{self, agent_color, tool_color, Theme, MONO_FONT, UI_FONT};
use futures::StreamExt;
use gpui::{
    div, prelude::*, px, relative, ClickEvent, Context, FocusHandle, FontWeight, HighlightStyle,
    IntoElement, KeyDownEvent, Render, Rgba, ScrollHandle, SharedString, StyledText, Window,
    WindowControlArea,
};
use portcode_agent::Reply;
use std::path::PathBuf;

const SUGGESTIONS: &[&str] = &[
    "Survey this folder and tell me what it is",
    "Find the entry point and explain how the program starts",
    "Port the program in original/ to Rust, one behaviour at a time",
];

pub struct Root {
    st: State,
    root: PathBuf,
    handle: Option<Handle>,
    input: String,
    focus: FocusHandle,
    scroll: ScrollHandle,
    follow: bool,
    sidebar: bool,
    dark: bool,
    tick: usize,
}

impl Root {
    pub fn new(root: PathBuf, deep: bool, cx: &mut Context<Self>) -> Self {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Msg>();
        let demo = std::env::var_os("PORTCODE_GUI_DEMO").is_some();
        if !demo {
            backend::spawn(root.clone(), deep, tx);
        }
        cx.spawn(async move |this, cx| {
            while let Some(m) = rx.next().await {
                if this.update(cx, |t, cx| t.on_msg(m, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // a slow heartbeat for the "working" dots; it only redraws while the agent is busy
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(450))
                .await;
            if this
                .update(cx, |t, cx| {
                    if t.st.busy {
                        t.tick += 1;
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
        let st = State {
            deep,
            ..State::default()
        };
        let mut this = Self {
            st,
            root,
            handle: None,
            input: String::new(),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            follow: true,
            sidebar: true,
            dark: portcode_agent::config::pref("gui_theme").as_deref() != Some("light"),
            tick: 0,
        };
        if demo {
            this.load_demo();
        }
        this
    }

    /// A made-up session for screenshots (`PORTCODE_GUI_DEMO=1`): no model, no project files.
    fn load_demo(&mut self) {
        use portcode_agent::tool::Todo;
        let td = |c: &str, s: &str| Todo {
            content: c.into(),
            status: s.into(),
        };
        let tool =
            |name: &str, args: &str, title: &str, text: &str, ok: bool, open: bool| Item::Tool {
                name: name.into(),
                args: args.into(),
                state: if ok { ToolState::Ok } else { ToolState::Err },
                title: title.into(),
                text: text.into(),
                open,
            };
        let st = &mut self.st;
        st.ready = true;
        st.agent = "port".into();
        st.agents = vec![
            "build".into(),
            "port".into(),
            "reverse".into(),
            "plan".into(),
        ];
        st.provider = "ollama".into();
        st.model = "qwen3-coder:30b".into();
        st.deep = true;
        st.ctx_window = 32768;
        st.last_prompt = 21400;
        st.ledger = (7, 4, 2);
        st.unknowns = vec![
            "How --top breaks ties between equal counts".into(),
            "Whether the original reads stdin when no file is given".into(),
        ];
        st.parity = Some((
            false,
            "DIFF top-ties\nparity: ok=6 diff=1 missing=0 extra=0\n".into(),
        ));
        st.todos = vec![
            td("Parse options and exit codes", "completed"),
            td("Count words, case-insensitive", "completed"),
            td("Reproduce --top ordering", "in_progress"),
            td("Match error text for a missing file", "pending"),
        ];
        st.title = Some("Port wordstat to Rust".into());
        st.items = vec![
            Item::User("Port the wordstat program in original/ to Rust and keep its output identical.".into()),
            Item::Assistant("I'll start from what the original does, not from how it is written. First a survey, then goldens from the real binary.".into()),
            tool("survey", r#"{"path":"original"}"#, "survey original", "files: 3\nlanguages: C 212 lines\nentry: main.c", true, false),
            tool("decompile", r#"{"file":"original/wordstat","mode":"func","query":"main"}"#, "decompile func", "int main(int argc, char **argv)\n{\n  if (argc < 2) return usage();\n  return run(argv[1]);\n}", true, true),
            tool("bash", r#"{"command":"./original/wordstat --top 3 sample.txt"}"#, "run the original with --top 3", "the 4\nof 3\nand 3", true, false),
            Item::Assistant("The original sorts by count, then **alphabetically** on ties. That is an `inference` until `parity` agrees, so I recorded it as one and wrote the port against it:\n\n- counts are case-insensitive\n- ties break on the word itself\n\n```rust\nlet mut v: Vec<_> = counts.into_iter().collect();\nv.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));\n```".into()),
            tool("parity", "{}", "parity: FAIL", "DIFF top-ties\n- the 4\n+ the 3\nparity: ok=6 diff=1 missing=0 extra=0", false, true),
            Item::Info("continuing: 2 todo item(s) still open".into()),
            Item::Footer("port · 11 steps · 18.2k in / 2.4k out".into()),
        ];
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn t(&self) -> Theme {
        if self.dark {
            theme::dark()
        } else {
            theme::light()
        }
    }

    // ---- messages from the agent ----

    fn on_msg(&mut self, m: Msg, cx: &mut Context<Self>) {
        match m {
            Msg::Ready(r) => {
                self.st.agent = r.agent;
                self.st.agents = r.agents;
                self.st.provider = r.provider;
                self.st.model = r.model;
                self.st.ctx_window = r.window;
                self.st.deep = r.deep;
                self.st.ready = true;
                self.handle = Some(r.handle);
                self.st.refresh_ledger(&self.root);
            }
            Msg::Failed(e) => {
                self.st.items.push(Item::Error(e));
                self.st.busy = false;
            }
            Msg::Ui(e) => {
                let refresh = matches!(&e, portcode_agent::UiEvent::ToolResult { name, .. }
                    if matches!(name.as_str(), "evidence_add" | "pin" | "parity"));
                self.st.on_ui(e);
                if refresh {
                    self.st.refresh_ledger(&self.root);
                }
            }
            Msg::Perm(p) => self.st.ask(p),
            Msg::Done(r) => {
                let cancelled =
                    matches!(&r, Ok(o) if o.status == portcode_agent::Status::Cancelled);
                self.st.on_done(r);
                self.st.refresh_ledger(&self.root);
                self.after_turn(cancelled);
            }
            Msg::Info(s) => self.st.items.push(Item::Info(s)),
            Msg::Window(n) => self.st.ctx_window = n,
            Msg::Idle => {
                self.st.busy = false;
                self.st.settle_tools();
                self.after_turn(false);
            }
        }
        if self.follow {
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Messages typed while the agent worked that it never reached: send them now, or hand them back after an interrupt.
    fn after_turn(&mut self, cancelled: bool) {
        let Some(h) = self.handle.clone() else { return };
        let left = h.take_steer();
        if left.is_empty() {
            return;
        }
        let text = left.join("\n\n");
        if cancelled {
            if self.input.is_empty() {
                self.input = text;
                self.st.items.push(Item::Info(
                    "interrupted: your queued message is back in the box".into(),
                ));
            }
        } else {
            self.st.busy = true;
            h.rearm();
            h.send(Cmd::Send(text));
        }
    }

    // ---- input ----

    fn submit(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(cmd) = text.strip_prefix('/') {
            self.input.clear();
            self.slash(cmd.trim());
            return;
        }
        let Some(h) = self.handle.clone() else {
            self.st
                .items
                .push(Item::Info("the agent is still starting".into()));
            return;
        };
        self.input.clear();
        self.follow = true;
        self.st.items.push(Item::User(text.clone()));
        if self.st.title.is_none() {
            let first = text.lines().next().unwrap_or("");
            self.st.title = Some(if first.chars().count() > 48 {
                format!("{}…", first.chars().take(47).collect::<String>().trim_end())
            } else {
                first.to_string()
            });
        }
        if self.st.busy {
            h.steer(text);
            self.st.items.push(Item::Info(
                "queued: the agent reads it before its next step (esc interrupts instead)".into(),
            ));
        } else {
            self.st.busy = true;
            h.rearm();
            h.send(Cmd::Send(text));
        }
    }

    fn slash(&mut self, line: &str) {
        let (name, arg) = match line.split_once(' ') {
            Some((n, a)) => (n, Some(a.trim())),
            None => (line, None),
        };
        let Some(h) = self.handle.clone() else { return };
        match (name, arg) {
            ("deep", _) => self.set_deep(true),
            ("clean", _) => self.set_deep(false),
            ("yolo" | "allow-all", _) => self.toggle_allow_all(),
            ("clear" | "new", _) => {
                self.st.items.clear();
                self.st.todos.clear();
                self.st.title = None;
                self.st.last_prompt = 0;
                h.send(Cmd::Clear);
            }
            ("compact", _) => {
                if self.st.busy {
                    self.st
                        .items
                        .push(Item::Error("wait for the running request first".into()));
                } else {
                    self.st.busy = true;
                    self.st.items.push(Item::Info("compacting the conversation…".into()));
                    h.send(Cmd::Compact);
                }
            }
            ("agent", Some(a)) => {
                self.st.agent = a.to_string();
                h.send(Cmd::SetAgent(a.into()));
            }
            ("model", Some(m)) => {
                h.send(Cmd::SetModel(m.into()));
                if let Some((p, mm)) = m.split_once('/') {
                    self.st.provider = p.into();
                    self.st.model = mm.into();
                } else {
                    self.st.model = m.into();
                }
            }
            ("context", Some(v)) => h.send(Cmd::Tune("context".into(), v.into())),
            ("sidebar", _) => self.sidebar = !self.sidebar,
            ("theme", _) => self.toggle_theme(),
            _ => self.st.items.push(Item::Info(
                "commands: /deep /clean /yolo /clear /compact /agent <name> /model <provider/model> /context <32k> /theme /sidebar".into(),
            )),
        }
    }

    fn set_deep(&mut self, on: bool) {
        self.st.deep = on;
        if let Some(h) = &self.handle {
            h.send(Cmd::SetDeep(on));
        }
    }

    fn toggle_allow_all(&mut self) {
        if self.st.auto_allow {
            self.st.auto_allow = false;
            self.st
                .items
                .push(Item::Info("allow-all is off: PortCode asks again".into()));
        } else {
            self.st.allow_all();
            self.st.items.push(Item::Info(
                "allow-all is ON: every request is approved without asking. Use it only on a machine and project you trust.".into(),
            ));
        }
    }

    fn toggle_theme(&mut self) {
        self.dark = !self.dark;
        let _ = portcode_agent::config::save_pref(
            "gui_theme",
            if self.dark { "dark" } else { "light" },
        );
    }

    fn cycle_agent(&mut self) {
        let Some(h) = self.handle.clone() else { return };
        if self.st.busy || self.st.agents.is_empty() {
            return;
        }
        let i = self
            .st
            .agents
            .iter()
            .position(|a| *a == self.st.agent)
            .map_or(0, |i| i + 1);
        let next = self.st.agents[i % self.st.agents.len()].clone();
        self.st.agent = next.clone();
        h.send(Cmd::SetAgent(next));
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let secondary = k.modifiers.secondary(cx);
        let ch = k.key_char.as_deref().unwrap_or("");
        if self.st.perm.is_some() {
            match (k.key.as_str(), ch) {
                (_, "A") => self.st.allow_all(),
                ("y" | "enter", _) => self.st.answer(Reply::Once),
                ("a", _) => self.st.answer(Reply::Always),
                ("n" | "escape", _) => self.st.answer(Reply::Reject),
                _ => {}
            }
            cx.notify();
            return;
        }
        match k.key.as_str() {
            "escape" => {
                if self.st.busy {
                    if let Some(h) = &self.handle {
                        h.interrupt();
                    }
                    self.st.items.push(Item::Info("interrupting…".into()));
                }
            }
            "enter" if !k.modifiers.shift => self.submit(),
            "enter" => self.input.push('\n'),
            "backspace" if secondary => self.input.clear(),
            "backspace" => {
                self.input.pop();
            }
            "v" if secondary => {
                if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.input.push_str(&t.replace('\r', ""));
                }
            }
            "\\" if secondary => self.sidebar = !self.sidebar,
            "c" if k.modifiers.control && !k.modifiers.platform => {
                if self.st.busy {
                    if let Some(h) = &self.handle {
                        h.interrupt();
                    }
                }
            }
            _ => {
                if !secondary && !k.modifiers.control && !ch.is_empty() {
                    self.input.push_str(ch);
                }
            }
        }
        cx.notify();
    }

    // ---- pieces ----

    fn rich(&self, text: &str, color: Rgba) -> impl IntoElement {
        let t = self.t();
        let (clean, marks) = markdown::inline(text);
        let hl: Vec<_> = marks
            .into_iter()
            .map(|(r, m)| {
                let s = match m {
                    Mark::Code => HighlightStyle {
                        color: Some(t.accent.into()),
                        background_color: Some(t.raised.into()),
                        ..Default::default()
                    },
                    Mark::Bold => HighlightStyle {
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                };
                (r, s)
            })
            .collect();
        div()
            .text_color(color)
            .child(StyledText::new(clean).with_highlights(hl))
    }

    fn reply(&self, text: &str) -> gpui::AnyElement {
        let t = self.t();
        let mut col = div().flex().flex_col().gap_3().line_height(relative(1.62));
        for b in markdown::blocks(text) {
            col = col.child(match b {
                Block::Heading(n, h) => div()
                    .mt_2()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(if n == 1 {
                        20.
                    } else if n == 2 {
                        17.
                    } else {
                        15.
                    }))
                    .child(self.rich(&h, t.text))
                    .into_any_element(),
                Block::Para(p) => self.rich(&p, t.text).into_any_element(),
                Block::Bullets(items) => div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .children(items.into_iter().map(|i| {
                        div()
                            .flex()
                            .gap_2()
                            .child(div().text_color(t.accent).child("•"))
                            .child(div().flex_1().child(self.rich(&i, t.text)))
                    }))
                    .into_any_element(),
                Block::Code { lang, text } => div()
                    .rounded_lg()
                    .border_1()
                    .border_color(t.rule)
                    .bg(t.raised)
                    .overflow_hidden()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_size(px(11.))
                            .text_color(t.faint)
                            .border_b_1()
                            .border_color(t.rule)
                            .child(if lang.is_empty() {
                                "code".to_string()
                            } else {
                                lang
                            }),
                    )
                    .child(
                        div()
                            .p_3()
                            .font_family(MONO_FONT)
                            .text_size(px(12.5))
                            .line_height(relative(1.5))
                            .text_color(t.text)
                            .flex()
                            .flex_col()
                            .children(text.lines().map(|l| {
                                div().child(if l.is_empty() {
                                    " ".to_string()
                                } else {
                                    l.to_string()
                                })
                            })),
                    )
                    .into_any_element(),
            });
        }
        col.into_any_element()
    }

    fn tool_row(&self, ix: usize, it: &Item, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        let Item::Tool {
            name,
            args,
            state,
            title,
            text,
            open,
        } = it
        else {
            return div().into_any_element();
        };
        let (glyph, gcol) = match state {
            ToolState::Running => (["◐", "◓", "◑", "◒"][self.tick % 4], t.accent),
            ToolState::Ok => ("✓", t.seen),
            ToolState::Err => ("✗", t.unknown),
        };
        let sum = if *state == ToolState::Running || title.is_empty() || title == name {
            summary(name, args)
        } else {
            title.clone()
        };
        let lines: Vec<&str> = text.lines().collect();
        let can_open = !lines.is_empty();
        let mut card = div()
            .rounded_lg()
            .border_1()
            .border_color(t.rule)
            .bg(t.panel)
            .overflow_hidden()
            .child(
                div()
                    .id(("tool", ix))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(34.))
                    .when(can_open, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(t.raised))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Some(Item::Tool { open, .. }) = this.st.items.get_mut(ix) {
                                    *open = !*open;
                                }
                                cx.notify();
                            }))
                    })
                    .child(div().w(px(14.)).text_color(gcol).child(glyph))
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tool_color(&t, name))
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.5))
                            .text_color(t.muted)
                            .child(sum),
                    )
                    .when(can_open, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.faint)
                                .child(if *open {
                                    "▾".to_string()
                                } else {
                                    format!("{} ▸", lines.len())
                                }),
                        )
                    }),
            );
        if *open && can_open {
            let shown = 18usize;
            card = card.child(
                div()
                    .border_t_1()
                    .border_color(t.rule)
                    .px_3()
                    .py_2()
                    .font_family(MONO_FONT)
                    .text_size(px(12.))
                    .line_height(relative(1.5))
                    .flex()
                    .flex_col()
                    .children(lines.iter().take(shown).map(|l| {
                        let c = if *state == ToolState::Err {
                            t.unknown
                        } else if l.starts_with('+') && !l.starts_with("++") {
                            t.seen
                        } else if l.starts_with('-') && !l.starts_with("--") {
                            t.unknown
                        } else {
                            t.muted
                        };
                        let l: String = l.chars().take(160).collect();
                        div()
                            .text_color(c)
                            .child(if l.is_empty() { " ".to_string() } else { l })
                    }))
                    .when(lines.len() > shown, |d| {
                        d.child(
                            div()
                                .text_color(t.faint)
                                .child(format!("… {} more lines", lines.len() - shown)),
                        )
                    }),
            );
        }
        card.into_any_element()
    }

    fn transcript(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        let mut col = div()
            .w_full()
            .max_w(px(780.))
            .mx_auto()
            .px_8()
            .pt_6()
            .pb_10()
            .flex()
            .flex_col()
            .gap_4();
        if self.st.items.is_empty() {
            col = col.child(self.empty_state(cx));
        }
        for (ix, it) in self.st.items.iter().enumerate() {
            let el = match it {
                Item::User(s) => div()
                    .flex()
                    .gap_3()
                    .rounded_lg()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.rule)
                    .p_3()
                    .line_height(relative(1.55))
                    .child(div().w(px(3.)).rounded_full().bg(t.blue).flex_shrink_0())
                    .child(div().flex_1().min_w_0().child(s.clone()))
                    .into_any_element(),
                Item::Assistant(s) => self.reply(s),
                Item::Reasoning(s) => {
                    if self.st.show_reasoning {
                        div()
                            .pl_3()
                            .border_l_2()
                            .border_color(t.rule)
                            .text_size(px(13.))
                            .italic()
                            .text_color(t.faint)
                            .child(s.clone())
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    }
                }
                Item::Tool { .. } => self.tool_row(ix, it, cx),
                Item::Info(s) => div()
                    .text_size(px(12.5))
                    .text_color(t.faint)
                    .child(format!("· {s}"))
                    .into_any_element(),
                Item::Error(s) => div()
                    .rounded_lg()
                    .border_1()
                    .border_color(t.unknown)
                    .p_3()
                    .text_color(t.unknown)
                    .child(s.clone())
                    .into_any_element(),
                Item::Footer(s) => div()
                    .pt_2()
                    .border_t_1()
                    .border_color(t.rule)
                    .font_family(MONO_FONT)
                    .text_size(px(11.5))
                    .text_color(t.faint)
                    .child(s.clone())
                    .into_any_element(),
            };
            col = col.child(el);
        }
        if self.st.busy {
            col = col.child(
                div()
                    .text_color(t.accent)
                    .text_size(px(13.))
                    .child(format!("Working{}", ".".repeat(self.tick % 4))),
            );
        }
        div()
            .id("transcript")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|this, _, _, _| this.follow = false))
            .child(col)
            .into_any_element()
    }

    fn empty_state(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        let label = self.root.display().to_string();
        let home = std::env::var("HOME").unwrap_or_default();
        let label = match label.strip_prefix(&home) {
            Some(rest) if !home.is_empty() => format!("~{rest}"),
            _ => label,
        };
        div()
            .pt(px(72.))
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .text_size(px(44.))
                    .font_family(MONO_FONT)
                    .child(div().text_color(t.faint).child("port"))
                    .child(
                        div()
                            .text_color(t.accent)
                            .font_weight(FontWeight::BOLD)
                            .child("code"),
                    ),
            )
            .child(div().text_color(t.muted).child(format!(
                "Working in {label}. Say what you want done, or start from one of these."
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(SUGGESTIONS.iter().enumerate().map(|(i, s)| {
                        div()
                            .id(("suggest", i))
                            .cursor_pointer()
                            .rounded_lg()
                            .border_1()
                            .border_color(t.rule)
                            .bg(t.panel)
                            .px_3()
                            .py_2()
                            .hover(|d| d.border_color(t.accent))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.input = SUGGESTIONS[i].to_string();
                                cx.notify();
                            }))
                            .child(*s)
                    })),
            )
            .into_any_element()
    }

    fn chip(
        &self,
        id: &'static str,
        label: String,
        color: Rgba,
        on: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let t = self.t();
        div()
            .id(id)
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_1p5()
            .h(px(26.))
            .px_2p5()
            .rounded_full()
            .border_1()
            .border_color(if on { color } else { t.rule })
            .text_size(px(12.))
            .text_color(if on { color } else { t.muted })
            .hover(|d| d.bg(t.raised))
            .child(div().size(px(7.)).rounded_full().bg(color))
            .child(label)
    }

    fn composer(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        let placeholder = if self.st.busy {
            "Type to steer the agent. Esc interrupts."
        } else {
            "Ask anything. Enter sends, Shift+Enter adds a line."
        };
        let mut text = div()
            .min_h(px(44.))
            .max_h(px(220.))
            .overflow_hidden()
            .px_4()
            .pt_3()
            .flex()
            .flex_col()
            .line_height(relative(1.5));
        let caret = || div().w(px(2.)).h(px(17.)).bg(t.accent).flex_shrink_0();
        if self.input.is_empty() {
            text = text.child(
                div()
                    .flex()
                    .items_center()
                    .child(caret())
                    .child(div().ml_1().text_color(t.faint).child(placeholder)),
            );
        } else {
            let lines: Vec<&str> = self.input.split('\n').collect();
            let last = lines.len().saturating_sub(1);
            for (i, l) in lines.iter().enumerate() {
                let mut row = div()
                    .flex()
                    .items_center()
                    .child(div().child(if l.is_empty() {
                        " ".to_string()
                    } else {
                        l.to_string()
                    }));
                if i == last {
                    row = row.child(caret());
                }
                text = text.child(row);
            }
        }
        let busy = self.st.busy;
        let can_send = !self.input.trim().is_empty();
        let send = div()
            .id("send")
            .cursor_pointer()
            .size(px(30.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(if can_send || busy { t.accent } else { t.raised })
            .text_color(if can_send || busy {
                t.on_accent
            } else {
                t.faint
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if this.st.busy && this.input.trim().is_empty() {
                    if let Some(h) = &this.handle {
                        h.interrupt();
                    }
                } else {
                    this.submit();
                }
                cx.notify();
            }))
            .child(if busy && !can_send { "■" } else { "↑" });
        let agent = self.st.agent.clone();
        div()
            .px_8()
            .pb_5()
            .child(
                div()
                    .w_full()
                    .max_w(px(780.))
                    .mx_auto()
                    .rounded_xl()
                    .border_1()
                    .border_color(t.rule)
                    .bg(t.panel)
                    .child(text)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .child(
                                self.chip(
                                    "chip-agent",
                                    capitalise(&agent),
                                    agent_color(&t, &agent),
                                    true,
                                )
                                .on_click(cx.listener(
                                    |this, _: &ClickEvent, _, cx| {
                                        this.cycle_agent();
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(div().text_size(px(12.)).text_color(t.muted).child(
                                if self.st.model.is_empty() {
                                    "connecting…".to_string()
                                } else {
                                    self.st.model.clone()
                                },
                            ))
                            .child(
                                self.chip("chip-deep", "deep".into(), t.inferred, self.st.deep)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        let on = !this.st.deep;
                                        this.set_deep(on);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.chip(
                                    "chip-allow",
                                    "allow-all".into(),
                                    t.unknown,
                                    self.st.auto_allow,
                                )
                                .on_click(cx.listener(
                                    |this, _: &ClickEvent, _, cx| {
                                        this.toggle_allow_all();
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(div().flex_1())
                            .child(send),
                    ),
            )
            .into_any_element()
    }

    fn section(&self, title: &str) -> gpui::Div {
        div().flex().flex_col().gap_2().child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(self.t().muted)
                .child(title.to_string()),
        )
    }

    fn bar(&self, parts: &[(f32, Rgba)], h: f32) -> gpui::Div {
        let t = self.t();
        let mut b = div()
            .flex()
            .h(px(h))
            .rounded_full()
            .overflow_hidden()
            .bg(t.raised);
        for (f, c) in parts {
            if *f > 0.0 {
                b = b.child(div().h_full().w(relative(*f)).bg(*c));
            }
        }
        b
    }

    fn sidebar_view(&self) -> gpui::AnyElement {
        let t = self.t();
        let st = &self.st;
        let pct = st.context_pct();
        let ctx_col = if pct >= 85 {
            t.unknown
        } else if pct >= 60 {
            t.inferred
        } else {
            t.seen
        };
        let (o, i, u) = st.ledger;
        let total = (o + i + u).max(1) as f32;
        let (done, all, tpct) = st.todo_progress();
        let k = |n: u64| {
            if n >= 1000 {
                format!("{:.1}k", n as f64 / 1000.0)
            } else {
                n.to_string()
            }
        };

        let proof = match &st.parity {
            Some((ok, _)) => {
                let col = if *ok { t.seen } else { t.unknown };
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div().flex().child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .border_2()
                                .border_color(col)
                                .text_color(col)
                                .font_family(MONO_FONT)
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(15.))
                                .child(if *ok { "PASS" } else { "FAIL" }),
                        ),
                    )
                    .when_some(st.parity_counts(), |d, (m, n)| {
                        d.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.muted)
                                .child(format!("{m} of {n} cases match · {}%", m * 100 / n.max(1))),
                        )
                    })
            }
            None => div()
                .text_size(px(12.5))
                .text_color(t.faint)
                .child("No parity run yet. Nothing here claims a match."),
        };

        div()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(t.rule)
            .bg(t.panel)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_5()
                    .py_5()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        self.section("Context")
                            .child(self.bar(&[((pct as f32 / 100.0).min(1.0), ctx_col)], 6.))
                            .child(div().text_size(px(12.5)).text_color(t.muted).child(format!(
                                "{pct}% · {} of {} tokens",
                                k(st.last_prompt),
                                k(u64::from(st.ctx_window))
                            ))),
                    )
                    .child(
                        self.section("Evidence")
                            .child(self.bar(
                                &[
                                    (o as f32 / total, t.seen),
                                    (i as f32 / total, t.inferred),
                                    (u as f32 / total, t.unknown),
                                ],
                                8.,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .gap_3()
                                    .text_size(px(12.5))
                                    .child(div().text_color(t.seen).child(format!("{o} seen")))
                                    .child(
                                        div().text_color(t.inferred).child(format!("{i} inferred")),
                                    )
                                    .child(div().text_color(t.unknown).child(format!("{u} open"))),
                            )
                            .children(st.unknowns.iter().map(|s| {
                                div()
                                    .flex()
                                    .gap_2()
                                    .text_size(px(12.5))
                                    .text_color(t.muted)
                                    .child(div().text_color(t.unknown).child("?"))
                                    .child(div().flex_1().min_w_0().child(s.clone()))
                            })),
                    )
                    .child(self.section("Proof").child(proof))
                    .when(!st.todos.is_empty(), |d| {
                        d.child(
                            self.section("Checklist")
                                .child(self.bar(&[(tpct as f32 / 100.0, t.accent)], 6.))
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(t.muted)
                                        .child(format!("{done} of {all} done · {tpct}%")),
                                )
                                .children(st.todos.iter().map(|td| {
                                    let (g, c) = match td.status.as_str() {
                                        "completed" => ("☑", t.seen),
                                        "in_progress" => ("◐", t.accent),
                                        _ => ("☐", t.faint),
                                    };
                                    div()
                                        .flex()
                                        .gap_2()
                                        .text_size(px(13.))
                                        .child(div().text_color(c).child(g))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_color(if td.status == "completed" {
                                                    t.faint
                                                } else {
                                                    t.text
                                                })
                                                .child(td.content.clone()),
                                        )
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .px_5()
                    .py_3()
                    .border_t_1()
                    .border_color(t.rule)
                    .flex()
                    .text_size(px(12.))
                    .font_family(MONO_FONT)
                    .child(div().text_color(t.faint).child("port"))
                    .child(
                        div()
                            .text_color(t.accent)
                            .font_weight(FontWeight::BOLD)
                            .child("code "),
                    )
                    .child(div().text_color(t.faint).child(env!("CARGO_PKG_VERSION"))),
            )
            .into_any_element()
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        div()
            .h(px(44.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .pl(px(84.))
            .pr_4()
            .border_b_1()
            .border_color(t.rule)
            .window_control_area(WindowControlArea::Drag)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.muted)
                    .child(
                        self.st
                            .title
                            .clone()
                            .unwrap_or_else(|| "New session".to_string()),
                    ),
            )
            .child(
                div()
                    .id("theme")
                    .cursor_pointer()
                    .px_2()
                    .text_color(t.faint)
                    .hover(|d| d.text_color(t.text))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_theme();
                        cx.notify();
                    }))
                    .child(if self.dark { "☾" } else { "☀" }),
            )
            .child(
                div()
                    .id("sidebar-toggle")
                    .cursor_pointer()
                    .px_2()
                    .text_color(if self.sidebar { t.accent } else { t.faint })
                    .hover(|d| d.text_color(t.text))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.sidebar = !this.sidebar;
                        cx.notify();
                    }))
                    .child("◧"),
            )
            .into_any_element()
    }

    fn permission(&self, p: &Perm, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.t();
        let btn =
            |id: &'static str, label: &'static str, key: &'static str, color: Rgba, solid: bool| {
                div()
                    .id(id)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.))
                    .px_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(color)
                    .bg(if solid { color } else { t.panel })
                    .text_color(if solid { t.on_accent } else { color })
                    .text_size(px(13.))
                    .child(label)
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(11.))
                            .opacity(0.7)
                            .child(key),
                    )
            };
        div()
            .absolute()
            .inset_0()
            .bg(gpui::rgba(if t.dark { 0x000000a8 } else { 0x1b1f2766 }))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(520.))
                    .rounded_xl()
                    .border_1()
                    .border_color(t.inferred)
                    .bg(t.panel)
                    .shadow_lg()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(t.inferred)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Permission needed"),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(12.5))
                                    .text_color(t.muted)
                                    .child(p.req.tool.clone()),
                            ),
                    )
                    .child(
                        div()
                            .rounded_lg()
                            .bg(t.raised)
                            .p_3()
                            .max_h(px(160.))
                            .overflow_hidden()
                            .font_family(MONO_FONT)
                            .text_size(px(12.5))
                            .child(SharedString::from(p.req.target.clone())),
                    )
                    .when(!p.req.reason.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(t.muted)
                                .child(p.req.reason.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(btn("p-once", "Allow once", "y", t.seen, true).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.st.answer(Reply::Once);
                                    cx.notify();
                                }),
                            ))
                            .child(btn("p-always", "Always", "a", t.accent, false).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.st.answer(Reply::Always);
                                    cx.notify();
                                }),
                            ))
                            .child(btn("p-all", "Allow all", "A", t.inferred, false).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.st.allow_all();
                                    cx.notify();
                                }),
                            ))
                            .child(div().flex_1())
                            .child(btn("p-no", "Reject", "n", t.unknown, false).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.st.answer(Reply::Reject);
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

impl Render for Root {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.t();
        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.transcript(cx))
            .child(self.composer(cx));
        let perm = self.st.perm.as_ref().map(|p| self.permission(p, cx));
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(UI_FONT)
            .text_size(px(14.))
            .child(self.titlebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(main)
                    .when(self.sidebar, |d| d.child(self.sidebar_view())),
            )
            .children(perm)
    }
}
