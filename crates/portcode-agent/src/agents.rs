//! Agent definitions and system-prompt assembly.
//!
//! Prompts are tuned for open-weight models: numbered one-line rules, an explicit tool protocol, a worked example,
//! the strict rules at the start and a short style block at the end, and a prompt prefix that is byte-stable across
//! turns (the only per-day change is the date line at the very end) so llama.cpp / vLLM / SGLang prefix caches hit.
use crate::skills::Skill;
use portcode_llm::ModelProfile;
use std::path::Path;

const COMMON: &str = include_str!("../prompts/common.md");
const FULL_EXTRA: &str = include_str!("../prompts/full_extra.md");

#[derive(Debug, Clone)]
pub struct AgentDef {
    pub name: &'static str,
    pub description: &'static str,
    pub primary: bool,
    pub readonly: bool,
    /// `None` = every tool (including MCP tools).
    pub tools: Option<&'static [&'static str]>,
    pub mcp: bool,
    pub steps: u32,
    /// Finish gate: the loop refuses to stop while the work is visibly unfinished (see `Runner::gaps`).
    pub gate: bool,
    prompt: &'static str,
    embed_skill: bool,
}

const READ: &[&str] = &[
    "read",
    "ls",
    "glob",
    "grep",
    "skill",
    "todowrite",
    "webfetch",
    "task",
    "bash",
    "survey",
    "evidence_list",
];
const EXPLORE: &[&str] = &[
    "read",
    "ls",
    "glob",
    "grep",
    "bash",
    "survey",
    "evidence_list",
];
const REVERSE: &[&str] = &[
    "re_status",
    "re_install",
    "disasm",
    "emulate",
    "decompile",
    "hexdump",
    "read",
    "ls",
    "glob",
    "grep",
    "skill",
    "todowrite",
    "webfetch",
    "task",
    "bash",
    "survey",
    "pin",
    "evidence_add",
    "evidence_list",
    "screenshot",
];
const GENERAL: &[&str] = &[
    "read",
    "ls",
    "glob",
    "grep",
    "skill",
    "todowrite",
    "webfetch",
    "bash",
    "write",
    "edit",
    "survey",
    "pin",
    "evidence_add",
    "evidence_list",
    "screenshot",
    "parity",
];

pub fn all() -> Vec<AgentDef> {
    vec![
        AgentDef {
            name: "build",
            description: "Default agent: full access for implementing changes",
            primary: true,
            readonly: false,
            tools: None,
            mcp: true,
            steps: 60,
            gate: false,
            prompt: include_str!("../prompts/build.md"),
            embed_skill: false,
        },
        AgentDef {
            name: "port",
            description:
                "Port a program to Rust with evidence, goldens, screenshots and a site (rustport)",
            primary: true,
            readonly: false,
            tools: None,
            mcp: true,
            steps: 150,
            gate: true,
            prompt: include_str!("../prompts/port.md"),
            embed_skill: true,
        },
        AgentDef {
            name: "reverse",
            description: "Study a binary/app/format and record evidence; no code changes",
            primary: true,
            readonly: false,
            tools: Some(REVERSE),
            mcp: true,
            steps: 80,
            gate: false,
            prompt: include_str!("../prompts/reverse.md"),
            embed_skill: true,
        },
        AgentDef {
            name: "plan",
            description: "Read-only: investigate and produce a plan",
            primary: true,
            readonly: true,
            tools: Some(READ),
            mcp: false,
            steps: 40,
            gate: false,
            prompt: include_str!("../prompts/plan.md"),
            embed_skill: false,
        },
        AgentDef {
            name: "explore",
            description: "Sub-agent: fast read-only search",
            primary: false,
            readonly: true,
            tools: Some(EXPLORE),
            mcp: false,
            steps: 25,
            gate: false,
            prompt: include_str!("../prompts/explore.md"),
            embed_skill: false,
        },
        AgentDef {
            name: "general",
            description: "Sub-agent: self-contained job with edits",
            primary: false,
            readonly: false,
            tools: Some(GENERAL),
            mcp: true,
            steps: 40,
            gate: false,
            prompt: include_str!("../prompts/general.md"),
            embed_skill: false,
        },
    ]
}

pub fn find(name: &str) -> Option<AgentDef> {
    all().into_iter().find(|a| a.name == name)
}

impl AgentDef {
    /// May this agent see tool `name`? `builtin` says whether the name is one of PortCode's own tools.
    pub fn allows(&self, name: &str, builtin: bool) -> bool {
        match self.tools {
            None => true,
            Some(l) => {
                if builtin {
                    l.contains(&name)
                } else {
                    self.mcp
                }
            }
        }
    }
}

fn civil_date(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn read_capped(p: &Path) -> Option<String> {
    let t = std::fs::read_to_string(p).ok()?;
    Some(if t.len() > 8000 {
        format!("{}\n[truncated]", t.chars().take(8000).collect::<String>())
    } else {
        t
    })
}

/// Project/global instruction files: first of AGENTS.md, PORTCODE.md, CLAUDE.md in the project root, plus the global one.
pub fn instructions(root: &Path) -> String {
    let mut out = String::new();
    if let Some(g) = crate::config::global_dir().and_then(|d| read_capped(&d.join("AGENTS.md"))) {
        out += &format!("## Global instructions\n{g}\n");
    }
    for n in ["AGENTS.md", "PORTCODE.md", "CLAUDE.md"] {
        if let Some(t) = read_capped(&root.join(n)) {
            out += &format!("## {n}\n{t}\n");
            break;
        }
    }
    out
}

pub fn system_prompt(
    agent: &AgentDef,
    profile: &ModelProfile,
    root: &Path,
    skills: &[Skill],
    deep: bool,
    now_secs: u64,
) -> String {
    let mut s = String::from(COMMON);
    if profile.prompt_style == "full" {
        s.push_str(FULL_EXTRA);
    }
    s.push_str(agent.prompt);
    if agent.embed_skill {
        if let Some(sk) = skills.iter().find(|k| k.name == "rustport") {
            s.push_str(&sk.body);
        }
        s.push_str("\n</rustport-skill>\n");
    }
    let listed: Vec<String> = skills
        .iter()
        .filter(|k| !(agent.embed_skill && k.name == "rustport"))
        .map(|k| {
            format!(
                "- {}: {}",
                k.name,
                k.description.chars().take(160).collect::<String>()
            )
        })
        .collect();
    if !listed.is_empty() && agent.allows("skill", true) {
        s += &format!(
            "\n# Skills (load with the `skill` tool)\n{}\n",
            listed.join("\n")
        );
    }
    let ins = instructions(root);
    if !ins.is_empty() {
        s += &format!("\n# Project instructions\n{ins}");
    }
    s += &format!(
        "\n# Environment\n- project: {}\n- platform: {}\n- agent: {}\n- mode: {}\n- date: {}\n",
        root.display(),
        std::env::consts::OS,
        agent.name,
        if deep {
            "deep (may look inside the original with disasm/emulate/decompile/hexdump)"
        } else {
            "clean-room (observe the original from outside only)"
        },
        civil_date(now_secs)
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_are_correct() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_709_164_800), "2024-02-29");
        assert_eq!(civil_date(1_793_750_400), "2026-11-04");
    }
    #[test]
    fn prompt_prefix_is_stable_and_complete() {
        let p = ModelProfile::default();
        let skills = vec![crate::skills::bundled()];
        let d = std::path::Path::new("/nonexistent-project");
        let a = system_prompt(&find("port").unwrap(), &p, d, &skills, false, 0);
        let b = system_prompt(&find("port").unwrap(), &p, d, &skills, false, 86_400 * 5);
        let cut = a.find("- date:").unwrap();
        assert_eq!(
            &a[..cut],
            &b[..cut],
            "everything before the date line must be byte-identical across days"
        );
        assert!(
            a.contains("Hard rules")
                && a.contains("</rustport-skill>")
                && a.contains("evidence_add")
        );
        assert!(
            system_prompt(&find("build").unwrap(), &p, d, &skills, false, 0).contains("# Skills")
        );
    }
    #[test]
    fn prompts_stay_small_enough_for_small_contexts() {
        let p = ModelProfile::default();
        let s = system_prompt(
            &find("port").unwrap(),
            &p,
            std::path::Path::new("/x"),
            &[crate::skills::bundled()],
            false,
            0,
        );
        assert!(
            s.len() < 12_000,
            "port prompt is {} bytes (~{} tokens)",
            s.len(),
            s.len() / 4
        );
    }
    #[test]
    fn readonly_agents_cannot_see_write_tools() {
        for n in ["plan", "explore"] {
            let a = find(n).unwrap();
            assert!(
                a.readonly
                    && !a.allows("write", true)
                    && !a.allows("edit", true)
                    && !a.allows("scaffold", true)
            );
        }
        assert!(find("build").unwrap().allows("anything", false));
        assert!(!find("explore").unwrap().allows("rea_open_binary", false));
    }
}
