//! `/settings`: one screen for the switches that otherwise live behind separate slash commands.
//! State, rows and keys only; the drawing is `view::settings`. Every change goes through the same slash command
//! a user would type (`/venv off`, `/theme light`…), so the two ways can never disagree.
use super::app::App;
use crossterm::event::{KeyCode, KeyEvent};

pub const CONTEXTS: [&str; 7] = ["auto", "8k", "16k", "32k", "64k", "128k", "256k"];
pub const STYLES: [&str; 4] = ["auto", "compact", "full", "qwen38"];
pub const EFFORTS: [&str; 4] = ["xhigh", "medium", "low", "none"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Id {
    Model,
    Context,
    Mode,
    AllowAll,
    Team,
    Venv,
    Theme,
    Reasoning,
    Sidebar,
    Style,
    Effort,
    Update,
}

pub const ORDER: [Id; 12] = [
    Id::Model,
    Id::Context,
    Id::Mode,
    Id::AllowAll,
    Id::Team,
    Id::Venv,
    Id::Theme,
    Id::Reasoning,
    Id::Sidebar,
    Id::Style,
    Id::Effort,
    Id::Update,
];

/// What the screen remembers; the live values are read from `App` each time it is drawn.
#[derive(Clone, Debug)]
pub struct Settings {
    pub sel: usize,
    /// `/venv` has no getter on `App`, so the screen tracks it (starts from the config).
    pub venv: bool,
    pub style: usize,
    /// None until the user picks one: the model's own default is in force.
    pub effort: Option<usize>,
}

pub enum Value {
    /// A switch: on or off.
    Toggle(bool),
    /// One of several; left/right cycle.
    Choice(String),
    /// A line of text that Enter acts on.
    Action(String),
}

pub struct Row {
    pub group: &'static str,
    pub label: &'static str,
    pub value: Value,
    pub hint: &'static str,
}

impl Settings {
    pub fn new(cfg: &portcode_agent::config::Config) -> Self {
        let style = cfg
            .prompt_style
            .as_deref()
            .and_then(|s| STYLES.iter().position(|x| *x == s))
            .unwrap_or(0);
        Settings {
            sel: 0,
            venv: cfg.venv.unwrap_or(true),
            style,
            effort: None,
        }
    }
}

fn cycle(list: &[&str], cur: usize, dir: isize) -> usize {
    (cur as isize + dir).rem_euclid(list.len() as isize) as usize
}

/// The preset closest to the context window in use, so left/right step from where the model really is.
fn context_index(tokens: u32) -> usize {
    let at = |s: &str| portcode_agent::config::parse_size(s).unwrap_or(0);
    (1..CONTEXTS.len())
        .min_by_key(|i| at(CONTEXTS[*i]).abs_diff(tokens))
        .unwrap_or(0)
}

impl App {
    pub fn open_settings(&mut self) {
        self.settings = Some(Settings::new(&self.cfg));
    }

    pub fn settings_rows(&self) -> Vec<Row> {
        let Some(s) = self.settings.as_ref() else {
            return Vec::new();
        };
        let tokens = if self.ctx_window >= 1000 {
            format!("{}k tokens", self.ctx_window / 1000)
        } else {
            format!("{} tokens", self.ctx_window)
        };
        ORDER
            .iter()
            .map(|id| match id {
                Id::Model => Row {
                    group: "Model",
                    label: "Model",
                    value: Value::Action(format!("{}/{}", self.provider, self.model)),
                    hint: "enter lists the models of this provider; /model <provider/model> switches",
                },
                Id::Context => Row {
                    group: "Model",
                    label: "Context window",
                    value: Value::Choice(tokens.clone()),
                    hint: "left/right set it for this model. Set the same size in the server too (Ollama num_ctx)",
                },
                Id::Mode => Row {
                    group: "Work",
                    label: "Look inside the original",
                    value: Value::Toggle(self.deep),
                    hint: "on: deep mode (disassemble, decompile, emulate). off: clean-room, observe from outside only",
                },
                Id::AllowAll => Row {
                    group: "Work",
                    label: "Allow everything",
                    value: Value::Toggle(self.auto_allow),
                    hint: "approve every permission request without asking. Only on a machine and project you trust",
                },
                Id::Team => Row {
                    group: "Work",
                    label: "Agent team",
                    value: Value::Toggle(self.agents.iter().any(|a| a == "team")),
                    hint: "adds the team agent: a PM, tech lead and workers in parallel (tab switches to it)",
                },
                Id::Venv => Row {
                    group: "Work",
                    label: "Python virtual env",
                    value: Value::Toggle(s.venv),
                    hint: "lets the agent create and use a Python .venv in the project",
                },
                Id::Theme => Row {
                    group: "Look",
                    label: "Light theme",
                    value: Value::Toggle(super::theme::is_light()),
                    hint: "remembered for the next start",
                },
                Id::Reasoning => Row {
                    group: "Look",
                    label: "Show reasoning",
                    value: Value::Toggle(self.show_reasoning),
                    hint: "the model's thinking, dimmed above its answer (ctrl+t)",
                },
                Id::Sidebar => Row {
                    group: "Look",
                    label: "Sidebar",
                    value: Value::Toggle(self.sidebar),
                    hint: "todos, files and the evidence ledger beside the chat (ctrl+b)",
                },
                Id::Style => Row {
                    group: "Model tuning",
                    label: "Prompt style",
                    value: Value::Choice(STYLES[s.style].into()),
                    hint: "auto picks one from the model id; compact suits small models",
                },
                Id::Effort => Row {
                    group: "Model tuning",
                    label: "Thinking effort",
                    value: Value::Choice(
                        s.effort
                            .map_or("model default".to_string(), |i| EFFORTS[i].to_string()),
                    ),
                    hint: "Qwen3.8-style templates only; none turns thinking off",
                },
                Id::Update => Row {
                    group: "PortCode",
                    label: "Updates",
                    value: Value::Action(format!(
                        "v{} · checks every minute · enter checks now",
                        crate::update::current()
                    )),
                    hint: "a notice appears here when a newer release is out; /update installs it",
                },
            })
            .collect()
    }

    /// Change row `id`; `dir` is -1/+1 for left/right and 0 for enter or space.
    fn settings_apply(&mut self, id: Id, dir: isize) {
        let Some(s) = self.settings.clone() else {
            return;
        };
        let flip = |on: bool| if on { "off" } else { "on" };
        match id {
            Id::Model => {
                if dir == 0 {
                    self.settings = None;
                    self.run_slash("models");
                }
            }
            Id::Context => {
                if dir != 0 {
                    let i = cycle(&CONTEXTS, context_index(self.ctx_window), dir);
                    self.run_slash(&format!("context {}", CONTEXTS[i]));
                }
            }
            Id::Mode => self.run_slash(if self.deep { "clean" } else { "deep" }),
            Id::AllowAll => self.run_slash(&format!("yolo {}", flip(self.auto_allow))),
            Id::Team => {
                let on = self.agents.iter().any(|a| a == "team");
                self.run_slash(&format!("team {}", flip(on)));
            }
            Id::Venv => {
                self.run_slash(&format!("venv {}", flip(s.venv)));
                if let Some(m) = self.settings.as_mut() {
                    m.venv = !s.venv;
                }
            }
            Id::Theme => self.run_slash("theme"),
            Id::Reasoning => self.show_reasoning = !self.show_reasoning,
            Id::Sidebar => self.sidebar = !self.sidebar,
            Id::Style => {
                if dir != 0 {
                    let i = cycle(&STYLES, s.style, dir);
                    self.run_slash(&format!("style {}", STYLES[i]));
                    if let Some(m) = self.settings.as_mut() {
                        m.style = i;
                    }
                }
            }
            Id::Effort => {
                if dir != 0 {
                    let i = s
                        .effort
                        .map_or(if dir > 0 { 0 } else { EFFORTS.len() - 1 }, |e| {
                            cycle(&EFFORTS, e, dir)
                        });
                    self.run_slash(&format!("effort {}", EFFORTS[i]));
                    if let Some(m) = self.settings.as_mut() {
                        m.effort = Some(i);
                    }
                }
            }
            Id::Update => {
                if dir == 0 {
                    self.run_slash("update");
                }
            }
        }
    }

    /// Keys while the settings screen is open.
    pub fn settings_key(&mut self, k: KeyEvent) {
        let n = ORDER.len();
        let Some(sel) = self.settings.as_ref().map(|s| s.sel) else {
            return;
        };
        let step = |me: &mut App, to: usize| {
            if let Some(s) = me.settings.as_mut() {
                s.sel = to;
            }
        };
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.settings = None,
            KeyCode::Up | KeyCode::Char('k') => step(self, (sel + n - 1) % n),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => step(self, (sel + 1) % n),
            KeyCode::BackTab => step(self, (sel + n - 1) % n),
            KeyCode::Home => step(self, 0),
            KeyCode::End => step(self, n - 1),
            KeyCode::Left | KeyCode::Char('h') => self.settings_apply(ORDER[sel], -1),
            KeyCode::Right | KeyCode::Char('l') => self.settings_apply(ORDER[sel], 1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let id = ORDER[sel];
                // a choice moves forward on enter, like right
                let dir = if matches!(id, Id::Context | Id::Style | Id::Effort) {
                    1
                } else {
                    0
                };
                self.settings_apply(id, dir);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_both_ways() {
        assert_eq!(cycle(&STYLES, 0, -1), 3);
        assert_eq!(cycle(&STYLES, 3, 1), 0);
        assert_eq!(cycle(&STYLES, 1, 1), 2);
    }

    #[test]
    fn context_index_picks_the_nearest_preset() {
        assert_eq!(CONTEXTS[context_index(32_768)], "32k");
        assert_eq!(CONTEXTS[context_index(131_072)], "128k");
        assert_eq!(CONTEXTS[context_index(40_000)], "32k");
        // never "auto": that is not a size
        assert_ne!(context_index(1), 0);
    }
}
