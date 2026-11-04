//! Sessions are JSON-lines files in `.portcode/sessions/` — human-readable, diffable, resumable.
use portcode_llm::Message;
use std::path::{Path, PathBuf};

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
    pub fn load(&self, id: &str) -> Result<Vec<Message>, String> {
        let f = self.file(id).ok_or("bad session id")?;
        let t = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        t.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| format!("corrupt session line: {e}")))
            .collect()
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
    fn roundtrip_and_id_validation() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::new(d.path());
        s.save("s-1", &[Message::system("a"), Message::user("héllo")])
            .unwrap();
        assert_eq!(s.load("s-1").unwrap().len(), 2);
        assert_eq!(s.latest().as_deref(), Some("s-1"));
        assert!(s.save("../evil", &[]).is_err() && s.load("../evil").is_err());
    }
}
