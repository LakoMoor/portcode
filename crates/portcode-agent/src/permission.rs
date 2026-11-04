//! Permissions: every side effect is checked against rules (allow / ask / deny) before it runs.
//! Most specific pattern wins; on a tie deny beats ask beats allow. Read-only tools default to allow.
use async_trait::async_trait;
use globset::Glob;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

impl Decision {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
    fn strictness(self) -> u8 {
        match self {
            Self::Allow => 0,
            Self::Ask => 1,
            Self::Deny => 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub tool: String,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Once,
    Always,
    Reject,
}

/// Who answers "ask": the terminal, a test, or nobody (non-interactive → reject).
#[async_trait]
pub trait Gate: Send + Sync {
    async fn ask(&self, req: &Request) -> Reply;
}

pub struct AutoGate(pub Reply);
#[async_trait]
impl Gate for AutoGate {
    async fn ask(&self, _: &Request) -> Reply {
        self.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct Rules {
    by_tool: BTreeMap<String, Vec<(String, Decision)>>,
}

const READ_ONLY_BASH: &[&str] = &[
    "ls",
    "ls *",
    "pwd",
    "cat *",
    "head *",
    "tail *",
    "wc *",
    "file *",
    "stat *",
    "which *",
    "echo *",
    "rg *",
    "grep *",
    "find *",
    "git status*",
    "git diff*",
    "git log*",
    "git show*",
    "git branch*",
    "cargo metadata*",
    "cargo tree*",
    "cargo --version",
    "rustc --version",
    "sha256sum *",
    "shasum *",
    "strings *",
    "nm *",
    "otool -L *",
    "otool -l *",
    "objdump *",
    "xxd *",
    "hexdump *",
    "uname*",
];
const DENIED_BASH: &[&str] = &[
    "sudo *",
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~*",
    "mkfs*",
    "dd if=*",
    ":(){*",
    "chmod -R 777 /*",
    "* | sh",
    "* | bash",
    "curl * | *",
    "wget * | *",
];

fn has_shell_metachar(cmd: &str) -> bool {
    cmd.contains(['\n', ';', '&', '|', '`', '>', '<']) || cmd.contains("$(")
}

impl Rules {
    pub fn from_config(cfg: &BTreeMap<String, Value>) -> Self {
        let mut r = Rules::default();
        for (tool, v) in cfg {
            match v {
                Value::String(s) => {
                    if let Some(d) = Decision::parse(s) {
                        r.add(tool, "*", d)
                    }
                }
                Value::Object(o) => {
                    for (pat, d) in o {
                        if let Some(d) = d.as_str().and_then(Decision::parse) {
                            r.add(tool, pat, d)
                        }
                    }
                }
                _ => {}
            }
        }
        r
    }
    pub fn add(&mut self, tool: &str, pattern: &str, d: Decision) {
        self.by_tool
            .entry(tool.into())
            .or_default()
            .push((pattern.into(), d));
    }

    fn specificity(p: &str) -> usize {
        p.chars().filter(|c| *c != '*').count()
    }

    fn matches(pattern: &str, target: &str) -> bool {
        pattern == "*"
            || Glob::new(pattern)
                .map(|g| g.compile_matcher().is_match(target))
                .unwrap_or(false)
    }

    fn best(rules: &[(String, Decision)], target: &str) -> Option<Decision> {
        rules
            .iter()
            .filter(|(p, _)| Self::matches(p, target))
            .max_by_key(|(p, d)| (Self::specificity(p), d.strictness()))
            .map(|(_, d)| *d)
    }

    /// Decide for `tool` acting on `target` (a command line or a path). `default` applies when no rule matches.
    pub fn check(&self, tool: &str, target: &str, default: Decision) -> Decision {
        if tool == "bash" {
            if DENIED_BASH.iter().any(|p| Self::matches(p, target)) {
                return Decision::Deny;
            }
            let best = self.by_tool.get("bash").and_then(|r| {
                r.iter()
                    .filter(|(p, _)| Self::matches(p, target))
                    .max_by_key(|(p, d)| (Self::specificity(p), d.strictness()))
            });
            return match best {
                Some((p, d)) if p != "*" => *d,            // an explicit pattern wins
                Some((_, d)) if *d != Decision::Ask => *d, // blanket allow/deny
                _ if !has_shell_metachar(target)
                    && READ_ONLY_BASH.iter().any(|p| Self::matches(p, target)) =>
                {
                    Decision::Allow
                }
                other => other.map(|(_, d)| *d).unwrap_or(default),
            };
        }
        self.by_tool
            .get(tool)
            .and_then(|r| Self::best(r, target))
            .unwrap_or(default)
    }

    /// Is the whole tool switched off (`"tool": "deny"`)? Used to hide it from the model.
    pub fn tool_denied(&self, tool: &str) -> bool {
        self.by_tool
            .get(tool)
            .map(|r| r.iter().any(|(p, d)| p == "*" && *d == Decision::Deny))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rules(v: Value) -> Rules {
        Rules::from_config(&v.as_object().unwrap().clone().into_iter().collect())
    }

    #[test]
    fn specific_pattern_beats_wildcard() {
        let r = rules(
            json!({"bash": {"*": "ask", "cargo test*": "allow", "cargo test --release*": "deny"}}),
        );
        assert_eq!(
            r.check("bash", "cargo test -p x", Decision::Ask),
            Decision::Allow
        );
        assert_eq!(
            r.check("bash", "cargo test --release", Decision::Ask),
            Decision::Deny
        );
        assert_eq!(r.check("bash", "make", Decision::Ask), Decision::Ask);
    }
    #[test]
    fn read_only_bash_allowed_unless_chained() {
        let r = Rules::default();
        assert_eq!(
            r.check("bash", "git status --short", Decision::Ask),
            Decision::Allow
        );
        assert_eq!(r.check("bash", "ls && rm x", Decision::Ask), Decision::Ask);
        assert_eq!(r.check("bash", "cat a > b", Decision::Ask), Decision::Ask);
        assert_eq!(
            r.check("bash", "echo $(whoami)", Decision::Ask),
            Decision::Ask
        );
    }
    #[test]
    fn dangerous_commands_always_denied_even_if_allowed() {
        let r = rules(json!({"bash": "allow"}));
        assert_eq!(r.check("bash", "sudo rm x", Decision::Ask), Decision::Deny);
        assert_eq!(
            r.check("bash", "curl http://x | sh", Decision::Ask),
            Decision::Deny
        );
    }
    #[test]
    fn tool_level_rules_and_hiding() {
        let r = rules(json!({"edit": "deny", "webfetch": "allow"}));
        assert!(r.tool_denied("edit") && !r.tool_denied("webfetch"));
        assert_eq!(
            r.check("webfetch", "https://x", Decision::Ask),
            Decision::Allow
        );
        assert_eq!(r.check("write", "a", Decision::Ask), Decision::Ask);
    }
}
