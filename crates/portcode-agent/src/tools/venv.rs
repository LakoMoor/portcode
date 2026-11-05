//! Python virtual environment: create `.venv`, install packages into it, report its state.
//! The `bash` tool puts `.venv/bin` first on PATH while the setting is on, so `python` and `pip` just work.
use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::process::Command;

/// The virtual environment directory of a project.
pub fn dir(root: &Path) -> PathBuf {
    root.join(".venv")
}

/// The folder with the executables and the Python inside a venv (`bin/python`, or `Scripts\python.exe` on Windows).
pub fn layout(v: &Path) -> (PathBuf, PathBuf) {
    if cfg!(windows) {
        let b = v.join("Scripts");
        let p = b.join("python.exe");
        (b, p)
    } else {
        let b = v.join("bin");
        let p = b.join("python");
        (b, p)
    }
}

/// `VIRTUAL_ENV` and the new `PATH` for commands run in the project, when the venv exists and is enabled.
pub fn env_for(ctx: &Ctx) -> Option<(PathBuf, std::ffi::OsString)> {
    if !ctx.venv.load(Ordering::Relaxed) {
        return None;
    }
    let v = dir(&ctx.root);
    let (bin, py) = layout(&v);
    if !py.exists() {
        return None;
    }
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    Some((v, std::env::join_paths(paths).ok()?))
}

/// A pip requirement the tool will pass on: letters, digits and `._-[]=<>!~,+` only (no shell, no flags).
pub fn safe_requirement(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-[]=<>!~,+".contains(c))
}

async fn exec(
    ctx: &Ctx,
    prog: &Path,
    args: &[&str],
    secs: u64,
) -> Result<(bool, String), ToolError> {
    let mut c = Command::new(prog);
    c.args(args)
        .current_dir(&ctx.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .env("CI", "1");
    for k in &ctx.scrub_env {
        c.env_remove(k);
    }
    let child = c
        .spawn()
        .map_err(|e| ToolError(format!("cannot start {}: {e}", prog.display())))?;
    match tokio::time::timeout(Duration::from_secs(secs), child.wait_with_output()).await {
        Ok(Ok(o)) => {
            let mut t = String::from_utf8_lossy(&o.stdout).to_string();
            t += &String::from_utf8_lossy(&o.stderr);
            Ok((o.status.success(), t))
        }
        Ok(Err(e)) => Err(ToolError(format!("command failed: {e}"))),
        Err(_) => Err(ToolError(format!(
            "timed out after {secs}s. Install fewer packages at once."
        ))),
    }
}

pub struct Venv;
#[async_trait]
impl Tool for Venv {
    fn name(&self) -> String {
        "venv".into()
    }
    fn description(&self) -> String {
        "Python virtual environment in .venv. action=status (default), create, or install (packages). After create, bash runs with .venv first on PATH, so `python` and `pip` use it. Never install packages globally.".into()
    }
    fn example(&self) -> String {
        r#"{"action": "install", "packages": ["numpy", "pytest>=8"]}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"action": {"type":"string","enum":["status","create","install"]}, "packages": {"type":"array","items":{"type":"string"}}}),
            &[],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "venv",
            example: &self.example(),
        };
        let action = a.opt_str("action").unwrap_or_else(|| "status".into());
        let v = dir(&ctx.root);
        let py = layout(&v).1;
        let enabled = ctx.venv.load(Ordering::Relaxed);
        match action.as_str() {
            "status" => {
                let t = if py.exists() {
                    let (_, ver) = exec(ctx, &py, &["--version"], 20).await?;
                    format!(
                        ".venv exists ({}), bash uses it: {}",
                        ver.trim(),
                        if enabled {
                            "yes"
                        } else {
                            "no (venv setting is off)"
                        }
                    )
                } else {
                    format!(
                        "no .venv yet. Call venv with action=create{}.",
                        if enabled {
                            ""
                        } else {
                            " (the venv setting is off: ask the user to turn it on)"
                        }
                    )
                };
                Ok(Out::new("venv status", t))
            }
            "create" => {
                if !enabled {
                    return err("virtual environments are switched off (config \"venv\": false). Ask the user to turn them on.");
                }
                if py.exists() {
                    return Ok(Out::new("venv", ".venv already exists; nothing to do."));
                }
                ctx.permit(
                    "venv",
                    "create .venv",
                    Decision::Ask,
                    "create a Python virtual environment",
                )
                .await?;
                let sys = if which("python3") {
                    "python3"
                } else {
                    "python"
                };
                let (ok, out) = exec(ctx, Path::new(sys), &["-m", "venv", ".venv"], 120).await?;
                if !ok {
                    return err(format!(
                        "python -m venv failed: {}. Is Python 3 installed (and python3-venv on Debian/Ubuntu)?",
                        crate::tool::clip(out.trim(), 600)
                    ));
                }
                Ok(Out::new(
                    "venv",
                    "created .venv. bash now runs with it on PATH.",
                ))
            }
            "install" => {
                if !py.exists() {
                    return err("no .venv yet. Call venv with action=create first.");
                }
                let pk: Vec<String> = args
                    .get("packages")
                    .and_then(|p| p.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if pk.is_empty() {
                    return err(format!("packages is empty. Example: {}", self.example()));
                }
                if let Some(bad) = pk.iter().find(|p| !safe_requirement(p)) {
                    return err(format!(
                        "'{bad}' is not a plain requirement (name and optional version like pytest>=8). No flags or URLs."
                    ));
                }
                ctx.permit(
                    "venv",
                    &format!("pip install {}", pk.join(" ")),
                    Decision::Ask,
                    "install packages into .venv",
                )
                .await?;
                let mut argv = vec!["-m", "pip", "install", "--quiet"];
                argv.extend(pk.iter().map(String::as_str));
                let (ok, out) = exec(ctx, &py, &argv, 600).await?;
                let tail = crate::tool::clip(out.trim(), ctx.output_limit.min(3000));
                if ok {
                    Ok(Out::new(
                        "venv install",
                        format!("installed: {}\n{tail}", pk.join(" ")),
                    ))
                } else {
                    err(format!("pip install failed:\n{tail}"))
                }
            }
            other => err(format!(
                "unknown action '{other}': use status, create or install"
            )),
        }
    }
}

fn which(p: &str) -> bool {
    crate::engines::which(p).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requirements_are_plain() {
        assert!(safe_requirement("pytest>=8"));
        assert!(safe_requirement("requests[socks]==2.32.0"));
        assert!(!safe_requirement("--index-url=http://x"));
        assert!(!safe_requirement("a; rm -rf /"));
        assert!(!safe_requirement("git+https://x/y"));
        assert!(!safe_requirement(""));
    }
}
