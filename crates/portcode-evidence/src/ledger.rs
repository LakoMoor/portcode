use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// How a fact is known. Decompiler output and static reading are `Inference`, never `Observation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Directly seen: run output, a file's bytes, a captured screenshot, a hash.
    Observation,
    /// Derived from observations (static analysis, decompilation, reasoning). May be wrong.
    Inference,
    /// A named gap: something the port needs but nobody has established yet.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Where a record came from, so a reviewer can reproduce it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Tool or command that produced the fact (`survey`, `bash`, `rea.procedure_pseudo_code`…).
    pub tool: String,
    /// Arguments or command line, free text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub args: String,
    /// File or artifact the fact is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// sha256 of that artifact (or of a captured output) for later audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Pinned original build/version the fact applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// `E-0001`, `E-0002`… stable within one ledger.
    pub id: String,
    pub kind: Kind,
    /// Feature or subsystem the record belongs to (`physics.brake`, `cli.args`).
    pub subject: String,
    pub statement: String,
    pub confidence: Confidence,
    #[serde(default)]
    pub source: Source,
    /// Ids of records this one builds on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Screenshot / capture files (relative paths) that back the record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// For `Unknown`: set once resolved, with the id of the resolving record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    pub created: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger I/O at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("ledger line {line} is not valid evidence: {msg}")]
    Corrupt { line: usize, msg: String },
    #[error("no such evidence id: {0}")]
    NotFound(String),
    #[error("statement and subject must not be empty")]
    Empty,
}

/// Append-only JSON-lines ledger at `<dir>/ledger.jsonl`.
#[derive(Debug, Clone)]
pub struct Ledger {
    path: PathBuf,
}

impl Ledger {
    pub fn open(dir: impl AsRef<Path>) -> Self {
        Self {
            path: dir.as_ref().join("ledger.jsonl"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn io(&self, source: std::io::Error) -> LedgerError {
        LedgerError::Io {
            path: self.path.display().to_string(),
            source,
        }
    }

    /// Read every record. A missing file is an empty ledger; a corrupt line is an error naming the line.
    pub fn all(&self) -> Result<Vec<Evidence>, LedgerError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(self.io(e)),
        };
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let ev: Evidence = serde_json::from_str(line).map_err(|e| LedgerError::Corrupt {
                line: i + 1,
                msg: e.to_string(),
            })?;
            out.push(ev);
        }
        Ok(out)
    }

    fn next_id(existing: &[Evidence]) -> String {
        let max = existing
            .iter()
            .filter_map(|e| e.id.strip_prefix("E-").and_then(|n| n.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        format!("E-{:04}", max.saturating_add(1))
    }

    /// Add a record; the id and timestamp are assigned here.
    pub fn add(&self, mut ev: Evidence) -> Result<Evidence, LedgerError> {
        if ev.subject.trim().is_empty() || ev.statement.trim().is_empty() {
            return Err(LedgerError::Empty);
        }
        let existing = self.all()?;
        for dep in ev.depends_on.iter().chain(ev.resolved_by.iter()) {
            if !existing.iter().any(|e| &e.id == dep) {
                return Err(LedgerError::NotFound(dep.clone()));
            }
        }
        ev.id = Self::next_id(&existing);
        ev.created = crate::now_secs();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| self.io(e))?;
        }
        let mut line = serde_json::to_string(&ev).map_err(|e| LedgerError::Corrupt {
            line: 0,
            msg: e.to_string(),
        })?;
        line.push('\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| self.io(e))?;
        f.write_all(line.as_bytes()).map_err(|e| self.io(e))?;
        Ok(ev)
    }

    /// Mark an `Unknown` as resolved by another record (appends a superseding record; history is kept).
    pub fn resolve(&self, unknown_id: &str, by: &str) -> Result<Evidence, LedgerError> {
        let all = self.all()?;
        let u = all
            .iter()
            .find(|e| e.id == unknown_id)
            .ok_or_else(|| LedgerError::NotFound(unknown_id.into()))?;
        let mut r = u.clone();
        r.statement = format!("resolved: {}", u.statement);
        r.resolved_by = Some(by.to_string());
        r.depends_on = vec![unknown_id.to_string()];
        self.add(r)
    }

    /// Unknowns that no later record has resolved.
    pub fn open_unknowns(&self) -> Result<Vec<Evidence>, LedgerError> {
        let all = self.all()?;
        let resolved: Vec<&str> = all
            .iter()
            .filter(|e| e.resolved_by.is_some())
            .flat_map(|e| e.depends_on.iter().map(String::as_str))
            .collect();
        Ok(all
            .iter()
            .filter(|e| {
                e.kind == Kind::Unknown
                    && e.resolved_by.is_none()
                    && !resolved.contains(&e.id.as_str())
            })
            .cloned()
            .collect())
    }

    pub fn by_subject(&self, prefix: &str) -> Result<Vec<Evidence>, LedgerError> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|e| e.subject.starts_with(prefix))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: Kind, subject: &str, st: &str) -> Evidence {
        Evidence {
            id: String::new(),
            kind,
            subject: subject.into(),
            statement: st.into(),
            confidence: Confidence::Medium,
            source: Source {
                tool: "test".into(),
                ..Default::default()
            },
            depends_on: vec![],
            attachments: vec![],
            resolved_by: None,
            created: 0,
        }
    }

    #[test]
    fn ids_increment_and_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path());
        assert!(l.all().unwrap().is_empty());
        let a = l.add(ev(Kind::Observation, "cli", "prints usage")).unwrap();
        let b = l
            .add(ev(Kind::Inference, "cli", "exit code 2 on bad flag"))
            .unwrap();
        assert_eq!((a.id.as_str(), b.id.as_str()), ("E-0001", "E-0002"));
        assert_eq!(l.all().unwrap().len(), 2);
    }

    #[test]
    fn rejects_empty_and_dangling_refs() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path());
        assert!(matches!(
            l.add(ev(Kind::Observation, "", "x")),
            Err(LedgerError::Empty)
        ));
        let mut e = ev(Kind::Inference, "a", "b");
        e.depends_on = vec!["E-9999".into()];
        assert!(matches!(l.add(e), Err(LedgerError::NotFound(_))));
    }

    #[test]
    fn unknowns_close_when_resolved() {
        let d = tempfile::tempdir().unwrap();
        let l = Ledger::open(d.path());
        let u = l
            .add(ev(Kind::Unknown, "save", "save file checksum algorithm"))
            .unwrap();
        let o = l
            .add(ev(Kind::Observation, "save", "crc32 over bytes 8.."))
            .unwrap();
        assert_eq!(l.open_unknowns().unwrap().len(), 1);
        l.resolve(&u.id, &o.id).unwrap();
        assert!(l.open_unknowns().unwrap().is_empty());
    }

    #[test]
    fn corrupt_line_is_reported_not_panicked() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("ledger.jsonl"), "{not json}\n").unwrap();
        assert!(matches!(
            Ledger::open(d.path()).all(),
            Err(LedgerError::Corrupt { line: 1, .. })
        ));
    }
}
