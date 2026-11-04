//! `portcode.json` — layered config: built-in defaults < `~/.config/portcode/portcode.json` < `./portcode.json`
//! (or `./.portcode/portcode.json`) < environment (`PORTCODE_MODEL`, `PORTCODE_BASE_URL`, `PORTCODE_API_KEY`).
use portcode_llm::{presets, profile_for, ModelProfile, ProviderConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_MODEL: &str = "lakomoor/deepseek-chat";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `provider/model`, e.g. `lakomoor/qwen3-coder`, `ollama/qwen3:14b`.
    pub model: Option<String>,
    /// Cheaper model for compaction and titles.
    pub small_model: Option<String>,
    pub default_agent: Option<String>,
    pub provider: BTreeMap<String, ProviderEntry>,
    /// Partial `ModelProfile` overrides keyed by model id (the part after `provider/`).
    pub models: BTreeMap<String, Value>,
    pub permission: BTreeMap<String, Value>,
    pub agent: BTreeMap<String, AgentOverride>,
    pub mcp: BTreeMap<String, McpEntry>,
    pub max_steps: Option<u32>,
    /// `clean-room` (default) or `deep`.
    pub mode: Option<String>,
    /// Extra directories file tools may touch without asking.
    pub allowed_dirs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderEntry {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentOverride {
    pub model: Option<String>,
    pub steps: Option<u32>,
    pub prompt_append: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct McpEntry {
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub enabled: Option<bool>,
}

pub fn global_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("portcode"))
}

fn state_path() -> Option<PathBuf> {
    global_dir().map(|d| d.join("state.json"))
}

fn read_state(p: &Path, key: &str) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
    v.get(key)?
        .as_str()
        .filter(|m| !m.is_empty())
        .map(String::from)
}

fn write_state(p: &Path, key: &str, val: &str) -> Result<(), String> {
    std::fs::create_dir_all(p.parent().ok_or("bad path")?).map_err(|e| e.to_string())?;
    let mut v: Value = std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Object(Default::default()));
    if let Some(o) = v.as_object_mut() {
        o.insert(key.into(), Value::String(val.into()));
    }
    std::fs::write(p, v.to_string()).map_err(|e| e.to_string())
}

/// Remember the model picked in the app (`/model`, `/provider`) so the next start uses it.
/// `-m` and `PORTCODE_MODEL` still win.
pub fn save_last_model(model: &str) -> Result<(), String> {
    write_state(&state_path().ok_or("no config directory")?, "model", model)
}

/// Small UI preferences kept next to the model (`theme`: dark or light).
pub fn save_pref(key: &str, val: &str) -> Result<(), String> {
    write_state(&state_path().ok_or("no config directory")?, key, val)
}
pub fn pref(key: &str) -> Option<String> {
    read_state(&state_path()?, key)
}

fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o,
    }
}

fn read_json(p: &Path) -> Result<Option<Value>, String> {
    match std::fs::read_to_string(p) {
        Ok(t) => {
            let t: String = t
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::from_str(&t)
                .map(Some)
                .map_err(|e| format!("{}: {e}", p.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

impl Config {
    pub fn load(project: &Path) -> Result<Config, String> {
        let mut v = Value::Object(Default::default());
        let mut files: Vec<PathBuf> = Vec::new();
        if let Some(g) = global_dir() {
            files.push(g.join("portcode.json"));
        }
        files.push(project.join("portcode.json"));
        files.push(project.join(".portcode/portcode.json"));
        for f in files {
            if let Some(o) = read_json(&f)? {
                merge(&mut v, o);
            }
        }
        let mut c: Config =
            serde_json::from_value(v).map_err(|e| format!("invalid config: {e}"))?;
        if let Some(m) = state_path().and_then(|p| read_state(&p, "model")) {
            c.model = Some(m);
        }
        if let Ok(m) = std::env::var("PORTCODE_MODEL") {
            if !m.is_empty() {
                c.model = Some(m);
            }
        }
        Ok(c)
    }

    /// Resolve `provider/model` (or bare model → default provider) into a ready provider + model id + profile.
    pub fn resolve(&self, reference: Option<&str>) -> Result<Resolved, String> {
        let r = reference
            .map(String::from)
            .or_else(|| self.model.clone())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        let (pname, model) = match r.split_once('/') {
            Some((p, m)) if self.knows_provider(p) => (p.to_string(), m.to_string()),
            _ => ("lakomoor".to_string(), r.clone()),
        };
        let mut provider =
            presets()
                .into_iter()
                .find(|p| p.name == pname)
                .unwrap_or(ProviderConfig {
                    name: pname.clone(),
                    base_url: String::new(),
                    api_key: None,
                    api_key_env: None,
                    headers: Default::default(),
                });
        if let Some(e) = self.provider.get(&pname) {
            if let Some(u) = &e.base_url {
                provider.base_url = u.clone();
            }
            if e.api_key.is_some() {
                provider.api_key = e.api_key.clone();
            }
            if e.api_key_env.is_some() {
                provider.api_key_env = e.api_key_env.clone();
            }
            provider.headers.extend(e.headers.clone());
        }
        if let Ok(u) = std::env::var("PORTCODE_BASE_URL") {
            if !u.is_empty() {
                provider.base_url = u;
            }
        }
        if let Ok(k) = std::env::var("PORTCODE_API_KEY") {
            if !k.is_empty() {
                provider.api_key = Some(k);
            }
        }
        if provider.api_key.is_none() {
            provider.api_key = stored_key(&pname);
        }
        if provider.base_url.is_empty() {
            return Err(format!(
                "provider '{pname}' has no base_url: add it under \"provider\" in portcode.json"
            ));
        }
        let mut pv = serde_json::to_value(profile_for(&model)).unwrap_or(Value::Null);
        if let Some(o) = self.models.get(&model) {
            merge(&mut pv, o.clone());
        }
        let profile: ModelProfile =
            serde_json::from_value(pv).map_err(|e| format!("bad profile for {model}: {e}"))?;
        Ok(Resolved {
            provider,
            model,
            profile,
        })
    }

    fn knows_provider(&self, p: &str) -> bool {
        self.provider.contains_key(p) || presets().iter().any(|x| x.name == p)
    }
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub provider: ProviderConfig,
    pub model: String,
    pub profile: ModelProfile,
}

fn auth_path() -> Option<PathBuf> {
    global_dir().map(|d| d.join("auth.json"))
}

pub fn stored_key(provider: &str) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(auth_path()?).ok()?).ok()?;
    v.get(provider)?.as_str().map(String::from)
}

/// Save a key to `auth.json` (mode 0600). Keys never go into `portcode.json`, which is meant to be committed.
pub fn store_key(provider: &str, key: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let p = auth_path().ok_or("no config directory on this platform")?;
    std::fs::create_dir_all(p.parent().ok_or("bad path")?).map_err(|e| e.to_string())?;
    let mut v: Value = std::fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Object(Default::default()));
    if let Some(o) = v.as_object_mut() {
        o.insert(provider.into(), Value::String(key.into()));
    }
    std::fs::write(
        &p,
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    Ok(p)
}

pub const INIT_TEMPLATE: &str = r#"{
  // PortCode config. Commit this file; keep keys in env vars or `portcode auth set <provider>`.
  "model": "lakomoor/deepseek-chat",
  "provider": {
    "lakomoor": { "base_url": "https://lakomoor.com/v1", "api_key_env": "LAKOMOOR_API_KEY" },
    "ollama":   { "base_url": "http://localhost:11434/v1" }
  },
  // Override sampling / context per model id (the part after "provider/").
  "models": {
    "qwen3:14b": { "context_window": 32768, "parallel_tool_calls": false }
  },
  "permission": {
    "edit": "ask",
    "bash": { "*": "ask", "cargo check*": "allow", "cargo test*": "allow", "cargo fmt*": "allow", "cargo clippy*": "allow" }
  }
}
"#;

/// Env var names of every built-in provider key; scrubbed from child processes.
pub fn all_key_envs() -> Vec<String> {
    presets()
        .into_iter()
        .filter_map(|p| p.api_key_env)
        .chain(["PORTCODE_API_KEY".to_string()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_model_round_trips() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub/state.json");
        assert_eq!(read_state(&p, "model"), None);
        write_state(&p, "model", "ollama/qwen3:14b").unwrap();
        write_state(&p, "theme", "light").unwrap();
        assert_eq!(read_state(&p, "model").as_deref(), Some("ollama/qwen3:14b"));
        assert_eq!(read_state(&p, "theme").as_deref(), Some("light"));
    }

    #[test]
    fn resolves_provider_model_and_profile() {
        let r = Config::default().resolve(Some("ollama/qwen3:14b")).unwrap();
        assert_eq!(
            (
                r.provider.name.as_str(),
                r.model.as_str(),
                r.profile.family.as_str()
            ),
            ("ollama", "qwen3:14b", "qwen")
        );
        assert_eq!(r.provider.base_url, "http://localhost:11434/v1");
    }
    #[test]
    fn bare_model_uses_lakomoor_and_slash_in_model_id_survives() {
        let r = Config::default().resolve(Some("Qwen/Qwen3-Coder")).unwrap();
        assert_eq!(
            (r.provider.name.as_str(), r.model.as_str()),
            ("lakomoor", "Qwen/Qwen3-Coder")
        );
    }
    #[test]
    fn profile_overrides_merge() {
        let mut c = Config::default();
        c.models
            .insert("m".into(), serde_json::json!({"context_window": 8000}));
        let r = c.resolve(Some("lakomoor/m")).unwrap();
        assert_eq!(r.profile.context_window, 8000);
        assert!(r.profile.text_tool_fallback);
    }
    #[test]
    fn init_template_parses() {
        let t: String = INIT_TEMPLATE
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let c: Config = serde_json::from_str(&t).unwrap();
        assert_eq!(
            c.provider["lakomoor"].base_url.as_deref(),
            Some("https://lakomoor.com/v1")
        );
    }
}
