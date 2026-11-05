//! Behaviour contract with OpenCode's terminal UI: the keys, slash commands and layout people already know.
//! If one of these fails, PortCode drifted from OpenCode, which is a bug unless docs/OPENCODE_MAPPING.md says otherwise.
use super::app::{App, Item, Msg, SLASH};
use super::view;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portcode_agent::Config;
use ratatui::{backend::TestBackend, Terminal};
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::mpsc;

fn app() -> (App, mpsc::UnboundedReceiver<super::app::Cmd>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let a = App::new(
        Config::default(),
        std::env::temp_dir(),
        "build",
        "deepseek-chat",
        "lakomoor",
        64_000,
        "~/proj".into(),
        tx,
        Arc::new(AtomicBool::new(false)),
    );
    (a, rx)
}
fn k(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}
fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}
fn type_str(a: &mut App, s: &str) {
    for c in s.chars() {
        a.on_key(k(KeyCode::Char(c)));
    }
}
fn screen(a: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| view::draw(f, a)).unwrap();
    let b = t.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| b[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn infos(a: &App) -> String {
    a.items
        .iter()
        .filter_map(|i| match i {
            Item::Info(t) | Item::Error(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- prompt keys ----

#[test]
fn enter_sends_and_alt_enter_or_ctrl_j_adds_a_line() {
    let (mut a, _rx) = app();
    type_str(&mut a, "hi");
    a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    type_str(&mut a, "there");
    assert_eq!(a.input, "hi\nthere");
    a.on_key(ctrl('j'));
    assert!(a.input.ends_with("there\n"));
    a.on_key(k(KeyCode::Enter));
    assert!(a.input.is_empty() && a.busy);
    assert!(matches!(a.items.last(), Some(Item::User(t)) if t.starts_with("hi\nthere")));
}

#[test]
fn up_and_down_walk_prompt_history_and_restore_the_draft() {
    let (mut a, _rx) = app();
    for t in ["one", "two"] {
        type_str(&mut a, t);
        a.on_key(k(KeyCode::Enter));
        a.busy = false;
    }
    a.on_key(k(KeyCode::Up));
    assert_eq!(a.input, "two");
    a.on_key(k(KeyCode::Up));
    assert_eq!(a.input, "one");
    a.on_key(k(KeyCode::Down));
    a.on_key(k(KeyCode::Down));
    assert_eq!(a.input, "");
}

#[test]
fn readline_editing_keys() {
    let (mut a, _rx) = app();
    type_str(&mut a, "alpha beta");
    a.on_key(ctrl('w'));
    assert_eq!(a.input, "alpha ");
    a.on_key(ctrl('a'));
    type_str(&mut a, ">");
    assert_eq!(a.input, ">alpha ");
    a.on_key(ctrl('e'));
    a.on_key(k(KeyCode::Left));
    a.on_key(k(KeyCode::Backspace));
    assert_eq!(a.input, ">alph ");
    a.on_key(ctrl('u'));
    assert_eq!(a.input, " ", "ctrl+u deletes up to the cursor");
}

#[test]
fn ctrl_c_clears_the_prompt_then_quits_and_interrupts_when_busy() {
    let (mut a, _rx) = app();
    type_str(&mut a, "draft");
    a.on_key(ctrl('c'));
    assert!(a.input.is_empty() && !a.quit);
    a.busy = true;
    a.on_key(ctrl('c'));
    assert!(!a.quit, "ctrl+c while working only interrupts");
    a.busy = false;
    a.on_key(ctrl('c'));
    assert!(a.quit);
}

#[test]
fn two_escapes_cancel_a_running_session_and_ctrl_d_quits_when_empty() {
    let (mut a, _rx) = app();
    a.busy = true;
    a.on_key(k(KeyCode::Esc));
    assert!(
        !a.cancel.load(std::sync::atomic::Ordering::Relaxed),
        "one esc only arms"
    );
    assert!(a.esc_armed());
    a.on_key(k(KeyCode::Esc));
    assert!(
        a.cancel.load(std::sync::atomic::Ordering::Relaxed),
        "the second esc cancels"
    );
    a.busy = false;
    a.on_key(ctrl('d'));
    assert!(a.quit);
}

#[test]
fn tab_cycles_primary_agents_and_shift_tab_goes_back() {
    let (mut a, _rx) = app();
    let first = a.agent.clone();
    a.on_key(k(KeyCode::Tab));
    let second = a.agent.clone();
    assert_ne!(first, second);
    a.on_key(k(KeyCode::BackTab));
    assert_eq!(a.agent, first);
    assert!(
        !a.agents.iter().any(|n| n == "explore" || n == "general"),
        "sub-agents are not in the Tab cycle"
    );
}

// ---- slash commands ----

#[test]
fn slash_menu_filters_as_you_type_and_hides_after_a_space() {
    let (mut a, _rx) = app();
    type_str(&mut a, "/");
    assert_eq!(a.slash_matches().len(), SLASH.len());
    type_str(&mut a, "mod");
    assert!(a.slash_matches().iter().all(|(n, _)| n.starts_with("mod")));
    type_str(&mut a, "el ");
    assert!(a.slash_matches().is_empty());
}

#[test]
fn opencode_command_names_exist() {
    let names: Vec<&str> = SLASH.iter().map(|(n, _)| *n).collect();
    for n in [
        "new", "models", "agents", "help", "exit", "clear", "thinking",
    ] {
        assert!(names.contains(&n), "/{n} is missing from the menu");
    }
}

#[test]
fn slash_new_starts_a_fresh_session_like_opencode() {
    let (mut a, mut rx) = app();
    a.items.push(Item::Info("old".into()));
    a.input = "/new".into();
    a.on_key(k(KeyCode::Enter));
    assert!(a.items.is_empty() && a.wiz.is_none());
    assert!(matches!(rx.try_recv(), Ok(super::app::Cmd::Clear)));
}

#[test]
fn exit_aliases_quit() {
    for c in ["/exit", "/quit", "/q"] {
        let (mut a, _rx) = app();
        a.input = c.into();
        a.on_key(k(KeyCode::Enter));
        assert!(a.quit, "{c}");
    }
}

#[test]
fn unknown_command_is_an_error_not_a_prompt_to_the_model() {
    let (mut a, mut rx) = app();
    a.input = "/nope".into();
    a.on_key(k(KeyCode::Enter));
    assert!(infos(&a).contains("unknown command /nope"));
    assert!(rx.try_recv().is_err(), "nothing was sent to the model");
}

#[test]
fn agents_and_models_commands_answer() {
    let (mut a, mut rx) = app();
    a.input = "/agents".into();
    a.on_key(k(KeyCode::Enter));
    assert!(infos(&a).contains("build"));
    a.input = "/models".into();
    a.on_key(k(KeyCode::Enter));
    assert!(matches!(rx.try_recv(), Ok(super::app::Cmd::ListModels)));
}

// ---- permission dialog ----

#[test]
fn permission_asks_allow_once_always_or_reject() {
    use portcode_agent::permission::{Reply, Request};
    for (key, want) in [
        ('y', Reply::Once),
        ('a', Reply::Always),
        ('n', Reply::Reject),
    ] {
        let (mut a, _rx) = app();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        a.on_msg(Msg::Perm(super::app::PermAsk {
            req: Request {
                tool: "bash".into(),
                target: "cargo test".into(),
                reason: String::new(),
            },
            reply: tx,
        }));
        let s = screen(&mut a, 100, 30);
        assert!(s.contains("cargo test"), "the dialog names what runs");
        a.on_key(k(KeyCode::Char(key)));
        assert_eq!(rx.try_recv().ok(), Some(want));
    }
}

// ---- layout ----

#[test]
fn home_screen_has_the_prompt_box_agent_model_and_tips() {
    let (mut a, _rx) = app();
    let s = screen(&mut a, 100, 30);
    assert!(s.contains("Ask anything"), "placeholder like OpenCode");
    assert!(s.contains("Build") && s.contains("deepseek-chat") && s.contains("lakomoor"));
    assert!(
        s.contains("tab") && s.contains("ctrl+c"),
        "key tips on the home screen"
    );
}

#[test]
fn session_view_shows_transcript_prompt_and_a_sidebar_on_wide_terminals() {
    let (mut a, _rx) = app();
    a.items.push(Item::User("port wordstat".into()));
    let wide = screen(&mut a, 140, 36);
    assert!(wide.contains("port wordstat"));
    assert!(wide.contains("Context"), "sidebar: context");
    assert!(wide.contains("Evidence"), "sidebar: evidence");
    let narrow = screen(&mut a, 90, 30);
    assert!(
        !narrow.contains("Context"),
        "sidebar hides on narrow terminals"
    );
}

#[test]
fn ctrl_b_toggles_the_sidebar_and_ctrl_t_the_thinking_blocks() {
    let (mut a, _rx) = app();
    a.items.push(Item::User("x".into()));
    a.on_key(ctrl('b'));
    assert!(!screen(&mut a, 140, 36).contains("Context"));
    a.on_key(ctrl('b'));
    assert!(screen(&mut a, 140, 36).contains("Context"));
    let t = a.show_reasoning;
    a.on_key(ctrl('t'));
    assert_ne!(a.show_reasoning, t);
}

#[test]
fn page_keys_scroll_the_transcript() {
    let (mut a, _rx) = app();
    a.on_key(k(KeyCode::PageUp));
    assert_eq!(a.scroll, 10);
    a.on_key(k(KeyCode::PageDown));
    assert_eq!(a.scroll, 0);
}

#[test]
fn every_screen_fits_small_terminals_without_panicking() {
    for (w, h) in [(40, 12), (80, 24), (120, 40), (200, 60)] {
        let (mut a, _rx) = app();
        screen(&mut a, w, h);
        a.items.push(Item::User("hello".into()));
        a.items.push(Item::Error("boom".into()));
        screen(&mut a, w, h);
        a.input = "/".into();
        screen(&mut a, w, h);
    }
}

// ---- PortCode's own additions ----

#[test]
fn parity_result_gets_a_verdict_banner_and_ids_are_coloured() {
    use super::app::ToolState;
    let (mut a, _rx) = app();
    a.items.push(Item::Tool {
        name: "parity".into(),
        args: "{}".into(),
        state: ToolState::Ok,
        title: "parity PASS".into(),
        text: "parity: ok=6 diff=0 missing=0 extra=0\nRESULT: PASS".into(),
    });
    a.items
        .push(Item::Assistant("Recorded E-0002 as unknown.".into()));
    a.unknowns = vec!["E-0002 locale".into()];
    let s = screen(&mut a, 120, 30);
    assert!(s.contains(" PASS ") && s.contains("ok=6 diff=0"));
    assert!(s.contains("E-0002"));
}

#[test]
fn theme_command_switches_palette_and_rejects_unknown_names() {
    let (mut a, _rx) = app();
    a.input = "/theme light".into();
    a.on_key(k(KeyCode::Enter));
    assert!(super::theme::is_light());
    let s = screen(&mut a, 100, 30);
    assert!(s.contains("Ask anything"), "still renders");
    a.input = "/theme dark".into();
    a.on_key(k(KeyCode::Enter));
    assert!(!super::theme::is_light());
    a.input = "/theme neon".into();
    a.on_key(k(KeyCode::Enter));
    assert!(infos(&a).contains("unknown theme"));
}

#[test]
fn yolo_approves_requests_and_shows_a_warning_chip() {
    use portcode_agent::{Reply, Request};
    let (mut a, mut rx) = app();
    type_str(&mut a, "/yolo");
    a.on_key(k(KeyCode::Enter));
    assert!(a.auto_allow);
    assert!(matches!(rx.try_recv(), Ok(super::app::Cmd::Tune(w, v)) if w == "allow" && v == "on"));
    let (tx, mut r) = tokio::sync::oneshot::channel();
    a.on_msg(super::app::Msg::Perm(super::app::PermAsk {
        req: Request {
            tool: "bash".into(),
            target: "rm x".into(),
            reason: String::new(),
        },
        reply: tx,
    }));
    assert_eq!(r.try_recv().ok(), Some(Reply::Once));
    assert!(a.perm.is_none());
    assert!(screen(&mut a, 100, 30).contains("allow-all"));
    type_str(&mut a, "/yolo off");
    a.on_key(k(KeyCode::Enter));
    assert!(!a.auto_allow);
}

#[test]
fn help_and_tools_print_aligned_blocks() {
    let (mut a, _rx) = app();
    type_str(&mut a, "/help");
    a.on_key(k(KeyCode::Enter));
    let s = screen(&mut a, 120, 60);
    assert!(
        s.contains("Keys") && s.contains("Commands") && s.contains("esc esc"),
        "{s}"
    );
    type_str(&mut a, "/tools");
    a.on_key(k(KeyCode::Enter));
    let s = screen(&mut a, 120, 70);
    assert!(s.contains("Tools") && s.contains("proof"), "{s}");
}

#[test]
fn team_command_adds_the_orchestrator_and_parallel_asks_queue_up() {
    use portcode_agent::{Reply, Request};
    let (mut a, _rx) = app();
    assert!(!a.agents.iter().any(|x| x == "team"));
    type_str(&mut a, "/team on");
    a.on_key(k(KeyCode::Enter));
    assert!(a.agents.iter().any(|x| x == "team"));
    let ask = |t: &str| {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            super::app::Msg::Perm(super::app::PermAsk {
                req: Request {
                    tool: "bash".into(),
                    target: t.into(),
                    reason: String::new(),
                },
                reply: tx,
            }),
            rx,
        )
    };
    let ((m1, mut r1), (m2, mut r2)) = (ask("one"), ask("two"));
    a.on_msg(m1);
    a.on_msg(m2);
    assert_eq!(
        a.perm_queue.len(),
        1,
        "the second ask waits, it is not dropped"
    );
    a.on_key(k(KeyCode::Char('y')));
    assert_eq!(r1.try_recv().ok(), Some(Reply::Once));
    assert!(a.perm.as_ref().is_some_and(|p| p.req.target == "two"));
    a.on_key(k(KeyCode::Char('n')));
    assert_eq!(r2.try_recv().ok(), Some(Reply::Reject));
    type_str(&mut a, "/team off");
    a.on_key(k(KeyCode::Enter));
    assert!(!a.agents.iter().any(|x| x == "team"));
}

#[test]
fn resume_picker_lists_sessions_and_enter_opens_the_chosen_one() {
    use super::app::{Cmd, Msg, PickRow};
    let (mut a, mut rx) = app();
    let row = |id: &str, t: &str| PickRow {
        id: id.into(),
        title: t.into(),
        messages: 4,
        ago: "3 min ago".into(),
        current: false,
    };
    a.on_msg(Msg::Pick(vec![
        row("s-2", "port the parser"),
        row("s-1", "fix the build"),
    ]));
    let s = screen(&mut a, 90, 20);
    assert!(
        s.contains("Resume a session") && s.contains("port the parser") && s.contains("3 min ago"),
        "{s}"
    );
    a.on_key(k(KeyCode::Down));
    a.on_key(k(KeyCode::Enter));
    assert!(a.picker.is_none());
    assert!(matches!(rx.try_recv(), Ok(Cmd::Resume(id)) if id == "s-1"));
    a.on_msg(Msg::Pick(vec![row("s-2", "x")]));
    a.on_key(k(KeyCode::Esc));
    assert!(
        a.picker.is_none() && rx.try_recv().is_err(),
        "esc cancels without resuming"
    );
}

#[test]
fn resumed_history_is_redrawn_with_text_and_tool_calls() {
    use super::app::{Item, Msg};
    use portcode_llm::Message;
    let (mut a, _rx) = app();
    let msgs = vec![
        Message::user("add a flag"),
        Message::assistant_text("done, see src/main.rs"),
    ];
    a.on_msg(Msg::Resumed("s-9".into(), msgs));
    assert!(matches!(&a.items[0], Item::User(t) if t == "add a flag"));
    assert!(matches!(&a.items[1], Item::Assistant(t) if t.contains("src/main.rs")));
    assert!(matches!(a.items.last(), Some(Item::Info(t)) if t.contains("s-9")));
}
