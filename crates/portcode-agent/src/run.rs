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
    /// Session id of the subscription CLI (`claude-code` / `codex`), kept so the next turn resumes it.
    cli_session: Option<String>,
    /// `provider/model` whose context window was already asked of the server.
    probed: Option<String>,
}

const RECONNECT_TRIES: u32 = 12;

/// Worth trying again: the server is unreachable, went silent, or answered "busy / bad gateway / timeout".
fn is_transient(e: &portcode_llm::LlmError) -> bool {
    use portcode_llm::LlmError;
    match e {
        LlmError::Connect { .. } | LlmError::Stalled { .. } => true,
        LlmError::Http { status, .. } => matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504),
        _ => false,
    }
}

/// 2, 4, 8, 15, then 30 seconds.
fn reconnect_wait(attempt: u32) -> u64 {
    [2, 4, 8, 15, 30]
        .get(attempt.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(30)
}

fn short(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 90 {
        format!("{}…", one.chars().take(89).collect::<String>())
    } else {
        one
    }
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
        ctx.venv = cfg.venv.unwrap_or(true).into();
        if cfg.team_enabled() {
            ctx.team_parallel = cfg.team_parallel().into();
        }
        if depth == 0 {
            let ui2 = ui.clone();
            *lock(&ctx.notify) = Some(Arc::new(move |s| ui2.event(UiEvent::Info(s))));
        }
        ctx.output_limit = resolved.profile.result_chars();
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
                cancel: ctx.cancel.clone(),
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
            cli_session: None,
            probed: None,
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

    /// Load a session by id, unique prefix, number from [`Runner::sessions`] or `last`. Returns the id and message count.
    pub fn resume(&mut self, reference: &str) -> Result<(String, usize), String> {
        let st = self.store.as_ref().ok_or("sessions disabled")?;
        let id = st.resolve(reference)?;
        self.messages = st.load(&id)?;
        let meta = st.load_meta(&id);
        self.session_id = id.clone();
        self.cli_session = None;
        if let Some(m) = meta {
            // come back in the agent, model and mode the session was left in; a model or agent that no longer exists is skipped
            if !m.agent.is_empty() {
                let _ = self.set_agent(&m.agent);
            }
            if !m.model.is_empty() {
                let _ = self.set_model(&m.model);
            }
            self.ctx.deep.store(m.deep, Ordering::Relaxed);
        }
        Ok((id, self.messages.len()))
    }
    /// Agent, `provider/model` and deep flag, as saved with the session.
    pub fn state(&self) -> (String, String, bool) {
        (
            self.agent.name.to_string(),
            format!("{}/{}", self.resolved.provider.name, self.resolved.model),
            self.ctx.is_deep(),
        )
    }
    pub fn sessions(&self) -> Vec<crate::session::SessionInfo> {
        self.store.as_ref().map(|s| s.list()).unwrap_or_default()
    }
    pub fn delete_session(&mut self, reference: &str) -> Result<String, String> {
        let st = self.store.as_ref().ok_or("sessions disabled")?;
        let id = st.resolve(reference)?;
        if id == self.session_id {
            return Err("that is the session in use; start a new one first (/new)".into());
        }
        st.delete(&id)?;
        Ok(id)
    }
    /// The conversation so far (for redrawing after a resume).
    pub fn history(&self) -> &[Message] {
        &self.messages
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

    /// Ask the server how large the context window of the current model really is and use that instead of the
    /// built-in guess. Once per model; `None` when it was asked before, the server says nothing, or portcode.json
    /// sets `context_window` for this model (an explicit setting always wins).
    pub async fn detect_context(&mut self) -> Option<u32> {
        let key = format!("{}/{}", self.resolved.provider.name, self.resolved.model);
        if self.probed.as_deref() == Some(key.as_str()) {
            return None;
        }
        self.probed = Some(key);
        if portcode_llm::is_cli(&self.resolved.provider)
            || self
                .cfg
                .models
                .get(&self.resolved.model)
                .and_then(|o| o.get("context_window"))
                .is_some()
        {
            return None;
        }
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            self.client.context_window(&self.resolved.model),
        )
        .await
        .ok()
        .flatten()?;
        if n == self.resolved.profile.context_window {
            return None;
        }
        self.resolved.profile.context_window = n;
        Some(n)
    }

    /// Use a freshly loaded configuration (a provider was added from the app).
    pub fn reload_config(&mut self, cfg: Config) {
        self.cfg = cfg;
        self.probed = None;
    }

    pub fn set_model(&mut self, reference: &str) -> Result<(), String> {
        let r = self.cfg.resolve(Some(reference))?;
        self.probed = None;
        self.client = Client::new(r.provider.clone()).map_err(err_text)?;
        self.resolved = r;
        Ok(())
    }

    /// Change prompt style or reasoning effort for the current model without restarting.
    pub fn tune(&mut self, what: &str, val: &str) -> Result<String, String> {
        let prof = &mut self.resolved.profile;
        match what {
            "style" => {
                if !["auto", "compact", "full", "qwen38"].contains(&val) {
                    return Err("style is one of: auto, compact, full, qwen38".into());
                }
                prof.prompt_style = if val == "auto" {
                    portcode_llm::profile_for(&self.resolved.model).prompt_style
                } else {
                    val.to_string()
                };
                Ok(format!("prompt style: {}", prof.prompt_style))
            }
            "effort" => {
                if !["xhigh", "medium", "low", "none"].contains(&val) {
                    return Err("effort is one of: xhigh, medium, low, none (thinking off)".into());
                }
                if val == "none" {
                    prof.non_thinking();
                } else {
                    let base = portcode_llm::profile_for(&self.resolved.model);
                    prof.reasoning_effort = Some(val.to_string());
                    if prof.reasoning_effort.as_deref() != Some("none")
                        && prof.presence_penalty == Some(1.5)
                    {
                        prof.temperature = base.temperature;
                        prof.top_p = base.top_p;
                        prof.presence_penalty = base.presence_penalty;
                        prof.max_tokens = base.max_tokens;
                    }
                }
                Ok(format!(
                    "reasoning effort: {} (sent to the server as chat_template_kwargs; only Qwen3.8-style templates read it)",
                    val
                ))
            }
            "team" => {
                let on = val == "on";
                self.ctx.team_parallel.store(
                    if on { self.cfg.team_parallel() } else { 0 },
                    Ordering::Relaxed,
                );
                Ok(format!(
                    "team mode {}",
                    if on {
                        "on: pick the team agent (tab)"
                    } else {
                        "off"
                    }
                ))
            }
            "venv" => {
                let on = val == "on";
                self.ctx.venv.store(on, Ordering::Relaxed);
                Ok(format!(
                    "virtual environments {}",
                    if on { "on" } else { "off" }
                ))
            }
            "allow" => {
                let on = val == "on";
                self.cfg.cli_permission = Some(
                    if on {
                        "bypassPermissions"
                    } else {
                        "acceptEdits"
                    }
                    .into(),
                );
                Ok(format!("allow-all {}", if on { "on" } else { "off" }))
            }
            _ => Err("unknown setting".into()),
        }
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
        let budget = self.resolved.profile.input_budget();
        let sys = self.system();
        let over = |s: &Self, sys: &Message| s.est_tokens(sys).max(s.last_prompt_tokens) > budget;
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
        if !self.summarize_upto(cut).await {
            self.ui.event(UiEvent::Info(
                "context is large and compaction failed; continuing".into(),
            ));
        }
    }

    /// `/compact`: summarise the whole conversation into one message to free context. Returns a short report.
    pub async fn compact(&mut self) -> Result<String, String> {
        if portcode_llm::is_cli(&self.resolved.provider) {
            return Err(
                "the Claude Code / Codex backend manages its own context; use its own /compact"
                    .into(),
            );
        }
        let n = self.messages.len();
        if n < 2 {
            return Err("nothing to compact yet".into());
        }
        let before = self.est_tokens(&self.system());
        if !self.summarize_upto(n).await {
            return Err("compaction failed: the model did not return a summary. Try again, or set small_model to a model that works.".into());
        }
        self.last_prompt_tokens = 0;
        self.save();
        let after = self.est_tokens(&self.system());
        Ok(format!(
            "compacted {n} messages into a summary (about {before} → {after} tokens). Keep going; the goal, decisions, files and evidence ids are kept."
        ))
    }

    /// Replace the first `cut` messages with a model-written summary. False if no summary came back.
    async fn summarize_upto(&mut self, cut: usize) -> bool {
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
                let tail = self.messages.split_off(cut.min(self.messages.len()));
                self.messages = vec![Message::user(format!(
                    "[Summary of the earlier conversation]\n{}",
                    a.text.trim()
                ))];
                self.messages.extend(tail);
                self.ui.event(UiEvent::Compacted);
                true
            }
            _ => false,
        }
    }

    /// Write the session now and return its id, or `None` when nothing was said yet (nothing to resume).
    pub fn checkpoint(&self) -> Option<String> {
        if self
            .messages
            .iter()
            .all(|m| m.role == portcode_llm::Role::System)
        {
            return None;
        }
        self.save();
        self.store.as_ref().map(|_| self.session_id.clone())
    }

    fn save(&self) {
        if let Some(s) = &self.store {
            let _ = s.save(&self.session_id, &self.messages);
            let (agent, model, deep) = self.state();
            s.save_meta(
                &self.session_id,
                &crate::session::Meta { agent, model, deep },
            );
        }
    }

    /// Run one user turn to completion.
    /// One turn through the user's own `claude` / `codex` CLI (their subscription). See `backend`.
    async fn send_cli(&mut self, cli: crate::backend::Cli, text: &str) -> Result<Outcome, String> {
        use crate::backend;
        let bin = backend::find_binary(cli).ok_or_else(|| {
            format!(
                "{} is not installed or not on PATH. Install it and {}. `portcode doctor` shows what was found.",
                cli.binary_name(),
                cli.login_hint()
            )
        })?;
        self.messages.push(Message::user(text));
        self.ctx.cancel.store(false, Ordering::Relaxed);
        if let Some(n) = self.detect_context().await {
            self.ui.event(UiEvent::Info(format!(
                "context window: {n} tokens, as the server reports"
            )));
        }
        let deep = self.ctx.is_deep();
        let brief = agents::cli_brief(&self.agent, &self.skills, deep);
        let permission = self
            .cfg
            .cli_permission
            .clone()
            .unwrap_or_else(|| "acceptEdits".into());
        let model = self.resolved.model.clone();
        let req = backend::Request {
            cli,
            bin: &bin,
            model: &model,
            prompt: text,
            brief: &brief,
            root: &self.ctx.root,
            session: self.cli_session.as_deref(),
            deep,
            readonly: self.agent.readonly,
            permission: &permission,
        };
        let turn = backend::run(&req, self.ui.as_ref(), &self.ctx.cancel).await?;
        if let Some(s) = &turn.session {
            self.cli_session = Some(s.clone());
        }
        self.messages.push(Message::assistant_text(&turn.text));
        self.usage.prompt_tokens += turn.usage.prompt_tokens;
        self.usage.completion_tokens += turn.usage.completion_tokens;
        self.ui.event(UiEvent::Usage(turn.usage));
        let cancelled = self.ctx.cancelled();
        match turn.error {
            Some(e) if !cancelled => Err(e),
            _ => Ok(Outcome {
                text: turn.text,
                steps: turn.steps,
                usage: turn.usage,
                status: if cancelled {
                    Status::Cancelled
                } else {
                    Status::Done
                },
            }),
        }
    }

    pub async fn send(&mut self, text: &str) -> Result<Outcome, String> {
        if let Some(cli) = crate::backend::cli_for(&self.resolved.provider.name) {
            return self.send_cli(cli, text).await;
        }
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
        let (mut extensions, mut gate_nudges, mut todo_nudges) = (0u32, 0u32, 0u32);
        lock(&self.ctx.state).blocked = None;
        let (mut recent, mut repeats, mut nudges) = (Vec::<String>::new(), 0u32, 0u32);
        let mut total = Usage::default();
        let mut last_text = String::new();
        // output-length handling for small windows and models that stop early
        let (mut cut_nudges, mut boost, mut pieces) = (0u32, 1u32, String::new());
        {
            let sys_tokens = self.est_tokens(&self.system());
            let window = u64::from(self.resolved.profile.context_window);
            if sys_tokens * 100 > window * 35 {
                self.ui.event(UiEvent::Info(format!(
                    "the context window is small ({window} tokens) and the instructions already take about {sys_tokens}. Raise the server's context size, or set \"context_window\" for this model in portcode.json."
                )));
            }
        }
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
            // ask for as much output as fits next to this prompt (servers such as vLLM reject prompt + max_tokens > window)
            let mut prof = self.resolved.profile.clone();
            let prompt_est = self.est_tokens(&msgs[0]).max(self.last_prompt_tokens);
            let room = u64::from(prof.context_window).saturating_sub(prompt_est + 128);
            prof.max_tokens = (u64::from(prof.out_tokens()) * u64::from(boost))
                .min(room.max(128))
                .max(128) as u32;
            // the model request is dropped (its connection closed) as soon as the user cancels
            let flag = self.ctx.cancel.clone();
            let mut on_event = |e: StreamEvent| match e {
                StreamEvent::Text(t) => ui.event(UiEvent::Text(t)),
                StreamEvent::Reasoning(t) => ui.event(UiEvent::Reasoning(t)),
                StreamEvent::ToolStart(n) => ui.event(UiEvent::ToolStart(n)),
            };
            // a lost connection is retried with a growing pause instead of ending the turn; esc stops it
            let mut tries = 0u32;
            let res = loop {
                let r = tokio::select! {
                    r = self.client.chat(
                        &self.resolved.model,
                        &prof,
                        &msgs,
                        &tools,
                        &mut on_event,
                    ) => r,
                    _ = wait_cancel(flag.clone()) => {
                        self.save();
                        return Ok(Outcome {
                            text: last_text,
                            steps: step,
                            usage: total,
                            status: Status::Cancelled,
                        });
                    }
                };
                match r {
                    Err(e) if is_transient(&e) && tries < RECONNECT_TRIES => {
                        tries += 1;
                        let wait = reconnect_wait(tries);
                        self.ui.event(UiEvent::Info(format!(
                            "connection lost ({}). Reconnecting in {wait}s, attempt {tries} of {RECONNECT_TRIES}; esc stops",
                            short(&e.to_string())
                        )));
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {}
                            _ = wait_cancel(flag.clone()) => {
                                self.save();
                                return Ok(Outcome {
                                    text: last_text,
                                    steps: step,
                                    usage: total,
                                    status: Status::Cancelled,
                                });
                            }
                        }
                    }
                    other => {
                        if tries > 0 && other.is_ok() {
                            self.ui.event(UiEvent::Info("reconnected".into()));
                        }
                        break other;
                    }
                }
            };
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
            let mut a = a;
            if a.finish_reason.as_deref() == Some("length") {
                // the reply hit the output limit: a tool call whose JSON is not whole must not run
                // the client may have "repaired" the cut JSON into something valid but incomplete, so the last call is never trusted
                let broken = !a.tool_calls.is_empty();
                if broken {
                    a.tool_calls.pop();
                }
                if broken && a.tool_calls.is_empty() && cut_nudges < 4 && !final_step {
                    cut_nudges += 1;
                    if boost < 3 {
                        boost += 1;
                    }
                    self.ui.event(UiEvent::Info(
                        "the tool call was cut off by the output limit; asking for a smaller one"
                            .into(),
                    ));
                    if !a.text.is_empty() {
                        self.messages.push(a.clone().into_message());
                    } else {
                        self.messages.push(Message::assistant_text("(cut off)"));
                    }
                    self.messages.push(Message::user(
                        "Your tool call was cut off by the output limit, so it did not run. Make it smaller: write a file in parts (first call creates it, later calls use `write` with append true), or use `edit` for one small change.",
                    ));
                    continue;
                }
            }
            let (calls, finish, reply) = (
                a.tool_calls.clone(),
                a.finish_reason.clone(),
                a.text.clone(),
            );
            if !reply.is_empty() {
                last_text = reply.clone();
            }
            let cut_text = calls.is_empty() && finish.as_deref() == Some("length");
            self.messages.push(a.into_message());
            if calls.is_empty() {
                let empty = reply.trim().is_empty();
                if cut_text && cut_nudges < 4 && !final_step {
                    // keep what we have and ask for the rest, so a short output limit does not lose the end of an answer
                    cut_nudges += 1;
                    pieces.push_str(&reply);
                    if boost < 3 {
                        boost += 1;
                    }
                    self.messages.push(Message::user(
                        "Your reply was cut off by the output limit. Continue exactly where you stopped, without repeating anything.",
                    ));
                    continue;
                }
                if empty && nudges < 2 && !final_step {
                    nudges += 1;
                    self.messages.push(Message::user(
                        "Your reply was empty. Continue: call a tool, or give the final answer.",
                    ));
                    continue;
                }
                if !pieces.is_empty() {
                    last_text = format!("{pieces}{reply}");
                    pieces.clear();
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
                if !self.agent.gate
                    && !final_step
                    && todo_nudges < 2
                    && !reply.trim_end().ends_with('?')
                {
                    // agents without the finish gate: a reply that stops while the todo list still has open items
                    // (typically a "next step" written as text) is a model stopping early, not a finished job.
                    let open: Vec<String> = lock(&self.ctx.state)
                        .todos
                        .iter()
                        .filter(|t| t.status != "completed")
                        .map(|t| t.content.clone())
                        .collect();
                    if !open.is_empty() {
                        todo_nudges += 1;
                        self.ui.event(UiEvent::Info(format!(
                            "continuing: {} todo item(s) still open",
                            open.len()
                        )));
                        self.messages.push(Message::user(format!(
                            "You stopped, but the todo list still has open items:\n- {}\nDo not summarise. Carry out the next step now with a tool call. If only the user can unblock you, call `blocked` with the reason.",
                            open.join("\n- ")
                        )));
                        continue;
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
                let result = if self.ctx.cancelled() {
                    "[cancelled by user before this call ran]".to_string()
                } else if same >= 3 {
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
        let mut handle = tokio::spawn(async move { t2.run(&ctx, args).await });
        let flag = self.ctx.cancel.clone();
        let joined = tokio::select! {
            j = &mut handle => j,
            _ = wait_cancel(flag) => {
                // tools that watch the flag (bash, sub-agents) wind down on their own; anything else is stopped
                match tokio::time::timeout(std::time::Duration::from_secs(3), &mut handle).await {
                    Ok(j) => j,
                    Err(_) => {
                        handle.abort();
                        Ok(Err(crate::tool::ToolError(
                            "cancelled by user; the tool was stopped".into(),
                        )))
                    }
                }
            }
        };
        let limit = self.resolved.profile.result_chars();
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

/// Resolves when the user asks to cancel (Esc Esc, Ctrl-C, remote interrupt).
async fn wait_cancel(flag: Arc<std::sync::atomic::AtomicBool>) {
    while !flag.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
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
    "team",
    "venv",
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
    "unity",
    "dotnet",
    "unity_assets",
    "hexdump",
];

struct SubAgents {
    cfg: Config,
    root: std::path::PathBuf,
    gate: Arc<dyn Gate>,
    model: Option<String>,
    /// The parent session's cancel flag: Esc Esc stops sub-agents and team workers too.
    cancel: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl SubRunner for SubAgents {
    async fn run(&self, agent: &str, prompt: &str) -> Result<String, String> {
        let def = agents::find(agent).filter(|a| !a.primary).ok_or_else(|| {
            "agent must be a sub-agent: explore, general or a team role".to_string()
        })?;
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
        let (parent, child) = (self.cancel.clone(), r.cancel_flag());
        let watcher = tokio::spawn(async move {
            wait_cancel(parent).await;
            child.store(true, Ordering::Relaxed);
        });
        let out = Box::pin(r.send(prompt)).await;
        watcher.abort();
        let out = out?;
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
