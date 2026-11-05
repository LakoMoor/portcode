//! Sessions are JSON-lines files in `.portcode/sessions/` — human-readable, diffable, resumable.
use portcode_llm::Message;
use std::path::{Path, PathBuf};

/// One line of the session list.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub modified: u64,
    /// User and assistant messages.
    pub messages: usize,
    pub title: String,
}

impl SessionInfo {
    /// "3 min ago", "2 days ago".
    pub fn ago(&self) -> String {
        let d = portcode_evidence::now_secs().saturating_sub(self.modified);
        match d {
            0..=89 => "just now".into(),
            90..=5399 => format!("{} min ago", d / 60),
            5400..=129_599 => format!("{} h ago", d / 3600),
            _ => format!("{} days ago", d / 86_400),
        }
    }
}

/// What a session needs besides its messages, so `--resume` comes back the way it was left.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Meta {
    pub agent: String,
    /// `provider/model`.
    pub model: String,
    pub deep: bool,
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(".portcode/sessions"),
        }
    }
    pub fn new_id() -> String {
        format!("s-{}", portcode_evidence::now_secs())
    }
    fn file(&self, id: &str) -> Option<PathBuf> {
        (!id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .then(|| self.dir.join(format!("{id}.jsonl")))
    }
    pub fn save(&self, id: &str, msgs: &[Message]) -> Result<(), String> {
        let f = self.file(id).ok_or("bad session id")?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let mut text = String::new();
        for m in msgs {
            text += &serde_json::to_string(m).map_err(|e| e.to_string())?;
            text.push('\n');
        }
        let tmp = f.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
    }
    fn meta_file(&self, id: &str) -> Option<PathBuf> {
        self.file(id).map(|f| f.with_extension("meta.json"))
    }
    pub fn save_meta(&self, id: &str, m: &Meta) {
        if let (Some(f), Ok(t)) = (self.meta_file(id), serde_json::to_string(m)) {
            let _ = std::fs::create_dir_all(&self.dir);
            let _ = std::fs::write(f, t);
        }
    }
    pub fn load_meta(&self, id: &str) -> Option<Meta> {
        let t = std::fs::read_to_string(self.meta_file(id)?).ok()?;
        serde_json::from_str(&t).ok()
    }
    pub fn load(&self, id: &str) -> Result<Vec<Message>, String> {
        let f = self.file(id).ok_or("bad session id")?;
        let t = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        t.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| format!("corrupt session line: {e}")))
            .collect()
    }
    /// Saved sessions, newest first.
    pub fn list(&self) -> Vec<SessionInfo> {
        let mut v: Vec<SessionInfo> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                let id = n.strip_suffix(".jsonl")?.to_string();
                let modified = e
                    .metadata()
                    .ok()?
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                let msgs = self.load(&id).unwrap_or_default();
                let title = msgs
                    .iter()
                    .find(|m| m.role == portcode_llm::Role::User)
                    .and_then(|m| m.content.as_deref())
                    .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                    .map(|t| {
                        if t.chars().count() > 60 {
                            format!("{}…", t.chars().take(59).collect::<String>())
                        } else {
                            t
                        }
                    })
                    .unwrap_or_else(|| "(empty)".into());
                Some(SessionInfo {
                    id,
                    modified,
                    messages: msgs
                        .iter()
                        .filter(|m| {
                            matches!(
                                m.role,
                                portcode_llm::Role::User | portcode_llm::Role::Assistant
                            )
                        })
                        .count(),
                    title,
                })
            })
            .collect();
        v.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.id.cmp(&a.id)));
        v
    }
    /// An id, a unique id prefix, `last`, or a 1-based number from [`Store::list`].
    pub fn resolve(&self, r: &str) -> Result<String, String> {
        let r = r.trim();
        let all = self.list();
        if all.is_empty() {
            return Err("no saved sessions in this project".into());
        }
        if r.is_empty() || r == "last" || r == "latest" {
            return Ok(all[0].id.clone());
        }
        if let Ok(n) = r.parse::<usize>() {
            return all
                .get(n.wrapping_sub(1))
                .map(|s| s.id.clone())
                .ok_or_else(|| {
                    format!(
                        "no session number {n}; there are {}. List them with /sessions.",
                        all.len()
                    )
                });
        }
        if all.iter().any(|s| s.id == r) {
            return Ok(r.to_string());
        }
        let hits: Vec<&SessionInfo> = all.iter().filter(|s| s.id.starts_with(r)).collect();
        match hits.as_slice() {
            [one] => Ok(one.id.clone()),
            [] => Err(format!("no session '{r}'. List them with /sessions.")),
            _ => Err(format!(
                "'{r}' matches {} sessions; type more of the id.",
                hits.len()
            )),
        }
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let f = self.file(id).ok_or("bad session id")?;
        if let Some(m) = self.meta_file(id) {
            let _ = std::fs::remove_file(m);
        }
        std::fs::remove_file(&f).map_err(|e| format!("{}: {e}", f.display()))
    }
    /// Newest session id, if any.
    pub fn latest(&self) -> Option<String> {
        let mut v: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                Some((
                    e.metadata().ok()?.modified().ok()?,
                    n.strip_suffix(".jsonl")?.to_string(),
                ))
            })
            .collect();
        v.sort();
        v.pop().map(|x| x.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meta_roundtrips_and_is_not_listed_as_a_session() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::new(d.path());
        s.save("s-1", &[Message::user("x")]).unwrap();
        s.save_meta(
            "s-1",
            &Meta {
                agent: "team".into(),
                model: "ollama/qwen3:14b".into(),
                deep: true,
            },
        );
        let m = s.load_meta("s-1").unwrap();
        assert_eq!((m.agent.as_str(), m.deep), ("team", true));
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.latest().as_deref(), Some("s-1"));
        s.delete("s-1").unwrap();
        assert!(s.load_meta("s-1").is_none());
    }
    #[test]
    fn roundtrip_and_id_validation() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::new(d.path());
        s.save("s-1", &[Message::system("a"), Message::user("héllo")])
            .unwrap();
        assert_eq!(s.load("s-1").unwrap().len(), 2);
        assert_eq!(s.latest().as_deref(), Some("s-1"));
        assert!(s.save("../evil", &[]).is_err() && s.load("../evil").is_err());
    }

    #[test]
    fn lists_newest_first_and_resolves_numbers_prefixes_and_last() {
        let d = std::env::temp_dir().join(format!("pc-sess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let st = Store::new(&d);
        assert!(st.resolve("last").is_err(), "nothing saved yet");
        st.save("s-100", &[Message::user("first task\n  with   spaces")])
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        st.save(
            "s-200",
            &[Message::user("second"), Message::assistant_text("ok")],
        )
        .unwrap();
        let l = st.list();
        assert_eq!(l[0].id, "s-200");
        assert_eq!((l[0].messages, l[0].title.as_str()), (2, "second"));
        assert_eq!(l[1].title, "first task with spaces");
        assert_eq!(st.resolve("").unwrap(), "s-200");
        assert_eq!(st.resolve("2").unwrap(), "s-100");
        assert_eq!(st.resolve("s-1").unwrap(), "s-100");
        assert!(st.resolve("s-").unwrap_err().contains("matches 2"));
        assert!(st.resolve("9").unwrap_err().contains("no session number"));
        assert!(st.resolve("../x").is_err());
        st.delete("s-100").unwrap();
        assert_eq!(st.list().len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
