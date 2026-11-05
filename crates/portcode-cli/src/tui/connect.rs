//! `/connect`: add or edit an OpenAI-compatible endpoint (Ollama, LM Studio, vLLM, OpenRouter, your own server).
//! State, keys and validation only; no rendering and no network (see `view::connect`, and `Cmd::ConnectTest` in `mod.rs`).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// (name, base_url, needs a key, one line about it)
pub const PRESETS: &[(&str, &str, bool, &str)] = &[
    (
        "ollama",
        "http://localhost:11434/v1",
        false,
        "runs on this machine",
    ),
    (
        "lmstudio",
        "http://localhost:1234/v1",
        false,
        "LM Studio local server",
    ),
    (
        "llamacpp",
        "http://localhost:8080/v1",
        false,
        "llama-server",
    ),
    (
        "vllm",
        "http://localhost:8000/v1",
        false,
        "vLLM, key optional",
    ),
    ("sglang", "http://localhost:30000/v1", false, "SGLang"),
    (
        "openrouter",
        "https://openrouter.ai/api/v1",
        true,
        "many hosted models",
    ),
    ("deepseek", "https://api.deepseek.com/v1", true, "hosted"),
    ("openai", "https://api.openai.com/v1", true, "hosted"),
    (
        "custom",
        "",
        false,
        "any server that speaks /v1/chat/completions",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Preset,
    Url,
    Key,
    Model,
    Name,
}
const ORDER: [Field; 5] = [
    Field::Preset,
    Field::Url,
    Field::Key,
    Field::Model,
    Field::Name,
];

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Idle,
    Testing,
    Ok(String),
    Fail(String),
}

pub struct Save {
    pub name: String,
    pub url: String,
    pub key: String,
    pub model: String,
}

pub enum Act {
    None,
    Cancel,
    /// Ask the event loop to contact the server: (url, key).
    Test(String, String),
    Save(Save),
}

pub struct Connect {
    pub preset: usize,
    pub focus: Field,
    pub name: String,
    pub url: String,
    pub key: String,
    pub model: String,
    /// Models the server reported on the last successful test.
    pub models: Vec<String>,
    pub status: Status,
    pub error: Option<String>,
}

impl Default for Connect {
    fn default() -> Self {
        let mut c = Self {
            preset: 0,
            focus: Field::Preset,
            name: String::new(),
            url: String::new(),
            key: String::new(),
            model: String::new(),
            models: vec![],
            status: Status::Idle,
            error: None,
        };
        c.apply_preset();
        c
    }
}

impl Connect {
    /// Start on a provider that already exists, for editing.
    pub fn for_provider(name: &str, url: &str) -> Self {
        let mut c = Self {
            preset: PRESETS
                .iter()
                .position(|p| p.0 == name)
                .unwrap_or(PRESETS.len() - 1),
            ..Self::default()
        };
        c.name = name.into();
        c.url = url.into();
        c
    }

    pub fn needs_key(&self) -> bool {
        PRESETS.get(self.preset).is_some_and(|p| p.2)
    }

    /// Re-fill name and address from the chosen preset (used by snapshots).
    pub fn paste_preset(&mut self) {
        self.apply_preset();
    }

    fn apply_preset(&mut self) {
        if let Some((n, u, _, _)) = PRESETS.get(self.preset) {
            self.url = (*u).into();
            self.name = if *n == "custom" {
                String::new()
            } else {
                (*n).into()
            };
        }
        self.models.clear();
        self.status = Status::Idle;
        self.error = None;
    }

    /// The server answered the test: remember its models and pick the first when none is chosen.
    pub fn tested(&mut self, r: Result<Vec<String>, String>) {
        match r {
            Ok(m) => {
                self.status = Status::Ok(if m.is_empty() {
                    "connected, but the server lists no models: type the model name".into()
                } else {
                    format!(
                        "connected · {} model{}",
                        m.len(),
                        if m.len() == 1 { "" } else { "s" }
                    )
                });
                if self.model.is_empty() {
                    if let Some(first) = m.first() {
                        self.model = first.clone();
                    }
                }
                self.models = m;
            }
            Err(e) => {
                self.models.clear();
                self.status = Status::Fail(e);
            }
        }
    }

    fn text(&mut self) -> Option<&mut String> {
        match self.focus {
            Field::Url => Some(&mut self.url),
            Field::Key => Some(&mut self.key),
            Field::Model => Some(&mut self.model),
            Field::Name => Some(&mut self.name),
            Field::Preset => None,
        }
    }

    pub fn paste(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        if let Some(t) = self.text() {
            t.push_str(s.trim());
            self.status = if self.status == Status::Testing {
                Status::Testing
            } else {
                Status::Idle
            };
        }
    }

    fn step(&mut self, d: isize) {
        let i = ORDER.iter().position(|f| *f == self.focus).unwrap_or(0) as isize;
        self.focus = ORDER[(i + d).rem_euclid(ORDER.len() as isize) as usize];
    }

    /// Why the form cannot be saved yet, in words the user can act on.
    pub fn problem(&self) -> Option<String> {
        let url = self.url.trim();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Some("The address must start with http:// or https:// (for example http://localhost:11434/v1).".into());
        }
        if !portcode_agent::config::valid_provider_name(self.name.trim()) {
            return Some("Give it a short name: letters, digits, - and _ only.".into());
        }
        if self.model.trim().is_empty() {
            return Some(
                "Choose or type a model. Press ctrl+t to ask the server which ones it has.".into(),
            );
        }
        None
    }

    pub fn on_key(&mut self, k: KeyEvent) -> Act {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        self.error = None;
        match k.code {
            KeyCode::Esc => return Act::Cancel,
            KeyCode::Char('c') if ctrl => return Act::Cancel,
            KeyCode::Char('t') if ctrl => return self.test(),
            KeyCode::Char('s') if ctrl => return self.save(),
            KeyCode::Char('u') if ctrl => {
                if let Some(t) = self.text() {
                    t.clear();
                }
            }
            KeyCode::Tab => self.step(1),
            KeyCode::BackTab => self.step(-1),
            KeyCode::Down if self.focus != Field::Model => self.step(1),
            KeyCode::Up if self.focus != Field::Model => self.step(-1),
            KeyCode::Enter => match self.focus {
                Field::Key => {
                    self.step(1);
                    return self.test();
                }
                Field::Name => return self.save(),
                Field::Model if self.problem().is_none() => return self.save(),
                _ => self.step(1),
            },
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                let fwd = matches!(k.code, KeyCode::Right | KeyCode::Down);
                match self.focus {
                    Field::Preset => {
                        let n = PRESETS.len() as isize;
                        self.preset = (self.preset as isize + if fwd { 1 } else { -1 })
                            .rem_euclid(n) as usize;
                        self.apply_preset();
                    }
                    Field::Model if !self.models.is_empty() => {
                        let n = self.models.len() as isize;
                        let cur = self
                            .models
                            .iter()
                            .position(|m| *m == self.model)
                            .map_or(-1, |p| p as isize);
                        let next = if cur < 0 {
                            0
                        } else {
                            (cur + if fwd { 1 } else { -1 }).rem_euclid(n)
                        };
                        if let Some(m) = self.models.get(next as usize) {
                            self.model = m.clone();
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Backspace => {
                if let Some(t) = self.text() {
                    t.pop();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(t) = self.text() {
                    t.push(c);
                    if self.focus != Field::Model {
                        self.status = Status::Idle;
                    }
                }
            }
            _ => {}
        }
        Act::None
    }

    fn test(&mut self) -> Act {
        let url = self.url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            self.error = Some("Fill in the address first (http://… or https://…).".into());
            return Act::None;
        }
        self.status = Status::Testing;
        Act::Test(url, self.key.trim().to_string())
    }

    fn save(&mut self) -> Act {
        if let Some(p) = self.problem() {
            self.error = Some(p);
            return Act::None;
        }
        Act::Save(Save {
            name: self.name.trim().to_string(),
            url: self.url.trim().trim_end_matches('/').to_string(),
            key: self.key.trim().to_string(),
            model: self.model.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }
    fn typed(c: &mut Connect, s: &str) {
        for ch in s.chars() {
            c.on_key(key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn preset_fills_name_and_address() {
        let mut c = Connect::default();
        assert_eq!(
            (c.name.as_str(), c.url.as_str()),
            ("ollama", "http://localhost:11434/v1")
        );
        c.on_key(key(KeyCode::Right));
        assert_eq!(c.name, "lmstudio");
        for _ in 0..7 {
            c.on_key(key(KeyCode::Right));
        }
        assert_eq!((c.name.as_str(), c.url.as_str()), ("", ""));
    }

    #[test]
    fn save_needs_address_name_and_model() {
        let mut c = Connect::default();
        assert!(matches!(
            c.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Act::None
        ));
        assert!(c.error.as_deref().unwrap_or("").contains("model"));
        c.focus = Field::Model;
        typed(&mut c, "qwen3:14b");
        match c.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)) {
            Act::Save(s) => {
                assert_eq!((s.name.as_str(), s.model.as_str()), ("ollama", "qwen3:14b"))
            }
            _ => panic!("expected save"),
        }
    }

    #[test]
    fn a_bad_address_is_refused_with_advice() {
        let mut c = Connect {
            focus: Field::Url,
            ..Connect::default()
        };
        c.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        typed(&mut c, "localhost:1234");
        assert!(matches!(
            c.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
            Act::None
        ));
        assert!(c.error.as_deref().unwrap_or("").contains("http"));
    }

    #[test]
    fn enter_on_the_key_tests_and_a_result_picks_a_model() {
        let mut c = Connect {
            focus: Field::Key,
            ..Connect::default()
        };
        c.paste("  sk-abc\n");
        assert_eq!(c.key, "sk-abc");
        assert!(matches!(c.on_key(key(KeyCode::Enter)), Act::Test(_, k) if k == "sk-abc"));
        assert_eq!(c.focus, Field::Model);
        c.tested(Ok(vec!["a".into(), "b".into()]));
        assert_eq!(c.model, "a");
        c.on_key(key(KeyCode::Down));
        assert_eq!(c.model, "b");
        c.tested(Err("refused".into()));
        assert!(matches!(c.status, Status::Fail(_)) && c.models.is_empty());
    }
}
