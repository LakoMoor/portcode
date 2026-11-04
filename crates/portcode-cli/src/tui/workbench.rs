//! Deep-mode workbench: functions on the left, code in the middle (Pseudo-C, Disassembly, Hex), the evidence ledger
//! on the right and the agent underneath. State and key handling only; the event loop runs the tools (see `mod.rs`).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Func {
    pub addr: u64,
    pub size: u64,
    pub offset: Option<u64>,
    pub name: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tab {
    Pseudo,
    Disasm,
    Hex,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Pseudo, Tab::Disasm, Tab::Hex];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Pseudo => "Pseudo-C",
            Tab::Disasm => "Disassembly",
            Tab::Hex => "Hex",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    List,
    Filter,
    Chat,
}

#[derive(Clone, Debug)]
pub enum Body {
    Loading,
    Text(String),
    Err(String),
}

/// What the event loop should run for the workbench.
#[derive(Clone, Debug)]
pub enum Req {
    Functions {
        file: String,
    },
    Load {
        addr: u64,
        tab: Tab,
        tool: String,
        args: Value,
    },
}

/// Results the event loop sends back.
pub enum WbMsg {
    Functions(Result<String, String>),
    Body {
        addr: u64,
        tab: Tab,
        res: Result<String, String>,
    },
}

pub enum Act {
    None,
    Close,
    Chat,
    Ask(String),
}

#[derive(Clone, Debug)]
pub struct Note {
    pub id: String,
    pub kind: &'static str,
    pub text: String,
}

pub struct Wb {
    pub file: String,
    pub format: String,
    pub arch: String,
    pub funcs: Vec<Func>,
    pub status: Option<String>,
    pub filter: String,
    pub sel: usize,
    pub tab: Tab,
    pub focus: Focus,
    pub scroll: usize,
    pub bodies: HashMap<(u64, Tab), Body>,
    pub notes: Vec<Note>,
    pub req: Vec<Req>,
}

/// First file in `original/` that looks like an executable (Mach-O, ELF, PE or a fat binary).
pub fn find_binary(root: &std::path::Path) -> Option<String> {
    let mut names: Vec<_> = std::fs::read_dir(root.join("original"))
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .collect();
    names.sort_by_key(|e| e.file_name());
    for e in names {
        let mut magic = [0u8; 4];
        let ok = std::fs::File::open(e.path())
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut magic))
            .is_ok();
        let known = matches!(
            magic,
            [0x7f, b'E', b'L', b'F']
                | [0xcf, 0xfa, 0xed, 0xfe]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [b'M', b'Z', _, _]
        );
        if ok && known {
            return Some(format!("original/{}", e.file_name().to_string_lossy()));
        }
    }
    None
}

fn parse_hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

impl Wb {
    pub fn new(file: &str) -> Self {
        Self {
            file: file.into(),
            format: String::new(),
            arch: String::new(),
            funcs: vec![],
            status: Some("reading functions…".into()),
            filter: String::new(),
            sel: 0,
            tab: Tab::Pseudo,
            focus: Focus::List,
            scroll: 0,
            bodies: HashMap::new(),
            notes: vec![],
            req: vec![Req::Functions { file: file.into() }],
        }
    }

    /// Indices into `funcs` that match the filter.
    pub fn visible(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.funcs
            .iter()
            .enumerate()
            .filter(|(_, x)| {
                f.is_empty()
                    || x.name.to_lowercase().contains(&f)
                    || format!("{:x}", x.addr).contains(&f)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn current(&self) -> Option<&Func> {
        let v = self.visible();
        v.get(self.sel).and_then(|&i| self.funcs.get(i))
    }

    pub fn body(&self) -> Option<&Body> {
        let f = self.current()?;
        self.bodies.get(&(f.addr, self.tab))
    }

    pub fn on_msg(&mut self, m: WbMsg) {
        match m {
            WbMsg::Functions(Ok(text)) => {
                self.parse(&text);
                self.status = if self.funcs.is_empty() {
                    Some("no functions found (stripped binary? try the decompile tool with mode=list)".into())
                } else {
                    None
                };
                self.want();
            }
            WbMsg::Functions(Err(e)) => self.status = Some(e),
            WbMsg::Body { addr, tab, res } => {
                let b = match res {
                    Ok(t) => Body::Text(t),
                    Err(e) => Body::Err(e),
                };
                self.bodies.insert((addr, tab), b);
            }
        }
    }

    fn parse(&mut self, text: &str) {
        let mut entry = 0;
        self.funcs.clear();
        for l in text.lines() {
            if let Some(h) = l.strip_prefix("format=") {
                for w in h.split_whitespace() {
                    if let Some(v) = w.strip_prefix("arch=") {
                        self.arch = v.into();
                    } else if let Some(v) = w.strip_prefix("entry=") {
                        entry = parse_hex(v).unwrap_or(0);
                    } else if !w.contains('=') {
                        self.format = w.into();
                    }
                }
                if let Some(f) = h.split_whitespace().next() {
                    self.format = f.into();
                }
                continue;
            }
            let mut p = l.splitn(4, ' ');
            let (Some(a), Some(s), Some(o), Some(n)) = (p.next(), p.next(), p.next(), p.next())
            else {
                continue;
            };
            let Some(addr) = a.strip_prefix("0x").and_then(parse_hex) else {
                continue;
            };
            self.funcs.push(Func {
                addr,
                size: s.parse().unwrap_or(0),
                offset: o.parse().ok(),
                name: n.trim().to_string(),
            });
        }
        self.sel = self
            .funcs
            .iter()
            .position(|f| f.name == "main")
            .or_else(|| self.funcs.iter().position(|f| f.addr == entry))
            .unwrap_or(0);
    }

    /// Ask for the current function's body if it is not loaded yet.
    pub fn want(&mut self) {
        let Some(f) = self.current().cloned() else {
            return;
        };
        let key = (f.addr, self.tab);
        if self.bodies.contains_key(&key) {
            return;
        }
        let Some(off) = f.offset else {
            self.bodies.insert(
                key,
                Body::Err("this function has no file offset (not backed by the file)".into()),
            );
            return;
        };
        let len = if f.size == 0 { 256 } else { f.size.min(1024) };
        let (tool, args) = match self.tab {
            Tab::Pseudo => (
                "decompile",
                json!({"file": self.file, "mode": "func", "query": format!("0x{:x}", f.addr)}),
            ),
            Tab::Disasm => (
                "disasm",
                json!({"file": self.file, "arch": self.arch, "offset": off, "length": len, "base": f.addr, "count": 400}),
            ),
            Tab::Hex => (
                "hexdump",
                json!({"file": self.file, "mode": "hex", "offset": off, "length": len.min(512)}),
            ),
        };
        self.bodies.insert(key, Body::Loading);
        self.req.push(Req::Load {
            addr: f.addr,
            tab: self.tab,
            tool: tool.into(),
            args,
        });
    }

    fn step(&mut self, d: isize) {
        let n = self.visible().len();
        if n == 0 {
            return;
        }
        self.sel = (self.sel as isize + d).clamp(0, n as isize - 1) as usize;
        self.scroll = 0;
        self.want();
    }

    fn set_tab(&mut self, t: Tab) {
        self.tab = t;
        self.scroll = 0;
        self.want();
    }

    pub fn on_key(&mut self, k: KeyEvent) -> Act {
        if self.focus == Focus::Filter {
            match k.code {
                KeyCode::Esc | KeyCode::Enter => self.focus = Focus::List,
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.sel = 0;
                    self.want();
                }
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.filter.push(c);
                    self.sel = 0;
                    self.want();
                }
                _ => {}
            }
            return Act::None;
        }
        match k.code {
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::Char('g') | KeyCode::Home => self.step(-(self.sel as isize)),
            KeyCode::Char('G') | KeyCode::End => self.step(isize::MAX / 2),
            KeyCode::Char('1') => self.set_tab(Tab::Pseudo),
            KeyCode::Char('2') => self.set_tab(Tab::Disasm),
            KeyCode::Char('3') => self.set_tab(Tab::Hex),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('h') | KeyCode::Left => {
                let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                let fwd = matches!(k.code, KeyCode::Char('l') | KeyCode::Right);
                let n = Tab::ALL.len();
                self.set_tab(Tab::ALL[(i + if fwd { 1 } else { n - 1 }) % n]);
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(12),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(12),
            KeyCode::Char('/') => self.focus = Focus::Filter,
            KeyCode::Char('r') => {
                self.bodies.clear();
                self.status = Some("reading functions…".into());
                self.req.push(Req::Functions {
                    file: self.file.clone(),
                });
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                self.focus = Focus::Chat;
                return Act::Chat;
            }
            KeyCode::Char('a') => {
                if let Some(f) = self.current() {
                    return Act::Ask(format!(
                        "In {} look at function {} at 0x{:x}{}. Explain what it does in plain words, then record it with evidence_add (kind=inference, cite the tool and address).",
                        self.file,
                        f.name,
                        f.addr,
                        f.offset.map(|o| format!(" (file offset {o})")).unwrap_or_default()
                    ));
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => return Act::Close,
            _ => {}
        }
        Act::None
    }
}

/// Strip the reminder the tools append for the model; it is noise on screen.
pub fn clean_body(t: &str) -> String {
    t.split("\n\nRecord what this shows")
        .next()
        .unwrap_or(t)
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    fn loaded() -> Wb {
        let mut w = Wb::new("original/app");
        w.on_msg(WbMsg::Functions(Ok(
            "format=MACHO arch=arm64 entry=0x460 total=3\n0x460 992 1120 main\n0x840 0 2112 cmp\n0x900 16 - stub\n\n\nRecord what this shows".into(),
        )));
        w
    }
    #[test]
    fn parses_functions_and_selects_main() {
        let w = loaded();
        assert_eq!(w.funcs.len(), 3);
        assert_eq!(w.arch, "arm64");
        assert_eq!(w.current().map(|f| f.name.as_str()), Some("main"));
        assert_eq!(w.funcs[2].offset, None);
    }
    #[test]
    fn selecting_asks_for_each_body_once() {
        let mut w = loaded();
        let n = w.req.len();
        assert!(matches!(w.req.get(n - 1), Some(Req::Load { tool, .. }) if tool == "decompile"));
        w.on_key(key(KeyCode::Char('2')));
        assert!(matches!(w.req.last(), Some(Req::Load { tool, .. }) if tool == "disasm"));
        let n = w.req.len();
        w.on_key(key(KeyCode::Char('1')));
        assert_eq!(w.req.len(), n, "already loaded, no second request");
        w.on_key(key(KeyCode::Char('j')));
        w.on_key(key(KeyCode::Char('j')));
        assert!(
            matches!(w.body(), Some(Body::Err(_))),
            "no file offset is explained, not hidden"
        );
    }
    #[test]
    fn filter_keys_edit_the_filter_not_the_selection() {
        let mut w = loaded();
        w.on_key(key(KeyCode::Char('/')));
        w.on_key(key(KeyCode::Char('c')));
        w.on_key(key(KeyCode::Char('m')));
        w.on_key(key(KeyCode::Enter));
        assert_eq!(w.visible().len(), 1);
        assert_eq!(w.current().map(|f| f.name.as_str()), Some("cmp"));
        assert!(matches!(w.on_key(key(KeyCode::Char('a'))), Act::Ask(t) if t.contains("cmp")));
        assert!(matches!(w.on_key(key(KeyCode::Esc)), Act::Close));
    }
    #[test]
    fn tool_reminder_is_stripped() {
        assert_eq!(clean_body("a\nb\n\nRecord what this shows with x"), "a\nb");
    }
}
