//! The agent loop: model call → tool calls → results → repeat, with step budget, loop guard,
//! context pruning/compaction, cancellation and sub-agents.
use crate::agents::{self, AgentDef};
use crate::config::{Config, Resolved};
use crate::permission::{Gate, Reply, Request, Rules};
use crate::session::Store;
use crate::skills::{self, Skill};
use crate::tool::*;
use async_trait::async_trait;
use portcode_llm::{Client, Message, Role, StreamEvent, ToolDef, Usage};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum UiEvent {
    Text(String),
    Reasoning(String),
    ToolStart(String),
    ToolCall {
        name: String,
        args: String,
    },
    ToolResult {
        name: String,
        title: String,
        ok: bool,
        text: String,
    },
    Todos(Vec<Todo>),
    Compacted,
    Info(String),
    Usage(Usage),
}
pub trait Ui: Send + Sync {
    fn event(&self, e: UiEvent);
}
pub struct NullUi;
impl Ui for NullUi {
    fn event(&self, _: UiEvent) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    MaxSteps,
    Cancelled,
    Stuck,
}
#[derive(Debug, Clone)]
pub struct Outcome {
    pub text: String,
    pub steps: u32,
    pub usage: Usage,
    pub status: Status,
}

/// Wraps a gate so that anything that would prompt is refused (read-only and non-interactive runs).
pub struct DenyAsk;
#[async_trait]
impl Gate for DenyAsk {
    async fn ask(&self, _: &Request) -> Reply {
        Reply::Reject
    }
}

pub struct Runner {
    client: Client,
    pub resolved: Resolved,
    pub cfg: Config,
    pub ctx: Arc<Ctx>,
    registry: Registry,
    pub agent: AgentDef,
    messages: Vec<Message>,
    ui: Arc<dyn Ui>,
    skills: Vec<Skill>,
    store: Option<Store>,
    pub session_id: String,
    pub usage: Usage,
    last_prompt_tokens: u64,
}

fn err_text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Runner {
    pub async fn new(
        root: std::path::PathBuf,
        cfg: Config,
        model: Option<&str>,
        agent: &str,
        gate: Arc<dyn Gate>,
        ui: Arc<dyn Ui>,
    ) -> Result<Self, String> {
        Self::build(root, cfg, model, agent, gate, ui, 0, true).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn build(
        root: std::path::PathBuf,
        cfg: Config,
        model: Option<&str>,
        agent: &str,
        gate: Arc<dyn Gate>,
        ui: Arc<dyn Ui>,
        depth: u8,
        persist: bool,
    ) -> Result<Self, String> {
        let agent_def = agents::find(agent).ok_or_else(|| {
            format!(
                "unknown agent '{agent}'. Available: {}",
                agents::all()
                    .iter()
                    .map(|a| a.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let model_ref = model
            .map(String::from)
            .or_else(|| cfg.agent.get(agent).and_then(|a| a.model.clone()));
        let resolved = cfg.resolve(model_ref.as_deref())?;
        let client = Client::new(resolved.provider.clone()).map_err(err_text)?;
        let skills = skills::discover(&root);
        let mut registry = crate::tools::builtin(skills.clone());
        // external MCP servers
        for (name, e) in &cfg.mcp {
            if e.enabled == Some(false) || !agent_def.mcp {
                continue;
            }
            match crate::mcp::McpServer::start(name, e, &root).await {
                Ok(s) => match s.tools().await {
                    Ok(ts) => {
                        for t in ts {
                            registry.add(t);
                        }
                    }
                    Err(er) => ui.event(UiEvent::Info(format!("MCP '{name}': {er}"))),
                },
                Err(er) => ui.event(UiEvent::Info(er)),
            }
        }
        let mut ctx = Ctx::new(
            root.clone(),
            gate.clone(),
            Rules::from_config(&cfg.permission),
        );
        ctx.readonly = agent_def.readonly.into();
        ctx.deep = (cfg.mode.as_deref() == Some("deep")).into();
        ctx.output_limit = resolved.profile.tool_output_limit;
        ctx.allowed_dirs = cfg
            .allowed_dirs
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        ctx.scrub_env = cfg
            .provider
            .values()
            .filter_map(|p| p.api_key_env.clone())
            .chain(crate::config::all_key_envs())
            .collect();
        let ctx = Arc::new(ctx);
        if depth == 0 {
            *lock(&ctx.sub) = Some(Arc::new(SubAgents {
                cfg: cfg.clone(),
                root: root.clone(),
                gate: gate.clone(),
                model: model_ref.clone(),
            }));
        }
        let store = persist.then(|| Store::new(&root));
        Ok(Self {
            client,
            resolved,
            cfg,
            ctx,
            registry,
            agent: agent_def,
            messages: vec![],
            ui,
            skills,
            store,
            session_id: Store::new_id(),
            usage: Usage::default(),
            last_prompt_tokens: 0,
        })
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
    /// Remember what the last tool calls mean for "is it finished".
    fn track(&self, name: &str, result: &str) {
        let mut st = lock(&self.ctx.state);
        match name {
            "parity" => {
                st.parity = Some(result.contains("RESULT: PASS"));
                st.dirty = false;
            }
            "write" | "edit" | "scaffold" if !result.starts_with("Error") => st.dirty = true,
            _ => {}
        }
    }

    /// What still stands between a gated agent (the `port` agent) and "done"; empty = finished.
    /// 1:1 means every behaviour is on the todo list, every todo is done, and the last parity run passed on the final code.
    pub fn gaps(&self) -> Vec<String> {
        if !self.agent.gate {
            return vec![];
        }
        let st = lock(&self.ctx.state);
        let mut g = Vec::new();
        if st.todos.is_empty() {
            g.push("no todo list: call todowrite with one item per behaviour of the original (options, outputs, exit codes, errors) so nothing is skipped".to_string());
        }
        for t in st.todos.iter().filter(|t| t.status != "completed") {
            g.push(format!("todo not done: {}", t.content));
        }
        match st.parity {
            None => g.push(
                "parity has not been run: capture goldens from the ORIGINAL and run `parity`"
                    .into(),
            ),
            Some(false) => g.push(
                "the last parity run FAILED: fix the port (never the goldens) and run parity again"
                    .into(),
            ),
            Some(true) if st.dirty => {
                g.push("code changed after the last parity run: run parity again".into())
            }
            Some(true) => {}
        }
        g
    }

    pub fn todos(&self) -> Vec<Todo> {
        lock(&self.ctx.state).todos.clone()
    }
    pub fn cancel_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.ctx.cancel.clone()
    }
    pub fn tool_names(&self) -> Vec<String> {
        self.registry.names()
    }
    pub fn clear(&mut self) {
        self.messages.clear();
        self.session_id = Store::new_id();
    }

    pub fn resume(&mut self, id: &str) -> Result<usize, String> {
        let st = self.store.as_ref().ok_or("sessions disabled")?;
        self.messages = st.load(id)?;
        self.session_id = id.to_string();
        Ok(self.messages.len())
    }
    pub fn latest_session(&self) -> Option<String> {
        self.store.as_ref().and_then(|s| s.latest())
    }

    pub fn set_agent(&mut self, name: &str) -> Result<(), String> {
        let a = agents::find(name)
            .filter(|a| a.primary)
            .ok_or_else(|| format!("unknown primary agent '{name}'"))?;
        self.ctx.readonly.store(a.readonly, Ordering::Relaxed);
        self.agent = a;
        Ok(())
    }

    pub fn set_model(&mut self, reference: &str) -> Result<(), String> {
        let r = self.cfg.resolve(Some(reference))?;
        self.client = Client::new(r.provider.clone()).map_err(err_text)?;
        self.resolved = r;
        Ok(())
    }

    fn system(&self) -> Message {
        Message::system(agents::system_prompt(
            &self.agent,
            &self.resolved.profile,
            &self.ctx.root,
            &self.skills,
            self.ctx.is_deep(),
            portcode_evidence::now_secs(),
        ))
    }

    fn defs(&self) -> Vec<ToolDef> {
        let names = BUILTIN_NAMES;
        let rules = lock(&self.ctx.rules).clone();
        self.registry
            .defs(&|n| self.agent.allows(n, names.contains(&n)) && !rules.tool_denied(n))
    }

    fn est_tokens(&self, sys: &Message) -> u64 {
        let chars: usize = std::iter::once(sys)
            .chain(self.messages.iter())
            .map(|m| {
                m.content.as_deref().map(str::len).unwrap_or(0)
                    + m.tool_calls
                        .iter()
                        .map(|c| c.function.arguments.len() + 20)
                        .sum::<usize>()
            })
            .sum();
        (chars as f64 / 3.3) as u64
    }

    /// Shrink old tool output first (cheap), then summarise older turns with the model if still too big.
    async fn fit_context(&mut self) {
        let window = u64::from(self.resolved.profile.context_window);
        let sys = self.system();
        let over = |s: &Self, sys: &Message| {
            s.est_tokens(sys).max(s.last_prompt_tokens) * 100 > window * 75
        };
        if !over(self, &sys) {
            return;
        }
        let keep_from = self.messages.len().saturating_sub(10);
        for m in self.messages.iter_mut().take(keep_from) {
            if m.role == Role::Tool {
                if let Some(c) = &m.content {
                    if c.len() > 1200 {
                        m.content = Some(format!(
                            "{}\n[older tool output trimmed]",
                            c.chars().take(500).collect::<String>()
                        ));
                    }
                }
            }
        }
        self.last_prompt_tokens = 0;
        if !over(self, &sys) {
            return;
        }
        // summarise everything before the last user message
        let Some(cut) = self
            .messages
            .iter()
            .rposition(|m| m.role == Role::User)
            .filter(|i| *i > 1)
        else {
            return;
        };
        let transcript: String = self
            .messages
            .iter()
            .take(cut)
            .map(|m| {
                format!(
                    "{:?}: {}{}",
                    m.role,
                    m.content
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(700)
                        .collect::<String>(),
                    if m.tool_calls.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " [calls: {}]",
                            m.tool_calls
                                .iter()
                                .map(|c| c.function.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let ask = vec![Message::system("You compress a coding session so the work can continue. Write a dense summary (max 350 words) with: the user's goal, decisions made, files changed, commands run and their results, evidence ids (E-xxxx), open problems, and the exact next step. No preamble."), Message::user(transcript)];
        let mut prof = self.resolved.profile.clone();
        prof.max_tokens = 800;
        prof.temperature = Some(0.1);
        let model = self
            .cfg
            .small_model
            .as_deref()
            .and_then(|m| self.cfg.resolve(Some(m)).ok());
        let res = match &model {
            Some(r) => match Client::new(r.provider.clone()) {
                Ok(c) => c.chat(&r.model, &r.profile, &ask, &[], &mut |_| {}).await,
                Err(e) => Err(e),
            },
            None => {
                self.client
                    .chat(&self.resolved.model, &prof, &ask, &[], &mut |_| {})
                    .await
            }
        };
        match res {
            Ok(a) if !a.text.trim().is_empty() => {
                let tail = self.messages.split_off(cut);
                self.messages = vec![Message::user(format!(
                    "[Summary of the earlier conversation]\n{}",
                    a.text.trim()
                ))];
                self.messages.extend(tail);
                self.ui.event(UiEvent::Compacted);
            }
            _ => self.ui.event(UiEvent::Info(
                "context is large and compaction failed; continuing".into(),
            )),
        }
    }

    fn save(&self) {
        if let Some(s) = &self.store {
            let _ = s.save(&self.session_id, &self.messages);
        }
    }

    /// Run one user turn to completion.
    pub async fn send(&mut self, text: &str) -> Result<Outcome, String> {
        self.messages.push(Message::user(text));
        self.ctx.cancel.store(false, Ordering::Relaxed);
        let base_steps = self
            .cfg
            .agent
            .get(self.agent.name)
            .and_then(|a| a.steps)
            .or(self.cfg.max_steps)
            .unwrap_or(self.agent.steps)
            .max(2);
        let mut limit = base_steps;
        let (mut extensions, mut gate_nudges) = (0u32, 0u32);
        lock(&self.ctx.state).blocked = None;
        let (mut recent, mut repeats, mut nudges) = (Vec::<String>::new(), 0u32, 0u32);
        let mut total = Usage::default();
        let mut last_text = String::new();
        for step in 0..base_steps * 4 {
            if step >= limit {
                break;
            }
            if self.ctx.cancelled() {
                self.save();
                return Ok(Outcome {
                    text: last_text,
                    steps: step,
                    usage: total,
                    status: Status::Cancelled,
                });
            }
            self.fit_context().await;
            if step + 1 == limit && extensions < 2 && !self.gaps().is_empty() {
                // out of steps but visibly unfinished: grant more instead of stopping short
                extensions += 1;
                limit += (base_steps / 2).max(10);
                self.ui.event(UiEvent::Info(format!(
                    "not finished after {step} steps; continuing ({} open)",
                    self.gaps().len()
                )));
            }
            let final_step = step + 1 == limit;
            let tools = if final_step { vec![] } else { self.defs() };
            let mut msgs = vec![self.system()];
            msgs.extend(self.messages.iter().cloned());
            if final_step {
                msgs.push(Message::user("Step limit reached. Do not call tools. Summarise what is done, what is not, and the next step."));
            }
            let ui = self.ui.clone();
            let res = self
                .client
                .chat(
                    &self.resolved.model,
                    &self.resolved.profile,
                    &msgs,
                    &tools,
                    &mut |e| match e {
                        StreamEvent::Text(t) => ui.event(UiEvent::Text(t)),
                        StreamEvent::Reasoning(t) => ui.event(UiEvent::Reasoning(t)),
                        StreamEvent::ToolStart(n) => ui.event(UiEvent::ToolStart(n)),
                    },
                )
                .await;
            let a = match res {
                Ok(a) => a,
                Err(e) => {
                    self.save();
                    return Err(e.to_string());
                }
            };
            if let Some(u) = a.usage {
                total.prompt_tokens += u.prompt_tokens;
                total.completion_tokens += u.completion_tokens;
                self.usage.prompt_tokens += u.prompt_tokens;
                self.usage.completion_tokens += u.completion_tokens;
                self.last_prompt_tokens = u.prompt_tokens;
                self.ui.event(UiEvent::Usage(u));
            }
            let (calls, finish, reply) = (
                a.tool_calls.clone(),
                a.finish_reason.clone(),
                a.text.clone(),
            );
            if !reply.is_empty() {
                last_text = reply.clone();
            }
            self.messages.push(a.into_message());
            if calls.is_empty() {
                let empty = reply.trim().is_empty();
                if (empty || finish.as_deref() == Some("length")) && nudges < 2 && !final_step {
                    nudges += 1;
                    self.messages.push(Message::user(if empty {
                        "Your reply was empty. Continue: call a tool, or give the final answer."
                    } else {
                        "Your reply was cut off. Continue briefly from where you stopped."
                    }));
                    continue;
                }
                if self.agent.gate && !final_step {
                    let gaps = self.gaps();
                    if !gaps.is_empty() {
                        if gate_nudges < 6 {
                            gate_nudges += 1;
                            self.ui.event(UiEvent::Info(format!(
                                "not finished: {} ({} open)",
                                gaps.first().cloned().unwrap_or_default(),
                                gaps.len()
                            )));
                            self.messages.push(Message::user(format!(
                                "Not finished. Open items:\n- {}\nDo not summarise yet. Continue with a tool call. If only the user can unblock you, call `blocked` with the reason.",
                                gaps.join("\n- ")
                            )));
                            continue;
                        }
                        self.ui.event(UiEvent::Info(format!(
                            "stopped with {} open item(s): {}",
                            gaps.len(),
                            gaps.join("; ")
                        )));
                    }
                }
                self.save();
                return Ok(Outcome {
                    text: last_text,
                    steps: step + 1,
                    usage: total,
                    status: Status::Done,
                });
            }
            for call in calls {
                let sig = format!("{}:{}", call.function.name, call.function.arguments);
                recent.push(sig.clone());
                let same = recent.iter().rev().take(3).filter(|s| **s == sig).count();
                let result = if same >= 3 {
                    repeats += 1;
                    "You have made this exact call 3 times in a row. Stop repeating it. Try a different approach, or tell the user what blocks you.".to_string()
                } else {
                    self.exec(&call.function.name, &call.function.arguments)
                        .await
                };
                self.track(&call.function.name, &result);
                self.messages.push(Message::tool(call.id.clone(), result));
                if repeats >= 2 {
                    self.save();
                    return Ok(Outcome {
                        text: "Stopped: the model kept repeating the same tool call.".into(),
                        steps: step + 1,
                        usage: total,
                        status: Status::Stuck,
                    });
                }
            }
            let todos = self.todos();
            if !todos.is_empty() {
                self.ui.event(UiEvent::Todos(todos));
            }
            if let Some(why) = lock(&self.ctx.state).blocked.clone() {
                self.save();
                return Ok(Outcome {
                    text: format!("Blocked, needs you: {why}"),
                    steps: step + 1,
                    usage: total,
                    status: Status::Done,
                });
            }
            self.save();
        }
        self.save();
        Ok(Outcome {
            text: last_text,
            steps: limit,
            usage: total,
            status: Status::MaxSteps,
        })
    }

    /// Execute one tool call. Never panics: the tool runs in its own task; every failure becomes text for the model.
    async fn exec(&self, name: &str, raw_args: &str) -> String {
        let Some(tool) = self.registry.get(name).filter(|_| {
            let rules = lock(&self.ctx.rules).clone();
            self.agent.allows(name, BUILTIN_NAMES.contains(&name)) && !rules.tool_denied(name)
        }) else {
            let avail: Vec<String> = self.defs().into_iter().map(|d| d.function.name).collect();
            return format!(
                "Error: unknown tool '{name}'. Available tools: {}",
                avail.join(", ")
            );
        };
        let args: Value =
            match portcode_llm::repair_json(raw_args).and_then(|j| serde_json::from_str(&j).ok()) {
                Some(v @ Value::Object(_)) => v,
                _ => {
                    return format!(
                        "Error: arguments for '{name}' must be a JSON object. Example: {}",
                        tool.example()
                    )
                }
            };
        let preview: String = args.to_string().chars().take(160).collect();
        self.ui.event(UiEvent::ToolCall {
            name: name.into(),
            args: preview,
        });
        if let Some(missing) = tool
            .schema()
            .get("required")
            .and_then(Value::as_array)
            .and_then(|r| {
                r.iter()
                    .filter_map(Value::as_str)
                    .find(|k| args.get(*k).map(Value::is_null).unwrap_or(true))
            })
        {
            let m = format!(
                "Error: '{name}' is missing required parameter '{missing}'. Example: {}",
                tool.example()
            );
            self.ui.event(UiEvent::ToolResult {
                name: name.into(),
                title: String::new(),
                ok: false,
                text: m.clone(),
            });
            return m;
        }
        let ctx = self.ctx.clone();
        let t2 = tool.clone();
        let joined = tokio::spawn(async move { t2.run(&ctx, args).await }).await;
        let limit = self.resolved.profile.tool_output_limit;
        let (ok, title, text) = match joined {
            Ok(Ok(o)) => (true, o.title, clip(&o.text, limit)),
            Ok(Err(e)) => (false, String::new(), format!("Error: {}", clip(&e.0, limit))),
            Err(_) => (false, String::new(), format!("Error: tool '{name}' crashed internally. This is a PortCode bug; try a different approach.")),
        };
        self.ui.event(UiEvent::ToolResult {
            name: name.into(),
            title,
            ok,
            text: text.clone(),
        });
        text
    }
}

const BUILTIN_NAMES: &[&str] = &[
    "read",
    "ls",
    "glob",
    "grep",
    "write",
    "edit",
    "bash",
    "todowrite",
    "webfetch",
    "skill",
    "task",
    "survey",
    "pin",
    "evidence_add",
    "evidence_list",
    "parity",
    "screenshot",
    "site_build",
    "scaffold",
    "blocked",
    "re_status",
    "re_install",
    "disasm",
    "emulate",
    "decompile",
    "functions",
    "hexdump",
];

struct SubAgents {
    cfg: Config,
    root: std::path::PathBuf,
    gate: Arc<dyn Gate>,
    model: Option<String>,
}

#[async_trait]
impl SubRunner for SubAgents {
    async fn run(&self, agent: &str, prompt: &str) -> Result<String, String> {
        let def = agents::find(agent)
            .filter(|a| !a.primary)
            .ok_or_else(|| "agent must be 'explore' or 'general'".to_string())?;
        let gate: Arc<dyn Gate> = if def.readonly {
            Arc::new(DenyAsk)
        } else {
            self.gate.clone()
        };
        let mut r = Runner::build(
            self.root.clone(),
            self.cfg.clone(),
            self.model.as_deref(),
            agent,
            gate,
            Arc::new(NullUi),
            1,
            false,
        )
        .await?;
        let out = Box::pin(r.send(prompt)).await?;
        Ok(if out.text.trim().is_empty() {
            format!(
                "(sub-agent finished with status {:?} and no text)",
                out.status
            )
        } else {
            out.text
        })
    }
}
