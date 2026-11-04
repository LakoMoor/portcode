//! Subscription backends: run a turn through the user's own `claude` (Claude Code) or `codex` CLI.
//!
//! PortCode never touches their login or tokens. It starts the official CLI as a child process in
//! non-interactive mode, hands it PortCode's tools over MCP (`portcode mcp`) and turns its JSON event
//! stream into the same [`UiEvent`]s the built-in loop emits. The CLI runs its own agent loop, so PortCode's
//! finish gate, step limits and permission prompts do not apply to it.
use crate::run::{Ui, UiEvent};
use portcode_llm::Usage;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    Claude,
    Codex,
}

impl Cli {
    pub fn provider(self) -> &'static str {
        match self {
            Cli::Claude => "claude-code",
            Cli::Codex => "codex",
        }
    }
    pub fn binary_name(self) -> &'static str {
        match self {
            Cli::Claude => "claude",
            Cli::Codex => "codex",
        }
    }
    pub fn login_hint(self) -> &'static str {
        match self {
            Cli::Claude => "run `claude` once and log in with your Claude subscription",
            Cli::Codex => "run `codex login` and sign in with your ChatGPT plan",
        }
    }
}

pub fn cli_for(provider: &str) -> Option<Cli> {
    match provider {
        "claude-code" => Some(Cli::Claude),
        "codex" => Some(Cli::Codex),
        _ => None,
    }
}

/// Find the CLI on PATH (and the usual user-level install folders).
pub fn find_binary(cli: Cli) -> Option<PathBuf> {
    let name = cli.binary_name();
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([
            h.join(".local/bin"),
            h.join(".claude/local"),
            h.join(".npm-global/bin"),
        ]);
    }
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

/// Model names worth offering. Claude Code takes aliases or full ids; Codex takes any id your plan includes.
pub fn model_hints(cli: Cli) -> Vec<String> {
    let v: &[&str] = match cli {
        Cli::Claude => &["default", "sonnet", "opus", "haiku"],
        Cli::Codex => &["default"],
    };
    v.iter().map(|s| s.to_string()).collect()
}

/// One line for `doctor` and `/provider`: is the CLI installed, and how it signs in.
pub fn status(cli: Cli) -> (bool, String) {
    match find_binary(cli) {
        Some(p) => (
            true,
            format!(
                "{} · signs in with your subscription ({})",
                p.display(),
                cli.login_hint()
            ),
        ),
        None => (
            false,
            format!(
                "{} not found. Install it, then {}",
                cli.binary_name(),
                cli.login_hint()
            ),
        ),
    }
}

/// What a finished turn left behind.
#[derive(Debug, Default, Clone)]
pub struct Turn {
    pub text: String,
    pub session: Option<String>,
    pub usage: Usage,
    pub steps: u32,
    pub error: Option<String>,
}

/// Turns one CLI's JSON lines into UI events. Pure, so it is testable without the CLI.
pub struct Parser {
    cli: Cli,
    names: HashMap<String, String>,
    pub turn: Turn,
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        o => o.to_string(),
    }
}

impl Parser {
    pub fn new(cli: Cli) -> Self {
        Self {
            cli,
            names: HashMap::new(),
            turn: Turn::default(),
        }
    }

    pub fn feed(&mut self, line: &str, ui: &dyn Ui) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return;
        };
        match self.cli {
            Cli::Claude => self.claude(&v, ui),
            Cli::Codex => self.codex(&v, ui),
        }
    }

    fn call(&mut self, id: &str, name: &str, args: String, ui: &dyn Ui) {
        self.names.insert(id.to_string(), name.to_string());
        self.turn.steps += 1;
        ui.event(UiEvent::ToolStart(name.to_string()));
        ui.event(UiEvent::ToolCall {
            name: name.to_string(),
            args,
        });
    }
    fn result(&mut self, id: &str, ok: bool, text: String, ui: &dyn Ui) {
        let name = self.names.remove(id).unwrap_or_else(|| "tool".into());
        ui.event(UiEvent::ToolResult {
            title: name.clone(),
            name,
            ok,
            text,
        });
    }

    fn claude(&mut self, v: &Value, ui: &dyn Ui) {
        if let Some(s) = v.get("session_id").and_then(Value::as_str) {
            self.turn.session = Some(s.to_string());
        }
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for b in v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match b.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let t = b.get("text").and_then(Value::as_str).unwrap_or("");
                            if !t.trim().is_empty() {
                                ui.event(UiEvent::Text(t.to_string()));
                            }
                        }
                        Some("thinking") => {
                            if let Some(t) = b.get("thinking").and_then(Value::as_str) {
                                ui.event(UiEvent::Reasoning(t.to_string()));
                            }
                        }
                        Some("tool_use") => {
                            let id = b.get("id").and_then(Value::as_str).unwrap_or("");
                            let name = b.get("name").and_then(Value::as_str).unwrap_or("tool");
                            let name = name.strip_prefix("mcp__portcode__").unwrap_or(name);
                            let args = b.get("input").cloned().unwrap_or(Value::Null).to_string();
                            self.call(id, name, args, ui);
                        }
                        _ => {}
                    }
                }
            }
            Some("user") => {
                for b in v
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if b.get("type").and_then(Value::as_str) == Some("tool_result") {
                        let id = b.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                        let bad = b.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                        let text = text_of(b.get("content").unwrap_or(&Value::Null));
                        self.result(id, !bad, text, ui);
                    }
                }
            }
            Some("result") => {
                self.turn.text = v
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let u = v.get("usage");
                let n = |k: &str| {
                    u.and_then(|u| u.get(k))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                self.turn.usage = Usage {
                    prompt_tokens: n("input_tokens")
                        + n("cache_creation_input_tokens")
                        + n("cache_read_input_tokens"),
                    completion_tokens: n("output_tokens"),
                };
                if v.get("is_error").and_then(Value::as_bool) == Some(true) {
                    self.turn.error = Some(if self.turn.text.is_empty() {
                        "Claude Code reported an error".into()
                    } else {
                        self.turn.text.clone()
                    });
                }
            }
            _ => {}
        }
    }

    fn codex(&mut self, v: &Value, ui: &dyn Ui) {
        match v.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                self.turn.session = v.get("thread_id").and_then(Value::as_str).map(String::from);
            }
            Some(k @ ("item.started" | "item.completed")) => {
                let done = k == "item.completed";
                let Some(it) = v.get("item") else { return };
                let id = it.get("id").and_then(Value::as_str).unwrap_or("");
                let s = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                match it.get("type").and_then(Value::as_str) {
                    Some("agent_message") if done => {
                        self.turn.text = s("text");
                        ui.event(UiEvent::Text(s("text")));
                    }
                    Some("reasoning") if done => ui.event(UiEvent::Reasoning(s("text"))),
                    Some("command_execution") => {
                        if done {
                            let ok = it.get("exit_code").and_then(Value::as_i64) == Some(0);
                            if !self.names.contains_key(id) {
                                self.call(
                                    id,
                                    "shell",
                                    json!({"command": s("command")}).to_string(),
                                    ui,
                                );
                            }
                            self.result(id, ok, s("aggregated_output"), ui);
                        } else {
                            self.call(
                                id,
                                "shell",
                                json!({"command": s("command")}).to_string(),
                                ui,
                            );
                        }
                    }
                    Some("file_change") if done => {
                        let files: Vec<String> = it
                            .get("changes")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|c| c.get("path").and_then(Value::as_str).map(String::from))
                            .collect();
                        self.call(id, "apply_patch", json!({"files": files}).to_string(), ui);
                        self.result(id, s("status") != "failed", files.join("\n"), ui);
                    }
                    Some("mcp_tool_call") => {
                        let name = s("tool");
                        if done {
                            if !self.names.contains_key(id) {
                                let a = it
                                    .get("arguments")
                                    .cloned()
                                    .unwrap_or(Value::Null)
                                    .to_string();
                                self.call(id, &name, a, ui);
                            }
                            let ok = s("status") != "failed";
                            let out =
                                text_of(it.pointer("/result/content").unwrap_or(&Value::Null));
                            self.result(id, ok, out, ui);
                        } else {
                            let a = it
                                .get("arguments")
                                .cloned()
                                .unwrap_or(Value::Null)
                                .to_string();
                            self.call(id, &name, a, ui);
                        }
                    }
                    Some("web_search") if done => {
                        self.call(
                            id,
                            "web_search",
                            json!({"query": s("query")}).to_string(),
                            ui,
                        );
                        self.result(id, true, String::new(), ui);
                    }
                    _ => {}
                }
            }
            Some("turn.completed") => {
                let u = v.get("usage");
                let n = |k: &str| {
                    u.and_then(|u| u.get(k))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                };
                self.turn.usage = Usage {
                    prompt_tokens: n("input_tokens"),
                    completion_tokens: n("output_tokens"),
                };
            }
            Some("turn.failed") => {
                self.turn.error = Some(
                    v.pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex reported a failure")
                        .to_string(),
                );
            }
            Some("error") => {
                self.turn.error = Some(
                    v.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex reported an error")
                        .to_string(),
                );
            }
            _ => {}
        }
    }
}

pub struct Request<'a> {
    pub cli: Cli,
    pub bin: &'a Path,
    pub model: &'a str,
    pub prompt: &'a str,
    pub brief: &'a str,
    pub root: &'a Path,
    pub session: Option<&'a str>,
    pub deep: bool,
    pub readonly: bool,
    /// Claude Code `--permission-mode` (`acceptEdits`, `plan`, `bypassPermissions`, `default`).
    pub permission: &'a str,
}

/// Command line for one turn. Kept apart from spawning so tests can read it.
pub fn args(r: &Request, mcp_config: &Path) -> Vec<String> {
    let model = (!r.model.is_empty() && r.model != "default").then(|| r.model.to_string());
    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "portcode".into());
    let mut a: Vec<String> = Vec::new();
    match r.cli {
        Cli::Claude => {
            a.extend([
                "-p".into(),
                r.prompt.into(),
                "--output-format".into(),
                "stream-json".into(),
                "--verbose".into(),
            ]);
            a.extend(["--append-system-prompt".into(), r.brief.into()]);
            a.extend(["--mcp-config".into(), mcp_config.display().to_string()]);
            let mode = if r.readonly { "plan" } else { r.permission };
            a.extend(["--permission-mode".into(), mode.into()]);
            a.extend(["--allowedTools".into(), "mcp__portcode".into()]);
            if let Some(m) = model {
                a.extend(["--model".into(), m]);
            }
            if let Some(s) = r.session {
                a.extend(["--resume".into(), s.into()]);
            }
        }
        Cli::Codex => {
            a.extend([
                "exec".into(),
                "--json".into(),
                "--skip-git-repo-check".into(),
            ]);
            a.extend(["-C".into(), r.root.display().to_string()]);
            let sandbox = if r.readonly {
                "read-only"
            } else {
                "workspace-write"
            };
            a.extend(["--sandbox".into(), sandbox.into()]);
            if let Some(m) = model {
                a.extend(["-m".into(), m]);
            }
            let mut mcp = vec![json!("mcp")];
            if r.deep {
                mcp.push(json!("--deep"));
            }
            a.extend([
                "-c".into(),
                format!("mcp_servers.portcode.command={}", json!(exe)),
            ]);
            a.extend([
                "-c".into(),
                format!("mcp_servers.portcode.args={}", Value::Array(mcp)),
            ]);
            // Codex has no system-prompt flag: the brief goes in front of the task.
            a.push(format!("{}\n\n# Task\n{}", r.brief, r.prompt));
        }
    }
    a
}

/// `--mcp-config` file for Claude Code: PortCode's tools, started from this same executable.
pub fn write_mcp_config(root: &Path, deep: bool) -> Result<PathBuf, String> {
    let dir = root.join(".portcode");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "portcode".into());
    let mut args = vec![json!("mcp")];
    if deep {
        args.push(json!("--deep"));
    }
    let cfg = json!({"mcpServers": {"portcode": {"command": exe, "args": args}}});
    let p = dir.join("cli-mcp.json");
    std::fs::write(&p, cfg.to_string()).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Run one turn and stream it to `ui`. Honours `cancel` by killing the child.
pub async fn run(r: &Request<'_>, ui: &dyn Ui, cancel: &AtomicBool) -> Result<Turn, String> {
    let cfg = write_mcp_config(r.root, r.deep)?;
    let mut cmd = tokio::process::Command::new(r.bin);
    cmd.args(args(r, &cfg))
        .current_dir(r.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "could not start {}: {e}. Install it and {}.",
            r.cli.binary_name(),
            r.cli.login_hint()
        )
    })?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take();
    let err_task = tokio::spawn(async move {
        let mut s = String::new();
        if let Some(e) = stderr {
            let mut l = BufReader::new(e).lines();
            while let Ok(Some(x)) = l.next_line().await {
                if s.len() < 2000 {
                    s.push_str(&x);
                    s.push('\n');
                }
            }
        }
        s
    });
    let mut parser = Parser::new(r.cli);
    let mut lines = BufReader::new(stdout).lines();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60 * 60);
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            parser.turn.error = Some("interrupted".into());
            break;
        }
        match tokio::time::timeout(Duration::from_millis(250), lines.next_line()).await {
            Err(_) => {
                if tokio::time::Instant::now() > deadline {
                    let _ = child.kill().await;
                    return Err("the CLI ran for an hour without finishing; stopped it".into());
                }
            }
            Ok(Ok(Some(l))) => parser.feed(&l, ui),
            Ok(Ok(None)) | Ok(Err(_)) => break,
        }
    }
    let status = child.wait().await.ok();
    let stderr = err_task.await.unwrap_or_default();
    let mut turn = parser.turn;
    if turn.error.is_none() && status.is_some_and(|s| !s.success()) {
        let hint =
            if stderr.to_lowercase().contains("login") || stderr.to_lowercase().contains("auth") {
                format!(" Not signed in? {}.", r.cli.login_hint())
            } else {
                String::new()
            };
        turn.error = Some(format!(
            "{} exited with an error: {}{hint}",
            r.cli.binary_name(),
            stderr.trim()
        ));
    }
    Ok(turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Rec(Mutex<Vec<String>>);
    impl Ui for Rec {
        fn event(&self, e: UiEvent) {
            let s = match e {
                UiEvent::Text(t) => format!("text:{t}"),
                UiEvent::Reasoning(t) => format!("think:{t}"),
                UiEvent::ToolStart(n) => format!("start:{n}"),
                UiEvent::ToolCall { name, args } => format!("call:{name} {args}"),
                UiEvent::ToolResult { name, ok, text, .. } => format!("result:{name} {ok} {text}"),
                _ => return,
            };
            self.0.lock().unwrap().push(s);
        }
    }

    // Lines recorded from `claude -p --output-format stream-json --verbose` (trimmed).
    const CLAUDE: &str = r#"{"type":"system","subtype":"init","session_id":"s-1","model":"claude-sonnet-5-5"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"note.txt"}}]},"session_id":"s-1"}
{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}
{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"1\thello portcode"}]},"session_id":"s-1"}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"mcp__portcode__evidence_add","input":{"kind":"observation"}}]}}
{"type":"user","message":{"content":[{"tool_use_id":"t2","type":"tool_result","is_error":true,"content":[{"type":"text","text":"missing subject"}]}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"The note says hello portcode."}]}}
{"type":"result","subtype":"success","is_error":false,"result":"The note says hello portcode.","session_id":"s-1","usage":{"input_tokens":4,"cache_creation_input_tokens":100,"cache_read_input_tokens":900,"output_tokens":136}}"#;

    #[test]
    fn claude_stream_becomes_ui_events_and_a_turn() {
        let ui = Rec::default();
        let mut p = Parser::new(Cli::Claude);
        for l in CLAUDE.lines() {
            p.feed(l, &ui);
        }
        let ev = ui.0.lock().unwrap().clone();
        assert_eq!(ev[0], "start:Read");
        assert!(ev[1].starts_with("call:Read ") && ev[1].contains("note.txt"));
        assert_eq!(ev[2], "result:Read true 1\thello portcode");
        assert_eq!(ev[3], "start:evidence_add", "the mcp prefix is dropped");
        assert_eq!(ev[4], "call:evidence_add {\"kind\":\"observation\"}");
        assert_eq!(ev[5], "result:evidence_add false missing subject");
        assert_eq!(ev[6], "text:The note says hello portcode.");
        assert_eq!(p.turn.text, "The note says hello portcode.");
        assert_eq!(p.turn.session.as_deref(), Some("s-1"));
        assert_eq!(
            (p.turn.usage.prompt_tokens, p.turn.usage.completion_tokens),
            (1004, 136)
        );
        assert_eq!(p.turn.steps, 2);
        assert!(p.turn.error.is_none());
    }

    #[test]
    fn claude_error_result_is_reported() {
        let mut p = Parser::new(Cli::Claude);
        p.feed(r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Not logged in"}"#, &Rec::default());
        assert_eq!(p.turn.error.as_deref(), Some("Not logged in"));
    }

    // Shapes of `codex exec --json` as documented by OpenAI. Not recorded from a live run: no Codex CLI was available.
    const CODEX: &str = r#"{"type":"thread.started","thread_id":"th-9"}
{"type":"turn.started"}
{"type":"item.started","item":{"id":"i1","type":"command_execution","command":"ls","status":"in_progress"}}
{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"ls","aggregated_output":"a\nb","exit_code":0,"status":"completed"}}
{"type":"item.completed","item":{"id":"i2","type":"mcp_tool_call","server":"portcode","tool":"parity","arguments":{"golden":"g"},"result":{"content":[{"type":"text","text":"RESULT: PASS"}]},"status":"completed"}}
{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"Done."}}
{"type":"turn.completed","usage":{"input_tokens":50,"output_tokens":7}}"#;

    #[test]
    fn codex_stream_becomes_ui_events_and_a_turn() {
        let ui = Rec::default();
        let mut p = Parser::new(Cli::Codex);
        for l in CODEX.lines() {
            p.feed(l, &ui);
        }
        let ev = ui.0.lock().unwrap().clone();
        assert_eq!(ev[0], "start:shell");
        assert_eq!(ev[2], "result:shell true a\nb");
        assert_eq!(ev[3], "start:parity");
        assert_eq!(ev[5], "result:parity true RESULT: PASS");
        assert_eq!(ev[6], "text:Done.");
        assert_eq!(p.turn.session.as_deref(), Some("th-9"));
        assert_eq!(p.turn.usage.prompt_tokens, 50);
    }

    #[test]
    fn command_lines_carry_mcp_tools_resume_and_modes() {
        let root = Path::new("/p");
        let bin = Path::new("claude");
        let mut r = Request {
            cli: Cli::Claude,
            bin,
            model: "sonnet",
            prompt: "port it",
            brief: "B",
            root,
            session: Some("abc"),
            deep: false,
            readonly: false,
            permission: "acceptEdits",
        };
        let a = args(&r, Path::new("/p/.portcode/cli-mcp.json"));
        let has = |a: &[String], k: &str, v: &str| a.windows(2).any(|w| w[0] == k && w[1] == v);
        assert!(has(&a, "--model", "sonnet") && has(&a, "--resume", "abc"));
        assert!(
            has(&a, "--permission-mode", "acceptEdits")
                && has(&a, "--allowedTools", "mcp__portcode")
        );
        assert!(has(&a, "--append-system-prompt", "B") && a.contains(&"stream-json".to_string()));
        r.readonly = true;
        r.model = "default";
        let a = args(&r, Path::new("x"));
        assert!(has(&a, "--permission-mode", "plan") && !a.contains(&"--model".to_string()));
        r.cli = Cli::Codex;
        r.deep = true;
        let a = args(&r, Path::new("x"));
        assert_eq!(a[0], "exec");
        assert!(
            has(&a, "--sandbox", "read-only")
                && a.iter().any(|x| x.contains("[\"mcp\",\"--deep\"]"))
        );
        assert!(a.last().unwrap().starts_with("B\n\n# Task\nport it"));
    }

    #[test]
    fn providers_map_to_clis() {
        assert_eq!(cli_for("claude-code"), Some(Cli::Claude));
        assert_eq!(cli_for("codex"), Some(Cli::Codex));
        assert_eq!(cli_for("ollama"), None);
    }
}
