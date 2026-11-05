use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const CAP: usize = 1_000_000;

pub struct Ran {
    pub code: Option<i32>,
    pub out: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

async fn drain<R: AsyncReadExt + Unpin>(mut r: R) -> Vec<u8> {
    let (mut keep, mut buf) = (Vec::new(), vec![0u8; 16384]);
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        if keep.len() < CAP {
            keep.extend_from_slice(buf.get(..n).unwrap_or(&[]));
        } // keep draining so the child never blocks on a full pipe
    }
    keep
}

/// A POSIX `sh`: always on macOS and Linux; on Windows the one that ships with Git for Windows, if installed.
pub fn posix_sh() -> Option<std::path::PathBuf> {
    #[cfg(not(windows))]
    {
        Some("sh".into())
    }
    #[cfg(windows)]
    {
        let mut c: Vec<std::path::PathBuf> = std::env::var_os("PATH")
            .map(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join("sh.exe"))
                    .collect()
            })
            .unwrap_or_default();
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(b) = std::env::var_os(var) {
                let b = std::path::PathBuf::from(b);
                c.push(b.join("Git").join("usr").join("bin").join("sh.exe"));
                c.push(
                    b.join("Programs")
                        .join("Git")
                        .join("usr")
                        .join("bin")
                        .join("sh.exe"),
                );
            }
        }
        c.into_iter().find(|p| p.is_file())
    }
}

/// `sh -c cmd`; on Windows without Git's sh, `cmd.exe /C cmd`.
fn shell_command(cmd: &str) -> Command {
    match posix_sh() {
        Some(sh) => {
            let mut c = Command::new(sh);
            c.arg("-c").arg(cmd);
            c
        }
        None => {
            let mut c = Command::new("cmd.exe");
            c.arg("/C").arg(cmd);
            c
        }
    }
}

/// Kill a process and everything it started.
async fn kill_tree(pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

/// Like [`run_command`] for scripts written in POSIX shell (the engine tools). On Windows this needs Git for Windows.
pub async fn run_posix(ctx: &Ctx, cmd: &str, timeout: Duration) -> Result<Ran, ToolError> {
    if posix_sh().is_none() {
        return err("this tool runs through a POSIX shell. On Windows install Git for Windows (it provides sh.exe: `winget install Git.Git`) or use WSL, then try again.");
    }
    run_command(ctx, cmd, timeout).await
}

/// Run `sh -c cmd` non-interactively: stdin closed, own process group, wall-clock deadline, capped output,
/// whole group killed on timeout/cancel. A missing shell or spawn failure is an `Err`, never a panic.
pub async fn run_command(ctx: &Ctx, cmd: &str, timeout: Duration) -> Result<Ran, ToolError> {
    run_command_with(ctx, cmd, timeout, false).await
}

/// Like [`run_command`]; `color` keeps terminal colours (for screenshots of colourful tools).
pub async fn run_command_with(
    ctx: &Ctx,
    cmd: &str,
    timeout: Duration,
    color: bool,
) -> Result<Ran, ToolError> {
    let mut c = shell_command(cmd);
    c.current_dir(&ctx.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("CI", "1")
        .env("PAGER", "cat")
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(unix)]
    c.process_group(0);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashes
    if color {
        c.env("PORTCODE_FORCE_COLOR", "1")
            .env("CLICOLOR_FORCE", "1")
            .env("TERM", "xterm-256color");
    } else {
        c.env("TERM", "dumb").env("NO_COLOR", "1");
    }
    if let Some((v, path)) = super::venv::env_for(ctx) {
        c.env("VIRTUAL_ENV", v)
            .env("PATH", path)
            .env_remove("PYTHONHOME");
    }
    for k in &ctx.scrub_env {
        c.env_remove(k);
    }
    let mut child = c
        .spawn()
        .map_err(|e| ToolError(format!("cannot start shell: {e}")))?;
    let pid = child.id();
    let (so, se) = (child.stdout.take(), child.stderr.take());
    let (ho, he) = (
        tokio::spawn(async move {
            match so {
                Some(s) => drain(s).await,
                None => vec![],
            }
        }),
        tokio::spawn(async move {
            match se {
                Some(s) => drain(s).await,
                None => vec![],
            }
        }),
    );
    let start = std::time::Instant::now();
    let (mut timed_out, mut cancelled) = (false, false);
    let status = loop {
        tokio::select! {
            s = child.wait() => break s.ok(),
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                if ctx.cancelled() { cancelled = true; } else if start.elapsed() >= timeout { timed_out = true; }
                if cancelled || timed_out {
                    if let Some(p) = pid { kill_tree(p).await; }
                    let _ = child.kill().await;
                    break child.wait().await.ok();
                }
            }
        }
    };
    let (o, e) = (ho.await.unwrap_or_default(), he.await.unwrap_or_default());
    let mut out = String::from_utf8_lossy(&o).to_string();
    if !e.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out += &String::from_utf8_lossy(&e);
    }
    Ok(Ran {
        code: status.and_then(|s| s.code()),
        out,
        timed_out,
        cancelled,
    })
}

pub struct Bash;
#[async_trait]
impl Tool for Bash {
    fn name(&self) -> String {
        "bash".into()
    }
    fn description(&self) -> String {
        if posix_sh().is_some() {
            "Run one shell command in the project directory (non-interactive, no stdin, default 120s timeout). Use for builds, tests, git, inspecting binaries. Do not use it to read/search files — use read/grep/glob.".into()
        } else {
            "Run one command in the project directory through cmd.exe (Windows; non-interactive, no stdin, default 120s timeout). Use cmd syntax (dir, type, &&), not bash. For builds, tests, git. Do not use it to read/search files — use read/grep/glob.".into()
        }
    }
    fn example(&self) -> String {
        r#"{"command": "cargo test -p mycrate", "description": "run unit tests"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"command": {"type":"string"}, "description": {"type":"string","description":"5-10 words: what this does"}, "timeout_ms": {"type":"integer","description":"max 600000"}}),
            &["command"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "bash",
            example: &self.example(),
        };
        let cmd = a.str("command")?;
        if cmd.trim().is_empty() {
            return err("command is empty");
        }
        let t = Duration::from_millis(
            a.opt_u64("timeout_ms")
                .unwrap_or(120_000)
                .clamp(1_000, 600_000),
        );
        if !ctx.is_deep() {
            if let Some(w) = inspects_binaries(&cmd) {
                return err(format!("`{w}` looks inside programs, which clean-room mode forbids. Observe the original from outside (run it, feed inputs, compare outputs) or ask the user to switch to deep mode (--deep or /deep)."));
            }
        }
        ctx.permit(
            "bash",
            cmd.trim(),
            Decision::Ask,
            &a.opt_str("description").unwrap_or_default(),
        )
        .await?;
        let r = run_command(ctx, &cmd, t).await?;
        let mut text = clip(r.out.trim_end(), ctx.output_limit);
        if r.timed_out {
            text += &format!(
                "\n[killed: exceeded {}s. Run long jobs in smaller steps or raise timeout_ms.]",
                t.as_secs()
            );
        } else if r.cancelled {
            text += "\n[cancelled by user]";
        } else {
            text += &format!(
                "\n[exit code {}]",
                r.code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            );
        }
        Ok(Out::new(
            a.opt_str("description")
                .unwrap_or_else(|| cmd.chars().take(60).collect()),
            text,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{AutoGate, Reply, Rules};
    use std::sync::Arc;
    fn ctx(d: &std::path::Path) -> Ctx {
        Ctx::new(
            d.to_path_buf(),
            Arc::new(AutoGate(Reply::Once)),
            Rules::default(),
        )
    }

    #[tokio::test]
    async fn runs_captures_and_reports_exit_code() {
        let d = tempfile::tempdir().unwrap();
        let o = Bash
            .run(
                &ctx(d.path()),
                json!({"command":"echo out; echo err 1>&2; exit 3"}),
            )
            .await
            .unwrap();
        assert!(
            o.text.contains("out") && o.text.contains("err") && o.text.contains("[exit code 3]"),
            "{}",
            o.text
        );
    }
    #[tokio::test]
    async fn stdin_is_closed_and_timeout_kills_group() {
        let d = tempfile::tempdir().unwrap();
        let c = ctx(d.path());
        let o = Bash
            .run(&c, json!({"command":"cat; echo done"}))
            .await
            .unwrap(); // would hang if stdin were inherited
        assert!(o.text.contains("done"));
        let t = std::time::Instant::now();
        let o = Bash
            .run(
                &c,
                json!({"command":"sleep 30 & sleep 30","timeout_ms":1000}),
            )
            .await
            .unwrap();
        assert!(o.text.contains("killed") && t.elapsed() < Duration::from_secs(8));
    }
    #[tokio::test]
    async fn rejected_and_dangerous_commands_do_not_run() {
        let d = tempfile::tempdir().unwrap();
        let c = Ctx::new(
            d.path().to_path_buf(),
            Arc::new(AutoGate(Reply::Reject)),
            Rules::default(),
        );
        assert!(Bash.run(&c, json!({"command":"touch x"})).await.is_err());
        assert!(!d.path().join("x").exists());
        assert!(Bash
            .run(&ctx(d.path()), json!({"command":"sudo ls"}))
            .await
            .is_err());
        // read-only commands skip the prompt entirely
        assert!(Bash.run(&c, json!({"command":"ls"})).await.is_ok());
    }
    #[tokio::test]
    async fn output_is_clipped() {
        let d = tempfile::tempdir().unwrap();
        let o = Bash
            .run(&ctx(d.path()), json!({"command":"yes x | head -c 200000"}))
            .await
            .unwrap();
        assert!(o.text.contains("omitted") && o.text.len() < 14_000);
    }
}

/// Commands that read the inside of a program; refused in clean-room mode.
const INSPECTORS: &[&str] = &[
    "objdump",
    "otool",
    "nm",
    "strings",
    "xxd",
    "hexdump",
    "ilspycmd",
    "monodis",
    "ildasm",
    "Il2CppDumper",
    "od",
    "readelf",
    "gdb",
    "lldb",
    "r2",
    "radare2",
    "rizin",
    "rz-bin",
    "ghidra",
    "analyzeHeadless",
    "ghidraRun",
    "retdec-decompiler",
    "binwalk",
    "jadx",
    "wasm2wat",
    "wasm-decompile",
    "ilspycmd",
    "monodis",
    "dumpbin",
    "llvm-objdump",
    "llvm-nm",
    "diec",
    "capa",
];

fn inspects_binaries(cmd: &str) -> Option<&'static str> {
    // the command word of each pipeline segment (after env assignments and sudo/env/time/xargs wrappers)
    cmd.split(|c: char| ";&|\n(`".contains(c)).find_map(|seg| {
        let w = seg.split_whitespace().find(|w| {
            !w.contains('=')
                && !["sudo", "env", "time", "xargs", "nice", "exec", "command"].contains(w)
        })?;
        let base = w.rsplit(['/', '\\']).next().unwrap_or(w);
        let base = base.strip_suffix(".exe").unwrap_or(base);
        INSPECTORS.iter().find(|i| **i == base).copied()
    })
}

#[cfg(test)]
mod clean_room_tests {
    use super::inspects_binaries;
    #[test]
    fn flags_inspectors_only() {
        assert_eq!(inspects_binaries("objdump -d a.out"), Some("objdump"));
        assert_eq!(
            inspects_binaries("cat x | /usr/bin/strings"),
            Some("strings")
        );
        assert_eq!(inspects_binaries("cargo test && ./wordstat in.txt"), None);
        assert_eq!(inspects_binaries("grep nm file"), None);
    }
}
