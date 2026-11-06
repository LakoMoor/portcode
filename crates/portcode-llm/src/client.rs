use crate::profile::ModelProfile;
use crate::repair::{parse_text_tool_calls, repair_json, strip_think};
use crate::types::{Assistant, FunctionCall, Message, ToolCall, ToolDef, Usage};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Name of an environment variable holding the key (preferred over storing the key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

/// Built-in endpoints. All speak the OpenAI `/chat/completions` protocol.
/// `claude-code` and `codex` run through the user's own CLI instead of an HTTP API.
pub fn is_cli(p: &ProviderConfig) -> bool {
    p.base_url.starts_with("cli://")
}

pub fn presets() -> Vec<ProviderConfig> {
    let p = |n: &str, u: &str, env: Option<&str>| ProviderConfig {
        name: n.into(),
        base_url: u.into(),
        api_key: None,
        api_key_env: env.map(Into::into),
        headers: BTreeMap::new(),
    };
    vec![
        // Subscription backends: no HTTP at all, PortCode starts the official CLI (see portcode-agent::backend).
        p("claude-code", "cli://claude", None),
        p("codex", "cli://codex", None),
        p("ollama", "http://localhost:11434/v1", None),
        p("lmstudio", "http://localhost:1234/v1", None),
        p("llamacpp", "http://localhost:8080/v1", None),
        p("vllm", "http://localhost:8000/v1", Some("VLLM_API_KEY")),
        p("sglang", "http://localhost:30000/v1", None),
        p(
            "deepseek",
            "https://api.deepseek.com/v1",
            Some("DEEPSEEK_API_KEY"),
        ),
        p(
            "openrouter",
            "https://openrouter.ai/api/v1",
            Some("OPENROUTER_API_KEY"),
        ),
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("cannot reach {url}: {msg}. Is the server running and is base_url correct? (try `portcode doctor`)")]
    Connect { url: String, msg: String },
    #[error("{status} from {url}: {body}")]
    Http {
        status: u16,
        url: String,
        body: String,
    },
    #[error("401/403 from {url}: the key was rejected or missing. Set {hint} or run `portcode auth set <provider>`")]
    Auth { url: String, hint: String },
    #[error("stream from {url} stalled for {secs}s with no data")]
    Stalled { url: String, secs: u64 },
    #[error("unexpected response from {url}: {msg}")]
    Protocol { url: String, msg: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Text(String),
    Reasoning(String),
    ToolStart(String),
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    pub provider: ProviderConfig,
    idle: Duration,
}

impl Client {
    pub fn new(provider: ProviderConfig) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent("PortCode/0.1")
            .build()
            .map_err(|e| LlmError::Connect {
                url: provider.base_url.clone(),
                msg: e.to_string(),
            })?;
        Ok(Self {
            http,
            provider,
            idle: Duration::from_secs(120),
        })
    }

    pub fn with_idle_timeout(mut self, d: Duration) -> Self {
        self.idle = d;
        self
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.provider.base_url.trim_end_matches('/'), path)
    }

    fn key(&self) -> Option<String> {
        self.provider
            .api_key
            .clone()
            .filter(|k| !k.is_empty())
            .or_else(|| {
                self.provider
                    .api_key_env
                    .as_deref()
                    .and_then(|e| std::env::var(e).ok())
                    .filter(|k| !k.is_empty())
            })
    }

    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self.http.request(method, self.url(path));
        if let Some(k) = self.key() {
            r = r.bearer_auth(k);
        }
        for (k, v) in &self.provider.headers {
            r = r.header(k, v);
        }
        r
    }

    fn map_status(&self, status: u16, body: String) -> LlmError {
        if status == 401 || status == 403 {
            LlmError::Auth {
                url: self.provider.base_url.clone(),
                hint: self
                    .provider
                    .api_key_env
                    .clone()
                    .unwrap_or_else(|| "api_key in portcode.json".into()),
            }
        } else {
            LlmError::Http {
                status,
                url: self.provider.base_url.clone(),
                body: body.chars().take(400).collect(),
            }
        }
    }

    /// `GET /models` → ids, sorted.
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = self.provider.base_url.clone();
        let resp = self
            .req(reqwest::Method::GET, "models")
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| LlmError::Connect {
                url: url.clone(),
                msg: e.to_string(),
            })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status >= 400 {
            return Err(self.map_status(status, text));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| LlmError::Protocol {
            url: url.clone(),
            msg: e.to_string(),
        })?;
        let mut ids: Vec<String> = v
            .get("data")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            // Ollama native-style listing
            ids = v
                .get("models")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|m| {
                            m.get("name")
                                .or_else(|| m.get("id"))
                                .and_then(Value::as_str)
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        ids.sort();
        Ok(ids)
    }

    /// The context window the server itself reports for `model`, in tokens. Tries, in order: the `/models` entry
    /// (vLLM `max_model_len`, OpenRouter `context_length`, others), llama.cpp `/props`, LM Studio `/api/v0/models`
    /// (the loaded size wins), Ollama `/api/ps` (the running size). `None` when the server says nothing.
    pub async fn context_window(&self, model: &str) -> Option<u32> {
        const KEYS: &[&str] = &[
            "loaded_context_length",
            "max_model_len",
            "context_length",
            "context_window",
            "max_context_length",
            "max_context_tokens",
            "max_input_tokens",
            "n_ctx",
        ];
        let sane = |n: u64| (1024..=4_000_000).contains(&n).then_some(n as u32);
        let root = self.provider.base_url.trim_end_matches('/');
        let root = root.strip_suffix("/v1").unwrap_or(root).to_string();
        let get = |url: String| {
            let mut r = self.http.get(url).timeout(Duration::from_secs(5));
            if let Some(k) = self.key() {
                r = r.bearer_auth(k);
            }
            async move {
                let resp = r.send().await.ok()?;
                if !resp.status().is_success() {
                    return None;
                }
                resp.json::<Value>().await.ok()
            }
        };
        let find = |entries: &[Value]| -> Option<u32> {
            let e = entries.iter().find(|m| {
                ["id", "name", "model"]
                    .iter()
                    .any(|k| m.get(*k).and_then(Value::as_str) == Some(model))
            })?;
            KEYS.iter()
                .find_map(|k| e.get(*k).and_then(Value::as_u64).and_then(sane))
        };
        if let Some(v) = get(self.url("models")).await {
            for key in ["data", "models"] {
                if let Some(n) = v.get(key).and_then(Value::as_array).and_then(|a| find(a)) {
                    return Some(n);
                }
            }
        }
        if let Some(v) = get(format!("{root}/props")).await {
            let n = v
                .pointer("/default_generation_settings/n_ctx")
                .or_else(|| v.get("n_ctx"))
                .and_then(Value::as_u64)
                .and_then(sane);
            if n.is_some() {
                return n;
            }
        }
        if let Some(v) = get(format!("{root}/api/v0/models")).await {
            if let Some(n) = v
                .get("data")
                .and_then(Value::as_array)
                .and_then(|a| find(a))
            {
                return Some(n);
            }
        }
        if let Some(v) = get(format!("{root}/api/ps")).await {
            if let Some(n) = v
                .get("models")
                .and_then(Value::as_array)
                .and_then(|a| find(a))
            {
                return Some(n);
            }
        }
        None
    }

    fn body(
        &self,
        model: &str,
        p: &ModelProfile,
        msgs: &[Message],
        tools: &[ToolDef],
        minimal: bool,
    ) -> Value {
        let messages: Vec<Value> = msgs
            .iter()
            .map(|m| {
                let mut v = serde_json::to_value(m).unwrap_or(Value::Null);
                if let Some(o) = v.as_object_mut() {
                    if !p.echo_reasoning {
                        o.remove("reasoning_content");
                    }
                    if m.role == crate::Role::Assistant && m.content.is_none() {
                        o.insert("content".into(), Value::String(String::new()));
                    }
                }
                v
            })
            .collect();
        let mut b = json!({"model": model, "messages": messages, "stream": true});
        if let Some(o) = b.as_object_mut() {
            if let Some(t) = p.temperature {
                o.insert("temperature".into(), json!(t));
            }
            if let Some(t) = p.top_p {
                o.insert("top_p".into(), json!(t));
            }
            o.insert("max_tokens".into(), json!(p.max_tokens));
            if !tools.is_empty() {
                o.insert(
                    "tools".into(),
                    serde_json::to_value(tools).unwrap_or(Value::Null),
                );
            }
            if !minimal {
                o.insert("stream_options".into(), json!({"include_usage": true}));
                if let Some(k) = p.top_k {
                    o.insert("top_k".into(), json!(k));
                }
                if let Some(x) = p.presence_penalty {
                    o.insert("presence_penalty".into(), json!(x));
                }
                let mut kw = serde_json::Map::new();
                match p.reasoning_effort.as_deref() {
                    Some("none") => {
                        kw.insert("enable_thinking".into(), json!(false));
                    }
                    Some(e) => {
                        kw.insert("reasoning_effort".into(), json!(e));
                    }
                    None => {}
                }
                if let Some(k) = p.preserve_thinking {
                    kw.insert("preserve_thinking".into(), json!(k));
                }
                if !kw.is_empty() {
                    o.insert("chat_template_kwargs".into(), Value::Object(kw));
                }
                if !tools.is_empty() {
                    o.insert("tool_choice".into(), json!("auto"));
                    if !p.parallel_tool_calls {
                        o.insert("parallel_tool_calls".into(), json!(false));
                    }
                }
            }
        }
        b
    }

    /// One streamed completion. `on` receives incremental events; the returned [`Assistant`] is fully assembled,
    /// with tool arguments repaired to valid JSON and (if enabled) text-form tool calls promoted.
    pub async fn chat(
        &self,
        model: &str,
        profile: &ModelProfile,
        msgs: &[Message],
        tools: &[ToolDef],
        on: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<Assistant, LlmError> {
        let url = self.provider.base_url.clone();
        let mut minimal = false;
        let mut attempt = 0u32;
        let resp = loop {
            attempt += 1;
            let sent = self
                .req(reqwest::Method::POST, "chat/completions")
                .json(&self.body(model, profile, msgs, tools, minimal))
                .send()
                .await;
            match sent {
                Err(e) if attempt < 3 && (e.is_connect() || e.is_timeout()) => {
                    tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
                    continue;
                }
                Err(e) => {
                    return Err(LlmError::Connect {
                        url,
                        msg: e.to_string(),
                    })
                }
                Ok(r) => {
                    let s = r.status().as_u16();
                    if s < 400 {
                        break r;
                    }
                    let body = r.text().await.unwrap_or_default();
                    if s == 400 && !minimal {
                        minimal = true;
                        continue;
                    } // server rejected optional fields: retry plain
                    if (s == 429 || s >= 500) && attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(u64::from(attempt) * 2)).await;
                        continue;
                    }
                    return Err(self.map_status(s, body));
                }
            }
        };

        let mut acc = Accum::new(profile.strip_think);
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let next = tokio::time::timeout(self.idle, stream.next())
                .await
                .map_err(|_| LlmError::Stalled {
                    url: self.provider.base_url.clone(),
                    secs: self.idle.as_secs(),
                })?;
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|e| LlmError::Connect {
                url: self.provider.base_url.clone(),
                msg: e.to_string(),
            })?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    buf.clear();
                    break;
                }
                match serde_json::from_str::<Value>(data) {
                    Ok(v) => {
                        if let Some(err) = v.get("error") {
                            return Err(LlmError::Protocol {
                                url: self.provider.base_url.clone(),
                                msg: err.to_string(),
                            });
                        }
                        acc.feed(&v, on);
                    }
                    Err(_) => continue, // keep-alive / partial junk from lenient servers
                }
            }
        }
        Ok(acc.finish(profile, tools, on))
    }
}

struct Accum {
    text: String,
    reasoning: String,
    calls: BTreeMap<u64, (String, String, String)>,
    finish: Option<String>,
    usage: Option<Usage>,
    think: ThinkFilter,
}

impl Accum {
    fn new(strip: bool) -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            calls: BTreeMap::new(),
            finish: None,
            usage: None,
            think: ThinkFilter::new(strip),
        }
    }

    fn feed(&mut self, v: &Value, on: &mut (dyn FnMut(StreamEvent) + Send)) {
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            self.usage = Some(Usage {
                prompt_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
                completion_tokens: u
                    .get("completion_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            });
        }
        let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
            return;
        };
        if let Some(f) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish = Some(f.into());
        }
        let Some(d) = choice.get("delta") else { return };
        for key in ["reasoning_content", "reasoning"] {
            if let Some(r) = d.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                self.reasoning.push_str(r);
                on(StreamEvent::Reasoning(r.into()));
            }
        }
        if let Some(t) = d
            .get("content")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            for ev in self.think.push(t) {
                match &ev {
                    StreamEvent::Text(s) => self.text.push_str(s),
                    StreamEvent::Reasoning(s) => self.reasoning.push_str(s),
                    _ => {}
                }
                on(ev);
            }
        }
        if let Some(tcs) = d.get("tool_calls").and_then(Value::as_array) {
            for tc in tcs {
                let idx = tc.get("index").and_then(Value::as_u64).unwrap_or(0);
                let e = self.calls.entry(idx).or_default();
                if let Some(id) = tc
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    e.0 = id.into();
                }
                if let Some(f) = tc.get("function") {
                    if let Some(n) = f
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        if e.1.is_empty() {
                            on(StreamEvent::ToolStart(n.into()));
                        }
                        e.1.push_str(n);
                    }
                    if let Some(a) = f.get("arguments").and_then(Value::as_str) {
                        e.2.push_str(a);
                    }
                }
            }
        }
    }

    fn finish(
        mut self,
        profile: &ModelProfile,
        tools: &[ToolDef],
        on: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Assistant {
        for ev in self.think.flush() {
            if let StreamEvent::Text(s) = &ev {
                self.text.push_str(s);
            } else if let StreamEvent::Reasoning(s) = &ev {
                self.reasoning.push_str(s);
            }
            on(ev);
        }
        let mut calls: Vec<ToolCall> = self
            .calls
            .into_iter()
            .filter(|(_, c)| !c.1.is_empty())
            .map(|(i, (id, name, args))| ToolCall {
                id: if id.is_empty() {
                    format!("call_{i}")
                } else {
                    id
                },
                kind: "function".into(),
                function: FunctionCall {
                    name,
                    arguments: repair_json(&args).unwrap_or(args),
                },
            })
            .collect();
        let mut text = self.text;
        if calls.is_empty() && profile.text_tool_fallback && !tools.is_empty() {
            let known: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
            let (c, rest) = parse_text_tool_calls(&text, &known);
            if !c.is_empty() {
                calls = c;
                text = rest;
            }
        }
        if profile.strip_think {
            text = strip_think(&text);
        }
        Assistant {
            text,
            reasoning: self.reasoning,
            tool_calls: calls,
            finish_reason: self.finish,
            usage: self.usage,
        }
    }
}

/// Routes inline `<think>…</think>` content to Reasoning events while streaming (holds back partial tags).
struct ThinkFilter {
    on: bool,
    inside: bool,
    pending: String,
}
impl ThinkFilter {
    fn new(on: bool) -> Self {
        Self {
            on,
            inside: false,
            pending: String::new(),
        }
    }
    fn emit(&self, s: &str, out: &mut Vec<StreamEvent>) {
        if !s.is_empty() {
            out.push(if self.inside {
                StreamEvent::Reasoning(s.into())
            } else {
                StreamEvent::Text(s.into())
            });
        }
    }
    fn push(&mut self, t: &str) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        if !self.on {
            out.push(StreamEvent::Text(t.into()));
            return out;
        }
        self.pending.push_str(t);
        loop {
            let tag = if self.inside { "</think>" } else { "<think>" };
            if let Some(i) = self.pending.find(tag) {
                let head = self.pending.get(..i).unwrap_or("").to_string();
                self.emit(&head, &mut out);
                self.pending = self.pending.get(i + tag.len()..).unwrap_or("").to_string();
                self.inside = !self.inside;
                continue;
            }
            // hold back a suffix that could be the start of the tag
            let keep = (1..tag.len())
                .rev()
                .find(|n| {
                    self.pending.len() >= *n
                        && self.pending.is_char_boundary(self.pending.len() - n)
                        && tag.starts_with(
                            self.pending
                                .get(self.pending.len() - n..)
                                .unwrap_or("\u{0}"),
                        )
                })
                .unwrap_or(0);
            let cut = self.pending.len() - keep;
            let head = self.pending.get(..cut).unwrap_or("").to_string();
            self.emit(&head, &mut out);
            self.pending = self.pending.get(cut..).unwrap_or("").to_string();
            return out;
        }
    }
    fn flush(&mut self) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        let p = std::mem::take(&mut self.pending);
        self.emit(&p, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn qwen38_request_carries_card_sampling_and_template_kwargs() {
        let c = Client::new(presets().remove(1)).unwrap();
        let mut p = crate::profile_for("qwen3.8-27b");
        let b = c.body("qwen3.8-27b", &p, &[], &[], false);
        assert_eq!(b["temperature"], 1.0);
        assert_eq!(b["top_k"], 20);
        assert_eq!(b["presence_penalty"], 0.0);
        assert_eq!(b["chat_template_kwargs"]["reasoning_effort"], "medium");
        assert_eq!(b["chat_template_kwargs"]["preserve_thinking"], true);
        p.non_thinking();
        let b = c.body("qwen3.8-27b", &p, &[], &[], false);
        assert_eq!(b["chat_template_kwargs"]["enable_thinking"], false);
        assert!(b["chat_template_kwargs"].get("reasoning_effort").is_none());
        assert_eq!(b["presence_penalty"], 1.5);
        // the minimal retry (for servers that reject extra fields) sends none of it
        let m = c.body("qwen3.8-27b", &p, &[], &[], true);
        assert!(m.get("chat_template_kwargs").is_none() && m.get("presence_penalty").is_none());
        // other families are untouched
        let g = c.body("llama3", &crate::profile_for("llama3"), &[], &[], false);
        assert!(g.get("chat_template_kwargs").is_none());
    }

    use super::*;
    #[test]
    fn think_filter_splits_across_chunks() {
        let mut f = ThinkFilter::new(true);
        let mut all = Vec::new();
        for c in ["Hi <thi", "nk>secret</th", "ink> there"] {
            all.extend(f.push(c));
        }
        all.extend(f.flush());
        let text: String = all
            .iter()
            .filter_map(|e| {
                if let StreamEvent::Text(s) = e {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        let reason: String = all
            .iter()
            .filter_map(|e| {
                if let StreamEvent::Reasoning(s) = e {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "Hi  there");
        assert_eq!(reason, "secret");
    }
}
