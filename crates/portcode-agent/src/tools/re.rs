//! Deep-mode reverse-engineering tools. All except `re_status`/`re_install` are refused in clean-room mode.
use super::shell::run_command;
use crate::engines::{self, sh_quote};
use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

const NOTE: &str = "\n\nRecord what this shows with evidence_add (kind=inference, cite this tool and the address/offset). Confirm behaviour against the running original before claiming parity.";

pub struct ReStatus;
#[async_trait]
impl Tool for ReStatus {
    fn name(&self) -> String {
        "re_status".into()
    }
    fn description(&self) -> String {
        "List the reverse-engineering engines (Capstone, Unicorn, Ghidra, ImHex, rizin…), whether each is installed, and the current mode (clean-room or deep).".into()
    }
    fn example(&self) -> String {
        "{}".into()
    }
    fn schema(&self) -> Value {
        obj(json!({}), &[])
    }
    async fn run(&self, ctx: &Ctx, _args: Value) -> ToolResult {
        let mode = if ctx.is_deep() { "deep" } else { "clean-room" };
        Ok(Out::new(
            "re_status",
            format!(
                "mode: {mode}\ninstall folder: {}\n\n{}\nInstall one with re_install {{\"engine\":\"unicorn\"}} (asks the user).",
                engines::dir().display(),
                engines::table()
            ),
        ))
    }
}

pub struct ReInstall;
#[async_trait]
impl Tool for ReInstall {
    fn name(&self) -> String {
        "re_install".into()
    }
    fn description(&self) -> String {
        "Install a reverse-engineering engine into PortCode's engines folder (pip packages into a private venv; Ghidra and ImHex from their official releases, sha256-verified). Always asks the user first. Engine ids: see re_status.".into()
    }
    fn example(&self) -> String {
        r#"{"engine": "unicorn"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"engine": {"type":"string","description":"capstone, unicorn, keystone, lief, yara, angr, frida, ghidra, imhex; or 'core' for capstone+unicorn+lief"}}),
            &["engine"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "re_install",
            example: &self.example(),
        };
        let id = a.str("engine")?;
        let list: Vec<&engines::Engine> = if id == "core" {
            engines::ENGINES.iter().filter(|e| e.core).collect()
        } else {
            vec![engines::find(&id).ok_or_else(|| {
                ToolError(format!(
                    "unknown engine '{id}'. Call re_status for the list."
                ))
            })?]
        };
        let mut log = String::new();
        for e in list {
            if engines::detect(e).is_some() {
                log.push_str(&format!("{}: already installed\n", e.id));
                continue;
            }
            let script = engines::install_script(e).map_err(ToolError)?;
            ctx.permit(
                "re_install",
                e.id,
                Decision::Ask,
                &format!(
                    "download and install {} ({}) into {}",
                    e.id,
                    e.license,
                    engines::dir().display()
                ),
            )
            .await?;
            let r = run_command(ctx, &script, Duration::from_secs(600)).await?;
            let ok = r.code == Some(0) && !r.timed_out;
            let tail = clip(r.out.trim(), 1500);
            log.push_str(&format!(
                "{}: {}\n{tail}\n",
                e.id,
                if ok { "installed" } else { "FAILED" }
            ));
            if r.timed_out {
                log.push_str("timed out; run re_install again (downloads resume)\n");
            }
        }
        Ok(Out::new("re_install", log))
    }
}

async fn py_helper(ctx: &Ctx, script: &str, req: &Value, engine: &str) -> Result<Value, ToolError> {
    let py = engines::venv_python();
    if !py.is_file() {
        return err(format!("{engine} is not installed. Call re_install {{\"engine\":\"{engine}\"}} first (it asks the user)."));
    }
    let dir = engines::write_helpers().map_err(ToolError)?;
    let cmd = format!(
        "{} -I {} {}",
        sh_quote(&py.display().to_string()),
        sh_quote(&dir.join(script).display().to_string()),
        sh_quote(&req.to_string())
    );
    let r = run_command(ctx, &cmd, Duration::from_secs(60)).await?;
    if r.timed_out {
        return err("timed out after 60s; lower max_insns/length");
    }
    let line = r
        .out
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'));
    match line.and_then(|l| serde_json::from_str::<Value>(l).ok()) {
        Some(v) => match v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty() && script == "disasm.py") {
            Some(e) => err(e.to_string()),
            None => Ok(v),
        },
        None => err(format!(
            "{engine} helper failed: {}. If a module is missing, re_install {{\"engine\":\"{engine}\"}}.",
            clip(r.out.trim(), 600)
        )),
    }
}

/// Fill `file`/`offset`/`length` from args after resolving the path inside the project.
async fn with_file(ctx: &Ctx, a: &Args<'_>, mut req: Value) -> Result<Value, ToolError> {
    if let Some(h) = a.opt_str("hex") {
        req["hex"] = h.into();
    } else {
        let p = ctx.path(&a.str("file")?, false).await?;
        req["file"] = p.display().to_string().into();
    }
    for k in [
        "offset",
        "length",
        "base",
        "count",
        "start",
        "end",
        "max_insns",
        "trace",
    ] {
        if let Some(v) = a.v.get(k).and_then(|v| v.as_u64()) {
            req[k] = v.into();
        }
    }
    Ok(req)
}

pub struct Disasm;
#[async_trait]
impl Tool for Disasm {
    fn name(&self) -> String {
        "disasm".into()
    }
    fn description(&self) -> String {
        "Deep mode. Disassemble bytes of a file (or a hex string) with Capstone. arch: x86 x64 arm thumb arm64 mips mipsel riscv32 riscv64 ppc64. Use the file offset, not the virtual address; set base to label addresses.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/app", "arch": "x64", "offset": 4096, "length": 128, "base": 4096}"#
            .into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"hex":{"type":"string","description":"bytes as hex instead of file"},"arch":{"type":"string"},"offset":{"type":"integer"},"length":{"type":"integer","description":"bytes, default 256"},"base":{"type":"integer"},"count":{"type":"integer","description":"max instructions, default 200"}}),
            &["arch"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("disasm")?;
        let a = Args {
            v: &args,
            tool: "disasm",
            example: &self.example(),
        };
        let req = with_file(ctx, &a, json!({"arch": a.str("arch")?})).await?;
        let v = py_helper(ctx, "disasm.py", &req, "capstone").await?;
        let lines: Vec<&str> = v["lines"]
            .as_array()
            .map(|l| l.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        Ok(Out::new(
            "disasm",
            format!("{}\n{NOTE}", clip(&lines.join("\n"), ctx.output_limit)),
        ))
    }
}

pub struct Emulate;
#[async_trait]
impl Tool for Emulate {
    fn name(&self) -> String {
        "emulate".into()
    }
    fn description(&self) -> String {
        "Deep mode. Run machine code in the Unicorn CPU emulator (isolated: no syscalls, no host access, hard instruction budget). Load bytes from a file range or hex at `base`, set registers, run from `start` to `end`, then read registers and memory. Use it to learn what a function computes, then check against the original's real output.".into()
    }
    fn example(&self) -> String {
        r#"{"hex": "4801d8c3", "arch": "x64", "base": 4096, "regs": {"rax": 2, "rbx": 40}, "end": 4099, "show_regs": ["rax"]}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"hex":{"type":"string"},"arch":{"type":"string","description":"x86 x64 arm thumb arm64 mips mipsel riscv64"},"offset":{"type":"integer"},"length":{"type":"integer"},"base":{"type":"integer","description":"load address, default 0x100000"},"start":{"type":"integer"},"end":{"type":"integer","description":"stop address (exclusive); default end of the code"},"regs":{"type":"object","description":"register name -> integer"},"maps":{"type":"array","description":"extra memory: [{addr,size,hex?}]"},"dump":{"type":"array","description":"[{addr,len}] memory to return"},"show_regs":{"type":"array"},"max_insns":{"type":"integer","description":"default 100000"},"trace":{"type":"integer","description":"return the first N executed instructions"}}),
            &["arch"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("emulate")?;
        let a = Args {
            v: &args,
            tool: "emulate",
            example: &self.example(),
        };
        let mut req = with_file(ctx, &a, json!({"arch": a.str("arch")?})).await?;
        for k in ["regs", "maps", "dump", "show_regs"] {
            if let Some(v) = args.get(k) {
                req[k] = v.clone();
            }
        }
        let v = py_helper(ctx, "emulate.py", &req, "unicorn").await?;
        if let Some(e) = v
            .get("error")
            .and_then(|e| e.as_str())
            .filter(|e| !e.is_empty())
        {
            if v.get("instructions").is_none() {
                return err(e.to_string());
            }
        }
        let mut s = format!("instructions executed: {}\n", v["instructions"]);
        if let Some(e) = v["error"].as_str() {
            s.push_str(&format!("stopped by emulator error: {e}\n"));
        }
        s.push_str(&format!("registers: {}\n", v["regs"]));
        if let Some(t) = v["trace"].as_array().filter(|t| !t.is_empty()) {
            s.push_str("trace:\n");
            for l in t.iter().filter_map(|x| x.as_str()) {
                s.push_str(&format!("  {l}\n"));
            }
        }
        if let Some(d) = v["dump"].as_array().filter(|d| !d.is_empty()) {
            s.push_str(&format!("memory: {}\n", Value::Array(d.clone())));
        }
        Ok(Out::new(
            "emulate",
            format!("{}{NOTE}", clip(&s, ctx.output_limit)),
        ))
    }
}

pub struct Decompile;
#[async_trait]
impl Tool for Decompile {
    fn name(&self) -> String {
        "decompile".into()
    }
    fn description(&self) -> String {
        "Deep mode. Ghidra headless: mode=list names functions, mode=func decompiles one (query = name or address), mode=strings lists defined strings. First call analyses the binary (can take minutes; later calls reuse the project). Decompiler output is evidence to read, never code to paste into the port.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/app", "mode": "func", "query": "main"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"mode":{"type":"string","description":"list | func | strings"},"query":{"type":"string","description":"function name or address for mode=func; max count for list/strings"}}),
            &["file", "mode"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("decompile")?;
        let a = Args {
            v: &args,
            tool: "decompile",
            example: &self.example(),
        };
        let mode = a.str("mode")?;
        if !["list", "func", "strings"].contains(&mode.as_str()) {
            return err("mode must be list, func or strings");
        }
        let query = a.opt_str("query").unwrap_or_default();
        if mode == "func" && query.is_empty() {
            return err("mode=func needs query (function name or address). Call mode=list first.");
        }
        let file = ctx.path(&a.str("file")?, false).await?;
        let Some(gh) = engines::ghidra_headless() else {
            return err("Ghidra is not installed. Call re_install {\"engine\":\"ghidra\"} (needs a JDK; asks the user), or use disasm for a quick look.");
        };
        let scripts = engines::write_helpers().map_err(ToolError)?;
        // Ghidra rejects project paths with a "." element, so projects live in the engines folder, not in .portcode
        let proj = engines::dir().join("projects");
        std::fs::create_dir_all(&proj)?;
        let out = proj.join("out.txt");
        let _ = std::fs::remove_file(&out);
        let name = format!(
            "p{}",
            &portcode_evidence::sha256_hex(file.display().to_string().as_bytes())[..12]
        );
        let have = proj.join(format!("{name}.gpr")).exists();
        let q = |p: &std::path::Path| sh_quote(&p.display().to_string());
        let step = if have {
            format!(
                "-process {} -noanalysis",
                sh_quote(
                    &file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                )
            )
        } else {
            format!("-import {}", q(&file))
        };
        let cmd = format!(
            "{} {} {} {step} -scriptPath {} -postScript PcDecompile.java {} {} {} 2>&1 | tail -n 25",
            q(&gh), q(&proj), name, q(&scripts), q(&out), mode, sh_quote(&query)
        );
        let r = run_command(ctx, &cmd, Duration::from_secs(580)).await?;
        match std::fs::read_to_string(&out) {
            Ok(t) => Ok(Out::new(
                format!("decompile {mode}"),
                format!("{}{NOTE}", clip(t.trim_end(), ctx.output_limit)),
            )),
            Err(_) => err(format!(
                "Ghidra produced no output{}. Last log lines:\n{}",
                if r.timed_out {
                    " (timed out after ~10 min; the project is kept, call again to continue)"
                } else {
                    ""
                },
                clip(r.out.trim(), 1500)
            )),
        }
    }
}

pub struct Hexdump;
#[async_trait]
impl Tool for Hexdump {
    fn name(&self) -> String {
        "hexdump".into()
    }
    fn description(&self) -> String {
        "Deep mode. Look at raw bytes: mode=hex (offset/length), strings (printable runs, min 6), entropy (per-block, finds packed/encrypted regions). Pair with an ImHex .hexpat pattern to document a file format.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "inputs/save.dat", "mode": "hex", "offset": 0, "length": 128}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"mode":{"type":"string","description":"hex | strings | entropy"},"offset":{"type":"integer"},"length":{"type":"integer","description":"default 256 (hex), 4 MiB cap otherwise"}}),
            &["file"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("hexdump")?;
        let a = Args {
            v: &args,
            tool: "hexdump",
            example: &self.example(),
        };
        let p = ctx.path(&a.str("file")?, false).await?;
        let mode = a.opt_str("mode").unwrap_or_else(|| "hex".into());
        let off = a.opt_u64("offset").unwrap_or(0) as usize;
        let cap = if mode == "hex" { 4096 } else { 4 << 20 };
        let len = (a
            .opt_u64("length")
            .unwrap_or(if mode == "hex" { 256 } else { cap as u64 }) as usize)
            .min(cap);
        let all = std::fs::read(&p)?;
        let data = all.get(off.min(all.len())..).unwrap_or(&[]);
        let data = data.get(..len.min(data.len())).unwrap_or(data);
        let text = match mode.as_str() {
            "hex" => data
                .chunks(16)
                .enumerate()
                .map(|(i, c)| {
                    let hex: Vec<String> = c.iter().map(|b| format!("{b:02x}")).collect();
                    let asc: String = c
                        .iter()
                        .map(|&b| {
                            if (0x20..0x7f).contains(&b) {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect();
                    format!("{:08x}  {:<47}  |{asc}|", off + i * 16, hex.join(" "))
                })
                .collect::<Vec<_>>()
                .join("\n"),
            "strings" => {
                let (mut out, mut cur, mut start) = (Vec::new(), String::new(), 0usize);
                for (i, &b) in data.iter().enumerate() {
                    if (0x20..0x7f).contains(&b) {
                        if cur.is_empty() {
                            start = i;
                        }
                        cur.push(b as char);
                    } else {
                        if cur.len() >= 6 {
                            out.push(format!("{:08x}  {cur}", off + start));
                        }
                        cur.clear();
                    }
                }
                if cur.len() >= 6 {
                    out.push(format!("{:08x}  {cur}", off + start));
                }
                out.join("\n")
            }
            "entropy" => data
                .chunks(4096)
                .enumerate()
                .map(|(i, c)| {
                    let mut f = [0usize; 256];
                    for &b in c {
                        if let Some(x) = f.get_mut(b as usize) {
                            *x += 1;
                        }
                    }
                    let n = c.len() as f64;
                    let h: f64 = f
                        .iter()
                        .filter(|&&x| x > 0)
                        .map(|&x| {
                            let p = x as f64 / n;
                            -p * p.log2()
                        })
                        .sum();
                    format!(
                        "{:08x}  {h:.2} bits/byte {}",
                        off + i * 4096,
                        if h > 7.5 {
                            "← packed or encrypted?"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return err("mode must be hex, strings or entropy"),
        };
        Ok(Out::new(
            format!("hexdump {mode}"),
            clip(&text, ctx.output_limit),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{AutoGate, Reply, Rules};
    use std::sync::Arc;

    fn ctx(deep: bool) -> Ctx {
        let c = Ctx::new(
            std::env::temp_dir(),
            Arc::new(AutoGate(Reply::Once)),
            Rules::default(),
        );
        c.deep.store(deep, std::sync::atomic::Ordering::Relaxed);
        c
    }
    #[tokio::test]
    async fn clean_room_refuses_every_inside_tool() {
        let c = ctx(false);
        for (t, a) in [
            (
                Box::new(Disasm) as Box<dyn Tool>,
                json!({"hex":"90","arch":"x64"}),
            ),
            (Box::new(Emulate), json!({"hex":"90","arch":"x64"})),
            (Box::new(Decompile), json!({"file":"x","mode":"list"})),
            (Box::new(Hexdump), json!({"file":"x"})),
        ] {
            let e = t.run(&c, a).await.unwrap_err().0;
            assert!(e.contains("clean-room") && e.contains("--deep"), "{e}");
        }
        assert!(ReStatus
            .run(&c, json!({}))
            .await
            .unwrap()
            .text
            .contains("clean-room"));
    }
    #[tokio::test]
    async fn hexdump_entropy_and_strings() {
        let c = ctx(true);
        let d = std::env::temp_dir().join(format!("pc-hex-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("f.bin"), b"\x00\x01hello world\x00\xff").unwrap();
        let mut c2 = Ctx::new(d.clone(), Arc::new(AutoGate(Reply::Once)), Rules::default());
        c2.deep = c.deep;
        let s = Hexdump
            .run(&c2, json!({"file":"f.bin","mode":"strings"}))
            .await
            .unwrap()
            .text;
        assert!(s.contains("hello world"), "{s}");
        let h = Hexdump
            .run(&c2, json!({"file":"f.bin"}))
            .await
            .unwrap()
            .text;
        assert!(h.starts_with("00000000  00 01 68"), "{h}");
        let _ = std::fs::remove_dir_all(d);
    }
}
