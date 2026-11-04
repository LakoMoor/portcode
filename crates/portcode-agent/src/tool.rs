//! The tool contract. Everything the model can do is a [`Tool`]: a name, a JSON schema, an example
//! (open models follow examples far better than prose) and an async `run` that never panics.
use crate::permission::{Decision, Gate, Reply, Request, Rules};
use async_trait::async_trait;
use portcode_evidence::Ledger;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct ToolError(pub String);
impl<T: std::fmt::Display> From<T> for ToolError {
    fn from(e: T) -> Self {
        ToolError(e.to_string())
    }
}
pub type ToolResult = Result<Out, ToolError>;
pub fn err<T>(s: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(s.into()))
}

#[derive(Debug, Clone, Default)]
pub struct Out {
    pub title: String,
    pub text: String,
}
impl Out {
    pub fn new(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Todo {
    pub content: String,
    pub status: String,
}

#[derive(Default)]
pub struct State {
    pub read: HashMap<PathBuf, SystemTime>,
    pub todos: Vec<Todo>,
    pub loaded_skills: HashSet<String>,
}

/// Runs a sub-agent for the `task` tool (implemented by the agent loop; kept as a trait to avoid a cycle).
#[async_trait]
pub trait SubRunner: Send + Sync {
    async fn run(&self, agent: &str, prompt: &str) -> Result<String, String>;
}

pub struct Ctx {
    pub root: PathBuf,
    pub gate: Arc<dyn Gate>,
    pub rules: Mutex<Rules>,
    pub state: Mutex<State>,
    pub ledger: Ledger,
    pub cancel: Arc<AtomicBool>,
    /// Read-only agent: file edits are denied and nothing may prompt.
    pub readonly: AtomicBool,
    pub output_limit: usize,
    pub allowed_dirs: Vec<PathBuf>,
    pub scrub_env: Vec<String>,
    pub sub: Mutex<Option<Arc<dyn SubRunner>>>,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Ctx {
    pub fn new(root: PathBuf, gate: Arc<dyn Gate>, rules: Rules) -> Self {
        let ledger = Ledger::open(root.join(".portcode/evidence"));
        Self {
            root,
            gate,
            rules: Mutex::new(rules),
            state: Mutex::new(State::default()),
            ledger,
            cancel: Arc::new(AtomicBool::new(false)),
            readonly: AtomicBool::new(false),
            output_limit: 12_000,
            allowed_dirs: vec![],
            scrub_env: vec![],
            sub: Mutex::new(None),
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Check rules, ask the user if needed. `Err` carries text that goes back to the model.
    pub async fn permit(
        &self,
        tool: &str,
        target: &str,
        default: Decision,
        reason: &str,
    ) -> Result<(), ToolError> {
        let d = lock(&self.rules).check(tool, target, default);
        let ro = self.readonly.load(Ordering::Relaxed);
        let d = if ro && (tool == "edit" || d == Decision::Ask) {
            Decision::Deny
        } else {
            d
        };
        match d {
            Decision::Allow => Ok(()),
            Decision::Deny => err(format!("Permission denied by rule for {tool}: {target}. Do not retry this; choose another approach or tell the user.")),
            Decision::Ask => match self.gate.ask(&Request { tool: tool.into(), target: target.into(), reason: reason.into() }).await {
                Reply::Once => Ok(()),
                Reply::Always => { lock(&self.rules).add(tool, target, Decision::Allow); Ok(()) }
                Reply::Reject => err(format!("The user rejected {tool}: {target}. Do not retry it; ask the user how to proceed.")),
            },
        }
    }

    /// Resolve a model-supplied path inside the project (or an allowed dir), asking before leaving it.
    pub async fn path(&self, raw: &str, write: bool) -> Result<PathBuf, ToolError> {
        if raw.trim().is_empty() {
            return err("path is empty");
        }
        let p = Path::new(raw);
        let joined = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        };
        let norm = normalize(&joined);
        let real = canonical_prefix(&norm);
        let root_real = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        let inside =
            real.starts_with(&root_real) || self.allowed_dirs.iter().any(|d| real.starts_with(d));
        if !inside {
            let dir = real.parent().unwrap_or(&real).display().to_string();
            self.permit(
                "external_directory",
                &dir,
                Decision::Ask,
                "path is outside the project",
            )
            .await?;
        }
        if write && norm.components().any(|c| c.as_os_str() == ".git") {
            return err("writing inside .git is not allowed");
        }
        let name = norm
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let secret =
            (name.starts_with(".env") && !name.ends_with(".example") && !name.ends_with(".sample"))
                || name.ends_with(".pem")
                || name.starts_with("id_rsa")
                || name.ends_with(".key");
        if secret {
            self.permit(
                "secret_file",
                &norm.display().to_string(),
                Decision::Ask,
                "file may contain secrets",
            )
            .await?;
        }
        Ok(norm)
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .display()
            .to_string()
    }
}

/// Lexical `.`/`..` removal (no filesystem access).
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize the longest existing ancestor (follows symlinks) and re-append the rest.
fn canonical_prefix(p: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = cur.canonicalize() {
            return tail.iter().rev().fold(c, |acc, t| acc.join(t));
        }
        match (
            cur.file_name().map(|n| n.to_os_string()),
            cur.parent().map(Path::to_path_buf),
        ) {
            (Some(n), Some(par)) => {
                tail.push(n);
                cur = par;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// Keep head and tail of long text, on char boundaries, with an explicit marker.
pub fn clip(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let (h, t) = (limit * 2 / 3, limit / 3);
    let mut a = h;
    while a > 0 && !text.is_char_boundary(a) {
        a -= 1;
    }
    let mut b = text.len() - t;
    while b < text.len() && !text.is_char_boundary(b) {
        b += 1;
    }
    format!(
        "{}\n[... {} bytes omitted; narrow the request (offset/limit, grep, head/tail) ...]\n{}",
        text.get(..a).unwrap_or(""),
        b - a,
        text.get(b..).unwrap_or("")
    )
}

pub struct Args<'a> {
    pub v: &'a Value,
    pub tool: &'a str,
    pub example: &'a str,
}
impl Args<'_> {
    fn miss(&self, k: &str) -> ToolError {
        ToolError(format!(
            "{}: missing or invalid parameter '{k}'. Example: {}",
            self.tool, self.example
        ))
    }
    pub fn str(&self, k: &str) -> Result<String, ToolError> {
        self.v
            .get(k)
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| self.miss(k))
    }
    pub fn opt_str(&self, k: &str) -> Option<String> {
        self.v
            .get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(String::from)
    }
    /// Accepts numbers and numeric strings (small models often quote numbers).
    pub fn opt_u64(&self, k: &str) -> Option<u64> {
        self.v.get(k).and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
    }
    pub fn opt_f64(&self, k: &str) -> Option<f64> {
        self.v.get(k).and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
        })
    }
    pub fn bool(&self, k: &str) -> bool {
        self.v
            .get(k)
            .map(|x| x.as_bool().unwrap_or_else(|| x.as_str() == Some("true")))
            .unwrap_or(false)
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn schema(&self) -> Value;
    /// Shown to the model inside the description; also used in "missing parameter" errors.
    fn example(&self) -> String {
        String::new()
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult;
}

pub fn obj(props: Value, required: &[&str]) -> Value {
    serde_json::json!({"type": "object", "properties": props, "required": required})
}

#[derive(Default, Clone)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
}
impl Registry {
    pub fn add(&mut self, t: Arc<dyn Tool>) {
        self.tools.retain(|x| x.name() != t.name());
        self.tools.push(t);
    }
    pub fn get(&self, n: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == n).cloned()
    }
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name()).collect()
    }
    pub fn defs(&self, allow: &dyn Fn(&str) -> bool) -> Vec<portcode_llm::ToolDef> {
        self.tools
            .iter()
            .filter(|t| allow(&t.name()))
            .map(|t| {
                let ex = t.example();
                let d = if ex.is_empty() {
                    t.description()
                } else {
                    format!("{}\nExample arguments: {}", t.description(), ex)
                };
                portcode_llm::ToolDef::new(t.name(), d, t.schema())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{AutoGate, Reply};

    fn ctx(root: &Path, reply: Reply) -> Ctx {
        Ctx::new(
            root.to_path_buf(),
            Arc::new(AutoGate(reply)),
            Rules::default(),
        )
    }

    #[tokio::test]
    async fn dot_dot_cannot_escape_without_permission() {
        let d = tempfile::tempdir().unwrap();
        let c = ctx(d.path(), Reply::Reject);
        assert!(c.path("a/b.txt", false).await.is_ok());
        assert!(c.path("../../etc/passwd", false).await.is_err());
        assert!(c.path("/etc/hosts", false).await.is_err());
        assert!(ctx(d.path(), Reply::Once)
            .path("/etc/hosts", false)
            .await
            .is_ok());
    }
    #[tokio::test]
    async fn symlink_escape_is_caught() {
        let d = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(out.path(), d.path().join("link")).unwrap();
        assert!(ctx(d.path(), Reply::Reject)
            .path("link/secret.txt", false)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn git_dir_and_env_files_are_protected() {
        let d = tempfile::tempdir().unwrap();
        let c = ctx(d.path(), Reply::Reject);
        assert!(c.path(".git/config", true).await.is_err());
        assert!(c.path(".env", false).await.is_err());
        assert!(c.path(".env.example", false).await.is_ok());
    }
    #[test]
    fn clip_is_char_safe() {
        let s = "é".repeat(5000);
        let c = clip(&s, 1000);
        assert!(c.contains("omitted") && c.len() < 1300);
        assert_eq!(clip("short", 100), "short");
    }
    #[test]
    fn args_accept_quoted_numbers() {
        let v = serde_json::json!({"limit": "50", "all": "true"});
        let a = Args {
            v: &v,
            tool: "t",
            example: "{}",
        };
        assert_eq!(a.opt_u64("limit"), Some(50));
        assert!(a.bool("all"));
        assert!(a.str("nope").unwrap_err().0.contains("Example"));
    }
}
