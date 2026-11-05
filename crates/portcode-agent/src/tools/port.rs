//! Porting tools: the rustport workflow as callable, evidence-producing tools.
use super::shell::run_command_with;
use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use portcode_evidence::{sha256_hex, Confidence, Evidence, Kind, Pin, Source};
use serde_json::{json, Value};
use std::time::Duration;

fn kind(s: &str) -> Result<Kind, ToolError> {
    match s.to_lowercase().as_str() {
        "observation" => Ok(Kind::Observation),
        "inference" => Ok(Kind::Inference),
        "unknown" => Ok(Kind::Unknown),
        o => err(format!(
            "kind must be observation, inference or unknown (got '{o}')"
        )),
    }
}
fn conf(s: Option<String>) -> Confidence {
    match s.as_deref() {
        Some("high") => Confidence::High,
        Some("low") => Confidence::Low,
        _ => Confidence::Medium,
    }
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub struct Survey;
#[async_trait]
impl Tool for Survey {
    fn name(&self) -> String {
        "survey".into()
    }
    fn description(&self) -> String {
        "Inventory a source tree before porting: languages/LOC, build files, engine and graphics-API hints, assets, tests/CI, risk hotspots (globals, raw pointers, threads, RNG).".into()
    }
    fn example(&self) -> String {
        r#"{"path": "."}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path": {"type":"string","description":"Directory to survey"}}),
            &[],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "survey",
            example: &self.example(),
        };
        let p = ctx
            .path(&a.opt_str("path").unwrap_or_else(|| ".".into()), false)
            .await?;
        if !p.is_dir() {
            return err(format!("{} is not a directory", ctx.rel(&p)));
        }
        let shown = ctx.rel(&p);
        let mut s =
            tokio::task::spawn_blocking(move || portcode_port::survey::run(&p, 50_000)).await?;
        s.root = if shown.is_empty() { ".".into() } else { shown }; // never leak absolute paths into transcripts
        Ok(Out::new("survey", s.render()))
    }
}

pub struct PinTool;
#[async_trait]
impl Tool for PinTool {
    fn name(&self) -> String {
        "pin".into()
    }
    fn description(&self) -> String {
        "Pin the exact original build (sha256, size, format, version) to .portcode/reference.lock.json and verify it later. Every fact about the original is only valid for the pinned build. Pinning proves file identity only.".into()
    }
    fn example(&self) -> String {
        r#"{"path": "original/game.exe", "version": "1.0.3"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path": {"type":"string"}, "version": {"type":"string"}}),
            &["path"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "pin",
            example: &self.example(),
        };
        let p = ctx.path(&a.str("path")?, false).await?;
        let pin = Pin::of(&p, a.opt_str("version"))?;
        let lock = ctx.root.join(".portcode/reference.lock.json");
        let mut pins: Vec<Pin> = std::fs::read_to_string(&lock)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        pins.retain(|x| x.path != pin.path);
        pins.push(pin.clone());
        if let Some(d) = lock.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&lock, serde_json::to_string_pretty(&pins)?)?;
        let ev = ctx.ledger.add(Evidence {
            id: String::new(),
            kind: Kind::Observation,
            subject: "original.build".into(),
            statement: format!(
                "{} is {} ({} bytes), sha256 {}{}",
                ctx.rel(&p),
                pin.format.label(),
                pin.size,
                pin.sha256,
                pin.version
                    .as_deref()
                    .map(|v| format!(", version {v}"))
                    .unwrap_or_default()
            ),
            confidence: Confidence::High,
            source: Source {
                tool: "pin".into(),
                path: Some(ctx.rel(&p)),
                sha256: Some(pin.sha256.clone()),
                build: pin.version.clone(),
                ..Default::default()
            },
            depends_on: vec![],
            attachments: vec![],
            resolved_by: None,
            created: 0,
        })?;
        Ok(Out::new(
            "pin",
            format!(
                "Pinned {} → {}\nformat: {}\nsize: {}\nsha256: {}\nrecorded as {}",
                ctx.rel(&p),
                ".portcode/reference.lock.json",
                pin.format.label(),
                pin.size,
                pin.sha256,
                ev.id
            ),
        ))
    }
}

pub struct EvidenceAdd;
#[async_trait]
impl Tool for EvidenceAdd {
    fn name(&self) -> String {
        "evidence_add".into()
    }
    fn description(&self) -> String {
        "Record one fact about the original in the evidence ledger. kind=observation (you saw it: run output, bytes, screenshot), inference (derived: static reading, decompiler, reasoning), unknown (a gap that blocks the port). Never label static reading as observation. Returns the id (E-0001) to cite in docs/PORT.md.".into()
    }
    fn example(&self) -> String {
        r#"{"kind": "observation", "subject": "cli.args", "statement": "`app --help` exits 0 and prints 12 lines", "confidence": "high", "tool": "bash", "args": "./app --help"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"kind": {"type":"string","enum":["observation","inference","unknown"]}, "subject": {"type":"string","description":"feature/subsystem, dotted: physics.brake"}, "statement": {"type":"string"}, "confidence": {"type":"string","enum":["low","medium","high"]}, "tool": {"type":"string"}, "args": {"type":"string"}, "path": {"type":"string"}, "depends_on": {"type":"array","items":{"type":"string"}}, "attachments": {"type":"array","items":{"type":"string"},"description":"files under docs/screenshots, e.g. screenshots/help.svg"}}),
            &["kind", "subject", "statement"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "evidence_add",
            example: &self.example(),
        };
        let e = ctx.ledger.add(Evidence {
            id: String::new(),
            kind: kind(&a.str("kind")?)?,
            subject: a.str("subject")?,
            statement: a.str("statement")?,
            confidence: conf(a.opt_str("confidence")),
            source: Source {
                tool: a.opt_str("tool").unwrap_or_else(|| "agent".into()),
                args: a.opt_str("args").unwrap_or_default(),
                path: a.opt_str("path"),
                sha256: None,
                build: None,
            },
            depends_on: strs(&args, "depends_on"),
            attachments: strs(&args, "attachments"),
            resolved_by: None,
            created: 0,
        })?;
        Ok(Out::new(
            e.id.clone(),
            format!(
                "Recorded {} ({:?}, {:?}) for {}.",
                e.id, e.kind, e.confidence, e.subject
            ),
        ))
    }
}

pub struct EvidenceList;
#[async_trait]
impl Tool for EvidenceList {
    fn name(&self) -> String {
        "evidence_list".into()
    }
    fn description(&self) -> String {
        "List ledger records, optionally by subject prefix or only open unknowns. Check this before re-investigating something already mapped.".into()
    }
    fn example(&self) -> String {
        r#"{"subject": "physics", "unknowns_only": false}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"subject": {"type":"string"}, "unknowns_only": {"type":"boolean"}}),
            &[],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "evidence_list",
            example: &self.example(),
        };
        let mut v = if a.bool("unknowns_only") {
            ctx.ledger.open_unknowns()?
        } else {
            ctx.ledger.all()?
        };
        if let Some(s) = a.opt_str("subject") {
            v.retain(|e| e.subject.starts_with(&s));
        }
        if v.is_empty() {
            return Ok(Out::new("evidence", "No records."));
        }
        let n = v.len();
        let mut text: String = v
            .iter()
            .rev()
            .take(60)
            .rev()
            .map(|e| {
                format!(
                    "{} [{:?}/{:?}] {}: {}",
                    e.id, e.kind, e.confidence, e.subject, e.statement
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if n > 60 {
            text += &format!("\n[showing last 60 of {n}; filter by subject]");
        }
        Ok(Out::new("evidence", text))
    }
}

pub struct Parity;
#[async_trait]
impl Tool for Parity {
    fn name(&self) -> String {
        "parity".into()
    }
    fn description(&self) -> String {
        "Compare golden outputs recorded from the ORIGINAL with the port's outputs (numbers within tolerance, PNG per-channel). Missing, extra or empty = failure. Never loosen tolerance to pass; justify it in docs/PORT.md.".into()
    }
    fn example(&self) -> String {
        r#"{"golden": "tests/golden", "port": "tests/out", "abs_tol": 1e-5, "rel_tol": 1e-4}"#
            .into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"golden": {"type":"string"}, "port": {"type":"string"}, "abs_tol": {"type":"number"}, "rel_tol": {"type":"number"}}),
            &["golden", "port"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "parity",
            example: &self.example(),
        };
        let (g, p) = (
            ctx.path(&a.str("golden")?, false).await?,
            ctx.path(&a.str("port")?, false).await?,
        );
        let (at, rt) = (
            a.opt_f64("abs_tol").unwrap_or(1e-5),
            a.opt_f64("rel_tol").unwrap_or(1e-4),
        );
        let r = tokio::task::spawn_blocking(move || portcode_port::parity::compare(&g, &p, at, rt))
            .await?
            .map_err(ToolError)?;
        let text = format!(
            "{}{}",
            r.render(),
            if r.passed() {
                "RESULT: PASS"
            } else {
                "RESULT: FAIL"
            }
        );
        Ok(Out::new(
            if r.passed() {
                "parity PASS"
            } else {
                "parity FAIL"
            },
            text,
        ))
    }
}

pub struct Screenshot;
#[async_trait]
impl Tool for Screenshot {
    fn name(&self) -> String {
        "screenshot".into()
    }
    fn description(&self) -> String {
        "Run a command and save its real output as a terminal screenshot (SVG) in docs/screenshots/, then record it as an observation in the ledger with the output's sha256. Use it to back every ported feature with proof. For GUI apps, capture a PNG with the OS or an MCP tool, save it to docs/screenshots/, and attach it with evidence_add.".into()
    }
    fn example(&self) -> String {
        r#"{"command": "./target/release/app --help", "name": "app-help", "subject": "cli.help", "statement": "port prints the same usage as the original"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"command": {"type":"string"}, "name": {"type":"string","description":"file stem: letters, digits, - _"}, "subject": {"type":"string"}, "statement": {"type":"string"}}),
            &["command", "name", "subject"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "screenshot",
            example: &self.example(),
        };
        let (cmd, name) = (a.str("command")?, a.str("name")?);
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return err("name must be letters, digits, '-' or '_' only");
        }
        ctx.permit("bash", cmd.trim(), Decision::Ask, "capture screenshot")
            .await?;
        let r = run_command_with(ctx, &cmd, Duration::from_secs(60), a.bool("color")).await?;
        let svg = portcode_port::shot::render_svg(
            &format!(
                "{name} — exit {}",
                r.code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
            ),
            &cmd,
            &r.out,
            110,
            48,
        );
        let rel = format!("screenshots/{name}.svg");
        let path = ctx.root.join("docs").join(&rel);
        std::fs::create_dir_all(path.parent().ok_or("bad path")?)?;
        std::fs::write(&path, &svg)?;
        let sha = sha256_hex(r.out.as_bytes());
        let st = a.opt_str("statement").unwrap_or_else(|| {
            format!(
                "`{cmd}` exits {} with the captured output",
                r.code.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
            )
        });
        let e = ctx.ledger.add(Evidence {
            id: String::new(),
            kind: Kind::Observation,
            subject: a.str("subject")?,
            statement: st,
            confidence: Confidence::High,
            source: Source {
                tool: "screenshot".into(),
                args: cmd,
                path: None,
                sha256: Some(sha),
                build: None,
            },
            depends_on: vec![],
            attachments: vec![rel.clone()],
            resolved_by: None,
            created: 0,
        })?;
        Ok(Out::new(
            name,
            format!(
                "Saved docs/{rel} and recorded {}. Exit code {:?}.{}",
                e.id,
                r.code,
                if r.timed_out { " (timed out)" } else { "" }
            ),
        ))
    }
}

pub struct SiteBuild;
#[async_trait]
impl Tool for SiteBuild {
    fn name(&self) -> String {
        "site_build".into()
    }
    fn description(&self) -> String {
        "Generate the GitHub Pages site (docs/site/) from docs/PORT.md, the evidence ledger and docs/screenshots/.".into()
    }
    fn example(&self) -> String {
        r#"{"title": "My Port"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"title": {"type":"string"}, "out": {"type":"string","description":"default docs/site"}, "repo_url": {"type":"string"}}),
            &[],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "site_build",
            example: &self.example(),
        };
        let out = ctx
            .path(
                &a.opt_str("out").unwrap_or_else(|| "docs/site".into()),
                true,
            )
            .await?;
        ctx.permit("edit", &ctx.rel(&out), Decision::Ask, "generate site")
            .await?;
        let (root, title, repo) = (
            ctx.root.clone(),
            a.opt_str("title").unwrap_or_else(|| "Rust port".into()),
            a.opt_str("repo_url"),
        );
        let o2 = out.clone();
        let files = tokio::task::spawn_blocking(move || {
            portcode_port::site::generate(&root, &o2, &title, repo.as_deref())
        })
        .await?
        .map_err(ToolError)?;
        Ok(Out::new(
            "site",
            format!(
                "Wrote {} to {}. Preview: open {}/index.html",
                files.join(", "),
                ctx.rel(&out),
                ctx.rel(&out)
            ),
        ))
    }
}

pub struct Scaffold;
#[async_trait]
impl Tool for Scaffold {
    fn name(&self) -> String {
        "scaffold".into()
    }
    fn description(&self) -> String {
        "Create a Rust workspace for a port: <name>-core (headless) + <name> (binary), docs/PORT.md, tests/golden, docs/screenshots. Created inside the project folder (dest is relative to it; default <name>-rs). Refuses to overwrite existing files.".into()
    }
    fn example(&self) -> String {
        r#"{"name": "mygame", "dest": "mygame-rs"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"name": {"type":"string"}, "dest": {"type":"string"}}),
            &["name"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "scaffold",
            example: &self.example(),
        };
        let name = a.str("name")?;
        let dest = ctx
            .path(
                &a.opt_str("dest").unwrap_or_else(|| format!("{name}-rs")),
                true,
            )
            .await?;
        ctx.permit("edit", &ctx.rel(&dest), Decision::Ask, "scaffold workspace")
            .await?;
        let made = portcode_port::scaffold::create(&name, &dest).map_err(ToolError)?;
        Ok(Out::new(
            "scaffold",
            format!(
                "Created {} files:\n{}",
                made.len(),
                made.iter()
                    .map(|m| ctx.rel(std::path::Path::new(m)))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        ))
    }
}

#[cfg(test)]
mod where_tests {
    use super::*;
    use crate::permission::{AutoGate, Reply, Rules};
    use std::sync::Arc;

    #[tokio::test]
    async fn scaffold_creates_the_workspace_inside_the_opened_folder() {
        let root = std::env::temp_dir().join(format!("pc-scaf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ctx = Ctx::new(
            root.clone(),
            Arc::new(AutoGate(Reply::Once)),
            Rules::default(),
        );
        Scaffold.run(&ctx, json!({"name": "game"})).await.unwrap();
        assert!(
            root.join("game-rs/docs/PORT.md").is_file(),
            "default dest is <name>-rs in the project"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
