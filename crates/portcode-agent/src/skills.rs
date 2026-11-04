//! Skills: `SKILL.md` folders with YAML-ish frontmatter. The `rustport` skill is embedded in the binary;
//! others are discovered from `.portcode/skills`, `.claude/skills`, `.agents/skills` and `~/.config/portcode/skills`.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
    pub dir: Option<PathBuf>,
    pub embedded: &'static [(&'static str, &'static str)],
}

macro_rules! emb { ($($p:literal),* $(,)?) => { &[$(($p, include_str!(concat!("../../../skills/rustport/", $p)))),*] }; }

/// Files of the bundled skill that the `skill` tool may serve by relative path.
pub const RUSTPORT_FILES: &[(&str, &str)] = emb![
    "SKILL.md",
    "references/evidence.md",
    "references/clean-room.md",
    "references/architecture.md",
    "references/site-and-screenshots.md",
    "references/pitfalls.md",
    "references/local-llm.md",
    "references/workbench.md",
    "references/reverse-methods.md",
    "references/compatible-runtime.md",
    "references/native-lab.md",
    "references/agent-roles.md",
    "references/scenarios.md",
    "references/agents.md",
    "references/rea.md",
    "assets/mappings.md",
    "assets/PORT_TEMPLATE.md",
];

pub fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>, String) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, None, text.to_string());
    };
    let Some(end) = rest.find("\n---") else {
        return (None, None, text.to_string());
    };
    let (fm, body) = (
        rest.get(..end).unwrap_or(""),
        rest.get(end + 4..).unwrap_or("").trim_start_matches('\n'),
    );
    let get = |k: &str| {
        fm.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}:")))
            .map(|v| v.trim().trim_matches('"').to_string())
    };
    (get("name"), get("description"), body.to_string())
}

pub fn bundled() -> Skill {
    let text = RUSTPORT_FILES
        .iter()
        .find(|(p, _)| *p == "SKILL.md")
        .map(|(_, t)| *t)
        .unwrap_or("");
    let (n, d, body) = parse_frontmatter(text);
    Skill {
        name: n.unwrap_or_else(|| "rustport".into()),
        description: d.unwrap_or_default(),
        body,
        dir: None,
        embedded: RUSTPORT_FILES,
    }
}

pub fn discover(project: &Path) -> Vec<Skill> {
    let mut found = vec![bundled()];
    let mut roots: Vec<PathBuf> = [".portcode/skills", ".claude/skills", ".agents/skills"]
        .iter()
        .map(|r| project.join(r))
        .collect();
    if let Some(g) = crate::config::global_dir() {
        roots.push(g.join("skills"));
    }
    for r in roots {
        let Ok(rd) = std::fs::read_dir(&r) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path().join("SKILL.md")) else {
                continue;
            };
            let (n, d, body) = parse_frontmatter(&text);
            let name = n.unwrap_or_else(|| e.file_name().to_string_lossy().to_string());
            if found.iter().any(|s| s.name == name) {
                continue;
            } // earlier roots win; bundled can be overridden by none (stable)
            found.push(Skill {
                name,
                description: d.unwrap_or_default(),
                body,
                dir: Some(e.path()),
                embedded: &[],
            });
        }
    }
    found
}

impl Skill {
    /// Load `file` (relative, inside the skill dir) or the main body.
    pub fn file(&self, rel: Option<&str>) -> Result<String, String> {
        let Some(rel) = rel.filter(|r| !r.is_empty() && *r != "SKILL.md") else {
            return Ok(self.body.clone());
        };
        if rel.contains("..") || rel.starts_with('/') {
            return Err("file must be a relative path inside the skill".into());
        }
        if let Some((_, t)) = self.embedded.iter().find(|(p, _)| *p == rel) {
            return Ok((*t).to_string());
        }
        if let Some(d) = &self.dir {
            return std::fs::read_to_string(d.join(rel)).map_err(|e| format!("{rel}: {e}"));
        }
        Err(format!(
            "no such file in skill '{}': {rel}. Available: {}",
            self.name,
            self.embedded
                .iter()
                .map(|(p, _)| *p)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_skill_is_compact_and_complete() {
        let s = bundled();
        assert_eq!(s.name, "rustport");
        assert!(
            s.description.contains("Rust") && s.body.lines().count() < 120,
            "SKILL.md must stay a short router, has {} lines",
            s.body.lines().count()
        );
        for (p, t) in RUSTPORT_FILES {
            assert!(!t.trim().is_empty(), "{p} is empty");
        }
        assert!(s
            .file(Some("references/evidence.md"))
            .unwrap()
            .contains("observation"));
        assert!(s.file(Some("../x")).is_err() && s.file(Some("references/none.md")).is_err());
    }
    #[test]
    fn every_reference_linked_from_skill_md_exists_in_the_bundle() {
        let s = bundled();
        for l in s.body.split("](").skip(1) {
            let target = l.split(')').next().unwrap_or("");
            if target.ends_with(".md") {
                assert!(
                    s.file(Some(target)).is_ok(),
                    "SKILL.md links missing file {target}"
                );
            }
        }
    }
}
