//! Terminal UI: colours (respects NO_COLOR / non-tty), streaming output, tool panels, permission prompts.
use async_trait::async_trait;
use portcode_agent::{Gate, Reply, Request, Ui, UiEvent};
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub fn color_on() -> bool {
    std::env::var_os("PORTCODE_FORCE_COLOR").is_some()
        || (std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}
fn paint(code: &str, s: &str) -> String {
    if color_on() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}
pub fn dim(s: &str) -> String {
    paint("2", s)
}
pub fn bold(s: &str) -> String {
    paint("1", s)
}
pub fn green(s: &str) -> String {
    paint("32", s)
}
pub fn red(s: &str) -> String {
    paint("31", s)
}
pub fn yellow(s: &str) -> String {
    paint("33", s)
}
pub fn accent(s: &str) -> String {
    paint("38;5;208", s)
}

pub struct Term {
    pub show_reasoning: bool,
    in_reasoning: AtomicBool,
    reasoning_chars: AtomicUsize,
    wrote_text: AtomicBool,
}
impl Term {
    pub fn new(show_reasoning: bool) -> Self {
        Self {
            show_reasoning,
            in_reasoning: false.into(),
            reasoning_chars: 0.into(),
            wrote_text: false.into(),
        }
    }
    fn end_reasoning(&self) {
        if self.in_reasoning.swap(false, Ordering::Relaxed) {
            if self.show_reasoning {
                println!();
            } else {
                println!(
                    "{}",
                    dim(&format!(
                        "  · thought for {} chars",
                        self.reasoning_chars.swap(0, Ordering::Relaxed)
                    ))
                );
            }
        }
    }
}

fn indent_preview(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = lines
        .iter()
        .take(max)
        .map(|l| {
            let l: String = l.chars().take(140).collect();
            if let Some(r) = l.strip_prefix('+').filter(|r| !r.starts_with('+')) {
                format!("    {}", green(&format!("+{r}")))
            } else if let Some(r) = l.strip_prefix('-').filter(|r| !r.starts_with('-')) {
                format!("    {}", red(&format!("-{r}")))
            } else {
                format!("    {}", dim(&l))
            }
        })
        .collect();
    if lines.len() > max {
        out.push(dim(&format!("    … {} more lines", lines.len() - max)));
    }
    out.join("\n")
}

impl Ui for Term {
    fn event(&self, e: UiEvent) {
        match e {
            UiEvent::Reasoning(t) => {
                if !self.in_reasoning.swap(true, Ordering::Relaxed) && self.show_reasoning {
                    print!("{}", dim("  ✻ "));
                }
                self.reasoning_chars
                    .fetch_add(t.chars().count(), Ordering::Relaxed);
                if self.show_reasoning {
                    print!("{}", dim(&t));
                }
            }
            UiEvent::Text(t) => {
                self.end_reasoning();
                self.wrote_text.store(true, Ordering::Relaxed);
                print!("{t}");
            }
            UiEvent::ToolStart(_) => {
                self.end_reasoning();
                if self.wrote_text.swap(false, Ordering::Relaxed) {
                    println!();
                }
            }
            UiEvent::ToolCall { name, args } => {
                println!("{} {} {}", accent("▸"), bold(&name), dim(&args))
            }
            UiEvent::ToolResult {
                ok,
                title,
                text,
                name,
            } => {
                let mark = if ok { green("✓") } else { red("✗") };
                println!(
                    "  {mark} {}",
                    dim(&if title.is_empty() { name } else { title })
                );
                let max = if ok { 6 } else { 4 };
                if !text.trim().is_empty() {
                    println!("{}", indent_preview(&text, max));
                }
            }
            // the todowrite result already shows the list
            UiEvent::Todos(_) => {}
            UiEvent::Compacted => println!(
                "{}",
                dim("  ⟲ older context summarised to stay within the model's window")
            ),
            UiEvent::Info(s) => println!("{} {s}", yellow("!")),
            UiEvent::Usage(_) => {}
        }
        let _ = std::io::stdout().flush();
    }
}

/// Asks on the terminal. When stdin is not a terminal and `auto` is false, everything is refused.
pub struct TermGate {
    pub auto: bool,
}

#[async_trait]
impl Gate for TermGate {
    async fn ask(&self, r: &Request) -> Reply {
        if self.auto {
            return Reply::Once;
        }
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "{} refused {}: {} (non-interactive; pass --yes to allow)",
                yellow("!"),
                r.tool,
                r.target
            );
            return Reply::Reject;
        }
        let (t, tg, why) = (r.tool.clone(), r.target.clone(), r.reason.clone());
        tokio::task::spawn_blocking(move || {
            println!("\n{} {} {}", yellow("? permission"), bold(&t), tg);
            if !why.is_empty() {
                println!("  {}", dim(&why));
            }
            print!("  allow? [y]es once / [a]lways this / [n]o  ❯ ");
            let _ = std::io::stdout().flush();
            let mut s = String::new();
            if std::io::stdin().read_line(&mut s).is_err() {
                return Reply::Reject;
            }
            match s.trim().to_lowercase().as_str() {
                "y" | "yes" => Reply::Once,
                "a" | "always" => Reply::Always,
                _ => Reply::Reject,
            }
        })
        .await
        .unwrap_or(Reply::Reject)
    }
}

/// One UI event as JSON (used by `--json` and by remote control).
pub fn event_json(e: &UiEvent) -> serde_json::Value {
    match e {
        UiEvent::Text(t) => serde_json::json!({"type":"text","text":t}),
        UiEvent::Reasoning(t) => serde_json::json!({"type":"reasoning","text":t}),
        UiEvent::ToolStart(n) => serde_json::json!({"type":"tool_start","name":n}),
        UiEvent::ToolCall { name, args } => {
            serde_json::json!({"type":"tool_call","name":name,"args":args})
        }
        UiEvent::ToolResult {
            name,
            title,
            ok,
            text,
        } => {
            serde_json::json!({"type":"tool_result","name":name,"title":title,"ok":ok,"text":text})
        }
        UiEvent::Todos(t) => serde_json::json!({"type":"todos","items":t}),
        UiEvent::Compacted => serde_json::json!({"type":"compacted"}),
        UiEvent::Info(s) => serde_json::json!({"type":"info","text":s}),
        UiEvent::Usage(u) => {
            serde_json::json!({"type":"usage","prompt_tokens":u.prompt_tokens,"completion_tokens":u.completion_tokens})
        }
    }
}

/// `--json`: one event per line.
pub struct JsonUi;
impl Ui for JsonUi {
    fn event(&self, e: UiEvent) {
        println!("{}", event_json(&e));
    }
}
