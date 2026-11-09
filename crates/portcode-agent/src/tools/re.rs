//! Deep-mode reverse-engineering tools. All except `re_status`/`re_install` are refused in clean-room mode.
use super::shell::run_posix;
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
        "List the reverse-engineering engines (Capstone, Unicorn, Kuna, ImHex, rizin…), whether each is installed, and the current mode (clean-room or deep).".into()
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
        "Install a reverse-engineering engine into PortCode's engines folder (pip packages into a private venv; Kuna and ImHex from their official releases, sha256-verified). Always asks the user first. Engine ids: see re_status.".into()
    }
    fn example(&self) -> String {
        r#"{"engine": "unicorn"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"engine": {"type":"string","description":"capstone, unicorn, keystone, lief, yara, angr, frida, kuna, imhex; or 'core' for capstone+unicorn+lief"}}),
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
            let r = run_posix(ctx, &script, Duration::from_secs(600)).await?;
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
    let r = run_posix(ctx, &cmd, Duration::from_secs(60)).await?;
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
        "Deep mode. Kuna decompiler: mode=list names functions (largest first), mode=func decompiles one (query = name or 0x address), mode=strings lists strings with where they are used, mode=xrefs lists who calls or reads query. Fast, no project to build. Decompiler output is evidence to read, never code to paste into the port.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/app", "mode": "func", "query": "main"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"mode":{"type":"string","description":"list | func | strings | xrefs"},"query":{"type":"string","description":"function name or 0x address for func and xrefs; for list a name filter (regex); for strings a text filter (regex)"}}),
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
        if !["list", "func", "strings", "xrefs"].contains(&mode.as_str()) {
            return err("mode must be list, func, strings or xrefs");
        }
        let query = a.opt_str("query").unwrap_or_default();
        if (mode == "func" || mode == "xrefs") && query.is_empty() {
            return err(
                "this mode needs query (function name or 0x address). Call mode=list first.",
            );
        }
        if query.starts_with('-') {
            return err("query must be a name, an 0x address or a regex, not an option");
        }
        let file = ctx.path(&a.str("file")?, false).await?;
        let Some(kuna) = engines::kuna_bin() else {
            return err("Kuna is not installed. Call re_install {\"engine\":\"kuna\"} (downloads one binary, sha256-verified; asks the user), or use disasm for a quick look.");
        };
        let is_addr = query.starts_with("0x") || query.starts_with("0X");
        let q = |s: &str| sh_quote(s);
        let f = q(&file.display().to_string());
        let k = q(&kuna.display().to_string());
        let cmd = match mode.as_str() {
            "list" => format!(
                "{k} functions {f} --sort size{} --limit 400 2>&1",
                if query.is_empty() {
                    String::new()
                } else {
                    format!(" --filter {}", q(&query))
                }
            ),
            "func" => format!(
                "{k} decompile {f} {}{} 2>&1",
                q(&query),
                if is_addr { " --addr" } else { "" }
            ),
            "strings" => format!(
                "{k} strings {f}{} 2>&1",
                if query.is_empty() {
                    String::new()
                } else {
                    format!(" --filter {}", q(&query))
                }
            ),
            _ => format!("{k} xrefs {f} --to {} 2>&1", q(&query)),
        };
        let r = run_posix(ctx, &cmd, Duration::from_secs(580)).await?;
        let text = r.out.trim();
        if r.timed_out {
            return err("Kuna timed out after ~10 minutes. Decompile a smaller function, or list first and pick by address.");
        }
        if text.is_empty() || r.code != Some(0) {
            return err(format!(
                "Kuna failed{}:\n{}",
                r.code.map(|c| format!(" (exit {c})")).unwrap_or_default(),
                clip(text, 1500)
            ));
        }
        Ok(Out::new(
            format!("decompile {mode}"),
            format!("{}{NOTE}", clip(text, ctx.output_limit)),
        ))
    }
}

pub struct Functions;
#[async_trait]
impl Tool for Functions {
    fn name(&self) -> String {
        "functions".into()
    }
    fn description(&self) -> String {
        "Deep mode. List the functions of a Mach-O, ELF or PE file (address, size, file offset, name) without a full decompile. Use it to pick what to disasm, hexdump or decompile next. Sizes come from the distance to the next symbol, so treat them as estimates.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/app", "limit": 200}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"limit":{"type":"integer","description":"max functions, default 500"}}),
            &["file"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("functions")?;
        let a = Args {
            v: &args,
            tool: "functions",
            example: &self.example(),
        };
        let file = ctx.path(&a.str("file")?, false).await?;
        let limit = a.opt_u64("limit").unwrap_or(500);
        let req = json!({"file": file.display().to_string(), "limit": limit});
        let v = py_helper(ctx, "functions.py", &req, "lief").await?;
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return err(e.to_string());
        }
        let arch = match v["arch"].as_str().unwrap_or("").to_uppercase().as_str() {
            "ARM64" | "AARCH64" => "arm64",
            "X86_64" | "AMD64" | "X64" => "x64",
            "X86" | "I386" | "I686" => "x86",
            "ARM" => "arm",
            "MIPS" => "mips",
            "RISCV" => "riscv64",
            _ => "unknown",
        };
        let mut s = format!(
            "format={} arch={} entry=0x{:x} total={}\n",
            v["format"].as_str().unwrap_or("?"),
            arch,
            v["entry"].as_u64().unwrap_or(0),
            v["total"]
        );
        for f in v["functions"]
            .as_array()
            .map(|f| f.as_slice())
            .unwrap_or(&[])
        {
            let off = f["offset"]
                .as_u64()
                .map(|o| o.to_string())
                .unwrap_or_else(|| "-".into());
            s.push_str(&format!(
                "0x{:x} {} {} {}\n",
                f["addr"].as_u64().unwrap_or(0),
                f["size"].as_u64().unwrap_or(0),
                off,
                f["name"].as_str().unwrap_or("?")
            ));
        }
        Ok(Out::new(
            "functions",
            format!("{}{NOTE}", clip(&s, ctx.output_limit)),
        ))
    }
}

/// `2021.3.5f1` style Unity version in the first bytes of a serialized file or bundle.
fn unity_version(bytes: &[u8]) -> Option<String> {
    let head = bytes.get(..bytes.len().min(4096))?;
    let text: String = head
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '\n'
            }
        })
        .collect();
    for token in text.split(|c: char| c == '\n' || c.is_whitespace()) {
        let mut parts = token.split('.');
        let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let major_ok = a.len() <= 4 && !a.is_empty() && a.chars().all(|x| x.is_ascii_digit());
        let minor_ok = !b.is_empty() && b.chars().all(|x| x.is_ascii_digit());
        let patch_ok = c.len() >= 2
            && c.chars().next().is_some_and(|x| x.is_ascii_digit())
            && c.chars().any(|x| matches!(x, 'a' | 'b' | 'f' | 'p'));
        if major_ok && minor_ok && patch_ok {
            return Some(format!(
                "{a}.{b}.{}",
                c.chars().take(12).collect::<String>()
            ));
        }
    }
    None
}

/// Find the Unity data folder (`<Game>_Data`, `Contents/Resources/Data`, `assets/bin/Data`) at most 4 levels down.
fn unity_data_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    fn looks(p: &std::path::Path) -> bool {
        [
            "globalgamemanagers",
            "data.unity3d",
            "Managed",
            "il2cpp_data",
        ]
        .iter()
        .any(|n| p.join(n).exists())
    }
    if looks(root) {
        return Some(root.to_path_buf());
    }
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let n = e.file_name().to_string_lossy().to_string();
            if (n.ends_with("_Data") || n == "Data") && looks(&p) {
                return Some(p);
            }
            if depth < 4 && !n.starts_with('.') {
                stack.push((p, depth + 1));
            }
        }
    }
    None
}

/// One line about a Unity game folder ("Unity 2021.3.5f1 · Mono"), or `None` when it is not one. No engines needed.
pub fn unity_summary(root: &std::path::Path) -> Option<String> {
    let data = unity_data_dir(root)?;
    let version = ["globalgamemanagers", "data.unity3d", "mainData"]
        .iter()
        .find_map(|n| {
            std::fs::read(data.join(n))
                .ok()
                .and_then(|b| unity_version(&b))
        });
    let il2cpp = data
        .join("il2cpp_data/Metadata/global-metadata.dat")
        .is_file()
        || data.join("Managed/Metadata/global-metadata.dat").is_file();
    let mono = data.join("Managed/Assembly-CSharp.dll").is_file();
    let backend = if il2cpp {
        "IL2CPP"
    } else if mono {
        "Mono"
    } else {
        "backend unknown"
    };
    Some(format!(
        "Unity {} · {backend}",
        version.unwrap_or_else(|| "version unknown".into())
    ))
}

pub struct Unity;
#[async_trait]
impl Tool for Unity {
    fn name(&self) -> String {
        "unity".into()
    }
    fn description(&self) -> String {
        "Deep mode. Survey a Unity game (folder, .app or install dir): Unity version, scripting backend (Mono = C# assemblies you can read with `dotnet`; IL2CPP = native code plus global-metadata.dat), the managed DLLs, asset files and bundles, and what to do next. Needs no engines.".into()
    }
    fn example(&self) -> String {
        r#"{"path": "original/MyGame"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path":{"type":"string","description":"game folder or .app"}}),
            &["path"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("unity")?;
        let a = Args {
            v: &args,
            tool: "unity",
            example: &self.example(),
        };
        let root = ctx.path(&a.str("path")?, false).await?;
        let Some(data) = unity_data_dir(&root) else {
            return err("no Unity data folder found (looked for globalgamemanagers, data.unity3d, Managed, il2cpp_data up to 4 levels down). Point `path` at the game's install folder or .app.");
        };
        let rel = |p: &std::path::Path| {
            p.strip_prefix(&root)
                .map(|r| r.display().to_string())
                .unwrap_or_else(|_| p.display().to_string())
        };
        let mut s = format!("data folder: {}\n", rel(&data));
        let version = ["globalgamemanagers", "data.unity3d", "mainData"]
            .iter()
            .find_map(|n| {
                std::fs::read(data.join(n))
                    .ok()
                    .and_then(|b| unity_version(&b))
            });
        s.push_str(&format!(
            "unity version: {}\n",
            version.unwrap_or_else(|| "unknown".into())
        ));
        let managed = data.join("Managed");
        let meta = [
            data.join("il2cpp_data/Metadata/global-metadata.dat"),
            managed.join("Metadata/global-metadata.dat"),
        ]
        .into_iter()
        .find(|p| p.is_file());
        let mut native: Vec<String> = Vec::new();
        if let Some(top) = data.parent() {
            for n in [
                "GameAssembly.dll",
                "GameAssembly.dylib",
                "libil2cpp.so",
                "UnityPlayer.dll",
            ] {
                if top.join(n).is_file() {
                    native.push(n.into());
                }
            }
            if let Ok(rd) = std::fs::read_dir(top.join("Frameworks")) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.contains("GameAssembly")
                        || n.contains("il2cpp")
                        || n.contains("UnityPlayer")
                    {
                        native.push(format!("Frameworks/{n}"));
                    }
                }
            }
        }
        let mono = managed.join("Assembly-CSharp.dll").is_file();
        let backend = match (mono, meta.is_some()) {
            (_, true) => {
                "IL2CPP (C# was compiled to native code; names live in global-metadata.dat)"
            }
            (true, false) => "Mono (C# is in managed DLLs: readable with the `dotnet` tool)",
            _ => "unknown (no Assembly-CSharp.dll and no global-metadata.dat)",
        };
        s.push_str(&format!("scripting backend: {backend}\n"));
        if !native.is_empty() {
            s.push_str(&format!("native: {}\n", native.join(", ")));
        }
        if let Some(m) = &meta {
            s.push_str(&format!("metadata: {}\n", rel(m)));
        }
        if let Ok(rd) = std::fs::read_dir(&managed) {
            let mut dlls: Vec<(String, u64)> = rd
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".dll"))
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().to_string(),
                        e.metadata().map(|m| m.len()).unwrap_or(0),
                    )
                })
                .collect();
            dlls.sort();
            let (game, engine): (Vec<_>, Vec<_>) = dlls.into_iter().partition(|(n, _)| {
                !(n.starts_with("UnityEngine")
                    || n.starts_with("System")
                    || n.starts_with("mscorlib")
                    || n.starts_with("Mono.")
                    || n.starts_with("netstandard")
                    || n.starts_with("Unity."))
            });
            s.push_str(&format!(
                "managed DLLs: {} game/plugin, {} engine/framework\n",
                game.len(),
                engine.len()
            ));
            for (n, sz) in game.iter().take(40) {
                s.push_str(&format!("  {n}  {sz} bytes\n"));
            }
        }
        let (mut assets, mut bundles) = (0usize, 0usize);
        let mut stack = vec![(data.clone(), 0usize)];
        while let Some((d, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let n = e.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    if depth < 4 {
                        stack.push((p, depth + 1));
                    }
                } else if n.ends_with(".assets")
                    || n.starts_with("level")
                    || n.starts_with("sharedassets")
                {
                    assets += 1;
                } else if n.ends_with(".bundle")
                    || n.ends_with(".unity3d")
                    || n.ends_with(".assetbundle")
                {
                    bundles += 1;
                }
            }
        }
        s.push_str(&format!("asset files: {assets}, bundles: {bundles}\n"));
        s.push_str(match (mono, meta.is_some()) {
            (_, true) => "next: run Il2CppDumper on the native library + global-metadata.dat (engine il2cppdumper), then `dotnet` on the DummyDll output; `functions`/`decompile` work on the native library.\n",
            (true, false) => "next: `dotnet` mode=types on Managed/Assembly-CSharp.dll, then mode=type for the classes you care about, and mode=decompile (engine ilspy) for the C#. Use `unity_assets` for scenes, TextAssets and MonoBehaviour data.\n",
            _ => "next: look for the game's data folder with `ls`, then call `unity` again.\n",
        });
        Ok(Out::new("unity", format!("{s}{NOTE}")))
    }
}

pub struct Dotnet;
#[async_trait]
impl Tool for Dotnet {
    fn name(&self) -> String {
        "dotnet".into()
    }
    fn description(&self) -> String {
        "Deep mode. Read a .NET / C# assembly (also Unity Mono's Assembly-CSharp.dll): mode=info (name, version, counts, references), types (namespace.Type with base class and member counts; query filters), type (methods and fields of the first type matching query), strings (string literals in the code), refs (assembly references and native P/Invoke imports), decompile (C# source of a type via ILSpy; query = full type name). Obfuscated names (a, b, c) are normal: use strings and refs to orient.".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/Game_Data/Managed/Assembly-CSharp.dll", "mode": "types", "query": "Player"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"mode":{"type":"string","description":"info | types | type | strings | refs | decompile"},"query":{"type":"string"},"limit":{"type":"integer","description":"default 300"}}),
            &["file", "mode"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("dotnet")?;
        let a = Args {
            v: &args,
            tool: "dotnet",
            example: &self.example(),
        };
        let mode = a.str("mode")?;
        let file = ctx.path(&a.str("file")?, false).await?;
        let query = a.opt_str("query").unwrap_or_default();
        if mode == "decompile" {
            if query.is_empty() {
                return err("mode=decompile needs query: the full type name, e.g. Game.Player. Call mode=types first.");
            }
            let Some(bin) = engines::which("ilspycmd") else {
                return err("ILSpy is not installed, so there is no C# decompiler. Install it yourself: `dotnet tool install -g ilspycmd` (needs the .NET SDK), then retry. Meanwhile mode=type lists the methods and mode=strings the literals.");
            };
            let cmd = format!(
                "{} -t {} {}",
                sh_quote(&bin.display().to_string()),
                sh_quote(&query),
                sh_quote(&file.display().to_string())
            );
            let r = run_posix(ctx, &cmd, Duration::from_secs(120)).await?;
            if r.timed_out {
                return err("ilspycmd timed out after 120s; decompile a smaller type.");
            }
            return Ok(Out::new(
                "dotnet decompile",
                format!("{}{NOTE}", clip(r.out.trim_end(), ctx.output_limit)),
            ));
        }
        let mut req = json!({"file": file.display().to_string(), "mode": mode, "query": query});
        if let Some(l) = a.opt_u64("limit") {
            req["limit"] = l.into();
        }
        let v = py_helper(ctx, "dotnet.py", &req, "dnfile").await?;
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return err(e.to_string());
        }
        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
        Ok(Out::new(
            format!("dotnet {mode}"),
            format!("{}{NOTE}", clip(&text, ctx.output_limit)),
        ))
    }
}

pub struct UnityAssets;
#[async_trait]
impl Tool for UnityAssets {
    fn name(&self) -> String {
        "unity_assets".into()
    }
    fn description(&self) -> String {
        "Deep mode. Read a Unity asset file or bundle (globalgamemanagers, levelN, sharedassetsN.assets, .bundle, .unity3d): mode=info (version, object classes), list (objects; class and query filter), text (a TextAsset by path_id or exact name), tree (the serialized fields of one object, e.g. a MonoBehaviour's values).".into()
    }
    fn example(&self) -> String {
        r#"{"file": "original/Game_Data/level0", "mode": "list", "class": "MonoBehaviour"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"file":{"type":"string"},"mode":{"type":"string","description":"info | list | text | tree"},"class":{"type":"string","description":"object class for list, e.g. TextAsset, MonoBehaviour, Texture2D"},"query":{"type":"string","description":"name filter (list) or exact name (text/tree)"},"path_id":{"type":"integer"},"limit":{"type":"integer"}}),
            &["file", "mode"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        ctx.need_deep("unity_assets")?;
        let a = Args {
            v: &args,
            tool: "unity_assets",
            example: &self.example(),
        };
        let file = ctx.path(&a.str("file")?, false).await?;
        let mut req = json!({"file": file.display().to_string(), "mode": a.str("mode")?});
        for k in ["class", "query"] {
            if let Some(v) = a.opt_str(k) {
                req[k] = v.into();
            }
        }
        for k in ["path_id", "limit"] {
            if let Some(v) = a.v.get(k).and_then(|v| v.as_i64()) {
                req[k] = v.into();
            }
        }
        let v = py_helper(ctx, "unityassets.py", &req, "unitypy").await?;
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return err(e.to_string());
        }
        let text = serde_json::to_string_pretty(&v).unwrap_or_default();
        Ok(Out::new(
            "unity_assets",
            format!("{}{NOTE}", clip(&text, ctx.output_limit)),
        ))
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
            (Box::new(Functions), json!({"file":"x"})),
            (Box::new(Unity), json!({"path":"x"})),
            (Box::new(Dotnet), json!({"file":"x","mode":"info"})),
            (Box::new(UnityAssets), json!({"file":"x","mode":"info"})),
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

    #[test]
    fn finds_unity_versions_in_headers() {
        assert_eq!(
            unity_version(b"\0\0\0\x15\0\0 2021.3.5f1\0\x01\0").as_deref(),
            Some("2021.3.5f1")
        );
        assert_eq!(
            unity_version(b"UnityFS\x005.x.x\x002019.4.40f1\x00rev").as_deref(),
            Some("2019.4.40f1")
        );
        assert_eq!(unity_version(b"nothing here 1.2.3"), None);
    }
    #[tokio::test]
    async fn unity_survey_tells_mono_from_il2cpp() {
        let d = tempfile::tempdir().unwrap();
        let mono = d.path().join("Mono_Game/Game_Data/Managed");
        std::fs::create_dir_all(&mono).unwrap();
        std::fs::write(mono.join("Assembly-CSharp.dll"), b"MZ").unwrap();
        std::fs::write(mono.join("UnityEngine.dll"), b"MZ").unwrap();
        std::fs::write(
            d.path().join("Mono_Game/Game_Data/globalgamemanagers"),
            b"\0\0 2020.3.1f1\0",
        )
        .unwrap();
        let il = d.path().join("Il_Game/Game_Data/il2cpp_data/Metadata");
        std::fs::create_dir_all(&il).unwrap();
        std::fs::write(il.join("global-metadata.dat"), b"x").unwrap();
        std::fs::write(d.path().join("Il_Game/GameAssembly.dll"), b"MZ").unwrap();
        let c = Ctx::new(
            d.path().to_path_buf(),
            Arc::new(AutoGate(Reply::Once)),
            Rules::default(),
        );
        c.deep.store(true, std::sync::atomic::Ordering::Relaxed);
        let m = Unity
            .run(&c, json!({"path":"Mono_Game"}))
            .await
            .unwrap()
            .text;
        assert!(
            m.contains("Mono") && m.contains("2020.3.1f1") && m.contains("Assembly-CSharp.dll"),
            "{m}"
        );
        let i = Unity.run(&c, json!({"path":"Il_Game"})).await.unwrap().text;
        assert!(
            i.contains("IL2CPP") && i.contains("Il2CppDumper") && i.contains("GameAssembly.dll"),
            "{i}"
        );
        std::fs::create_dir_all(d.path().join("empty")).unwrap();
        assert!(
            Unity.run(&c, json!({"path":"empty"})).await.is_err(),
            "a folder without Unity data is an error that says what to do"
        );
    }
}
