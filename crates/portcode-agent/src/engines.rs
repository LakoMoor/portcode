//! Reverse-engineering engines PortCode can drive (deep mode) and where they live.
//!
//! Everything PortCode installs goes under one folder next to the `portcode` binary (`<exe dir>/engines`), or
//! `PORTCODE_ENGINES`, or `~/.portcode/engines` when the binary's folder is read-only. Nothing is installed without
//! the user's consent (`re_install` asks), and downloads are verified against the publisher's sha256.
use std::path::{Path, PathBuf};

pub const DISASM_PY: &str = include_str!("../re/disasm.py");
pub const FUNCTIONS_PY: &str = include_str!("../re/functions.py");
pub const DOTNET_PY: &str = include_str!("../re/dotnet.py");
pub const UNITY_PY: &str = include_str!("../re/unityassets.py");
pub const EMULATE_PY: &str = include_str!("../re/emulate.py");

#[derive(Debug, Clone, Copy)]
pub enum How {
    /// pip package into PortCode's private venv; `import` name used to detect it.
    Pip(&'static str, &'static str),
    /// Kuna decompiler: one self-contained binary from the project's GitHub releases.
    Kuna,
    ImHex,
    /// Found on PATH; installed by the user's package manager (`hint`).
    System(&'static str, &'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Engine {
    pub id: &'static str,
    pub what: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub how: How,
    /// Installed by default with `engines install --core`.
    pub core: bool,
}

pub const ENGINES: &[Engine] = &[
    Engine { id: "capstone", what: "disassembler library (x86, ARM, ARM64, MIPS, RISC-V, PPC…) — tool `disasm`", license: "BSD-3", url: "https://www.capstone-engine.org", how: How::Pip("capstone", "capstone"), core: true },
    Engine { id: "unicorn", what: "CPU emulator — run a function or decoder in isolation — tool `emulate`", license: "GPL-2.0", url: "https://www.unicorn-engine.org", how: How::Pip("unicorn", "unicorn"), core: true },
    Engine { id: "keystone", what: "assembler library (write a patch or test stub)", license: "GPL-2.0", url: "https://www.keystone-engine.org", how: How::Pip("keystone-engine", "keystone"), core: false },
    Engine { id: "lief", what: "parse/modify ELF, PE, Mach-O, DEX (sections, imports, exports)", license: "Apache-2.0", url: "https://lief.re", how: How::Pip("lief", "lief"), core: true },
    Engine { id: "yara", what: "pattern rules for files (crypto constants, packers, formats)", license: "BSD-3", url: "https://virustotal.github.io/yara", how: How::Pip("yara-python", "yara"), core: false },
    Engine { id: "angr", what: "symbolic execution and CFG recovery (large install)", license: "BSD-2", url: "https://angr.io", how: How::Pip("angr", "angr"), core: false },
    Engine { id: "frida", what: "dynamic instrumentation of a running process (frida-tools)", license: "wxWindows", url: "https://frida.re", how: How::Pip("frida-tools", "frida"), core: false },
    Engine { id: "kuna", what: "fast Rust decompiler (ported from Ghidra), one binary, no JDK — tool `decompile`", license: "Apache-2.0", url: "https://github.com/Noelo-Lab/kuna", how: How::Kuna, core: true },
    Engine { id: "imhex", what: "hex editor with a pattern language for binary formats (for you; the agent writes .hexpat files)", license: "GPL-2.0", url: "https://imhex.werwolv.net", how: How::ImHex, core: false },
    Engine { id: "rizin", what: "reverse-engineering framework (radare2 fork), scriptable from `bash`", license: "LGPL-3.0", url: "https://rizin.re", how: How::System("rizin", "brew install rizin  |  apt install rizin"), core: false },
    Engine { id: "radare2", what: "reverse-engineering framework, 60+ architectures", license: "LGPL-3.0", url: "https://rada.re", how: How::System("r2", "brew install radare2  |  apt install radare2"), core: false },
    Engine { id: "binwalk", what: "find and extract embedded files and firmware images", license: "MIT", url: "https://github.com/ReFirmLabs/binwalk", how: How::System("binwalk", "brew install binwalk  |  cargo install binwalk"), core: false },
    Engine { id: "die", what: "Detect It Easy: packers, compilers, protectors", license: "MIT", url: "https://github.com/horsicq/Detect-It-Easy", how: How::System("diec", "brew install detect-it-easy"), core: false },
    Engine { id: "capa", what: "identify capabilities in executables (Mandiant)", license: "Apache-2.0", url: "https://github.com/mandiant/capa", how: How::System("capa", "pip install flare-capa  |  brew install capa"), core: false },
    Engine { id: "dnfile", what: ".NET / C# / Unity Mono assembly metadata: types, methods, strings, P/Invoke", license: "MIT", url: "https://github.com/malwarefrank/dnfile", how: How::Pip("dnfile", "dnfile"), core: false },
    Engine { id: "unitypy", what: "Unity asset files and bundles: objects, TextAssets, MonoBehaviour data", license: "MIT", url: "https://github.com/K0lb3/UnityPy", how: How::Pip("UnityPy", "UnityPy"), core: false },
    Engine { id: "ilspy", what: "ILSpy command line: decompile .NET / Unity Mono assemblies back to C#", license: "MIT", url: "https://github.com/icsharpcode/ILSpy", how: How::System("ilspycmd", "dotnet tool install -g ilspycmd   (needs the .NET SDK)"), core: false },
    Engine { id: "il2cppdumper", what: "Unity IL2CPP: rebuild type and method names from global-metadata.dat", license: "MIT", url: "https://github.com/Perfare/Il2CppDumper", how: How::System("Il2CppDumper", "download a release from the project page and put it on PATH (needs .NET)"), core: false },
    Engine { id: "jadx", what: "Android DEX/APK → Java decompiler", license: "Apache-2.0", url: "https://github.com/skylot/jadx", how: How::System("jadx", "brew install jadx"), core: false },
    Engine { id: "wabt", what: "WebAssembly binary toolkit (wasm2wat, wasm-decompile)", license: "Apache-2.0", url: "https://github.com/WebAssembly/wabt", how: How::System("wasm2wat", "brew install wabt"), core: false },
    Engine { id: "upx", what: "detect/unpack UPX-packed executables", license: "GPL-2.0", url: "https://upx.github.io", how: How::System("upx", "brew install upx"), core: false },
];

pub fn find(id: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|e| e.id == id)
}

/// Where engines are installed.
pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PORTCODE_ENGINES") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("engines")))
    {
        if d.is_dir() || std::fs::create_dir_all(&d).is_ok() {
            return d;
        }
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".portcode/engines")
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path)
        .flat_map(|d| exts.iter().map(move |x| d.join(format!("{bin}{x}"))))
        .find(|p| p.is_file())
}

pub fn venv_python() -> PathBuf {
    let unix = dir().join("py/bin/python");
    if unix.is_file() {
        return unix;
    }
    let win = dir().join("py").join("Scripts").join("python.exe");
    if win.is_file() {
        return win;
    }
    unix
}

fn first_dir_with(base: &Path, rel: &str) -> Option<PathBuf> {
    for e in std::fs::read_dir(base).ok()?.flatten() {
        let p = e.path().join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// The `kuna` binary: PortCode's own install under the engines folder, else one on PATH.
pub fn kuna_bin() -> Option<PathBuf> {
    let name = if cfg!(windows) { "kuna.exe" } else { "kuna" };
    first_dir_with(&dir().join("kuna"), name)
        .or_else(|| {
            let p = dir().join("kuna").join(name);
            p.is_file().then_some(p)
        })
        .or_else(|| which("kuna"))
}

pub fn imhex_path() -> Option<PathBuf> {
    let d = dir().join("imhex");
    let mut cands = vec![
        d.join("ImHex.app"),
        PathBuf::from("/Applications/ImHex.app"),
    ];
    if let Ok(rd) = std::fs::read_dir(&d) {
        cands.extend(
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "AppImage")),
        );
    }
    cands
        .into_iter()
        .find(|p| p.exists())
        .or_else(|| which("imhex"))
}

fn py_has(import: &str) -> bool {
    let py = venv_python();
    py.is_file()
        && std::process::Command::new(py)
            .args(["-I", "-c", &format!("import {import}")])
            .stdin(std::process::Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// `Some(where)` when usable.
pub fn detect(e: &Engine) -> Option<String> {
    match e.how {
        How::Pip(_, imp) => py_has(imp).then(|| "portcode venv".into()),
        How::Kuna => kuna_bin().map(|p| p.display().to_string()),
        How::ImHex => imhex_path().map(|p| p.display().to_string()),
        How::System(bin, _) => which(bin).map(|p| p.display().to_string()),
    }
}

pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Shell script that installs `e` under [`dir`]. `Err` explains why it cannot (and what to do instead).
pub fn install_script(e: &Engine) -> Result<String, String> {
    let d = dir();
    let q = sh_quote(&d.display().to_string());
    match e.how {
        How::Pip(pkg, _) => Ok(format!(
            "set -e\nmkdir -p {q}\nif [ ! -x {q}/py/bin/python ] && [ ! -f {q}/py/Scripts/python.exe ]; then python3 -m venv {q}/py || python -m venv {q}/py; fi\nPY={q}/py/bin/python; [ -x \"$PY\" ] || PY={q}/py/Scripts/python.exe\n\"$PY\" -m pip install --disable-pip-version-check -q {}\necho installed {pkg}",
            sh_quote(pkg)
        )),
        How::Kuna => Ok(format!(
            r#"set -e
mkdir -p {q}/kuna && cd {q}/kuna
python3 - <<'PY'
import hashlib, io, json, os, platform, sys, tarfile, urllib.request, zipfile
sysname, mach = platform.system(), platform.machine().lower()
arm = mach in ("arm64", "aarch64")
key = {{"Darwin": "macos-arm64" if arm else "macos-x86_64",
        "Linux": "linux-arm64" if arm else "linux-x86_64-musl",
        "Windows": "windows-x86_64"}}.get(sysname)
if not key:
    sys.exit("no Kuna build for " + sysname + " " + mach + ": see https://github.com/Noelo-Lab/kuna/releases")
r = json.load(urllib.request.urlopen("https://api.github.com/repos/Noelo-Lab/kuna/releases/latest", timeout=30))
asset = next((a for a in r["assets"] if key in a["name"] and a["name"].endswith((".tar.gz", ".zip")) and "ghidra" not in a["name"] and "specs" not in a["name"]), None)
if not asset:
    sys.exit("the latest Kuna release has no build for " + key)
want = (asset.get("digest") or "").replace("sha256:", "")
if not want:
    sys.exit("release has no published sha256; refusing to install an unverified download")
data = urllib.request.urlopen(asset["browser_download_url"], timeout=300).read()
if hashlib.sha256(data).hexdigest() != want:
    sys.exit("sha256 mismatch - download discarded")
exe = "kuna.exe" if sysname == "Windows" else "kuna"
if asset["name"].endswith(".zip"):
    z = zipfile.ZipFile(io.BytesIO(data)); member = next(n for n in z.namelist() if n.endswith(exe)); body = z.read(member)
else:
    t = tarfile.open(fileobj=io.BytesIO(data), mode="r:gz"); member = next(m for m in t.getmembers() if m.isfile() and m.name.endswith("/" + exe)); body = t.extractfile(member).read()
open(exe, "wb").write(body); os.chmod(exe, 0o755)
print("sha256 verified; installed Kuna " + r.get("tag_name", "") + " under " + os.getcwd())
PY"#
        )),
        How::ImHex => {
            if cfg!(target_os = "macos") {
                Ok(format!(
                    "set -e\ncommand -v brew >/dev/null || {{ echo 'needs Homebrew, or download ImHex from https://github.com/WerWolv/ImHex/releases'; exit 1; }}\nmkdir -p {q}/imhex\nbrew install --cask --appdir={q}/imhex imhex"
                ))
            } else {
                Ok(format!(
                    r#"set -e
mkdir -p {q}/imhex && cd {q}/imhex
python3 - <<'PY' > .asset
import json, platform, urllib.request
r = json.load(urllib.request.urlopen("https://api.github.com/repos/WerWolv/ImHex/releases/latest", timeout=30))
arch = "aarch64" if platform.machine() in ("aarch64", "arm64") else "x86_64"
for a in r["assets"]:
    if a["name"].endswith(".AppImage") and arch in a["name"]:
        print(a["name"], a["browser_download_url"], (a.get("digest") or "").replace("sha256:", "")); break
PY
read NAME URL SHA < .asset
[ -n "$SHA" ] || {{ echo "release has no published sha256; refusing to install an unverified download"; exit 1; }}
curl -fL --retry 3 -o "$NAME" "$URL"
python3 -c "import hashlib,sys; h=hashlib.sha256(open(sys.argv[2],'rb').read()).hexdigest(); sys.exit(0 if h==sys.argv[1] else 'sha256 mismatch')" "$SHA" "$NAME" || {{ rm -f "$NAME"; exit 1; }}
chmod +x "$NAME" && rm -f .asset"#
                ))
            }
        }
        How::System(_, hint) => Err(format!(
            "{} is a system tool; PortCode does not install it. Install it yourself: {hint}",
            e.id
        )),
    }
}

/// Write the helper scripts next to the engines and return their directory.
pub fn write_helpers() -> Result<PathBuf, String> {
    let d = dir().join("scripts");
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    for (n, body) in [
        ("disasm.py", DISASM_PY),
        ("functions.py", FUNCTIONS_PY),
        ("dotnet.py", DOTNET_PY),
        ("unityassets.py", UNITY_PY),
        ("emulate.py", EMULATE_PY),
    ] {
        let p = d.join(n);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(body) {
            std::fs::write(&p, body).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(d)
}

pub fn table() -> String {
    let mut s = String::new();
    for e in ENGINES {
        let st = detect(e)
            .map(|w| format!("installed ({w})"))
            .unwrap_or_else(|| match e.how {
                How::System(_, h) => format!("missing — {h}"),
                _ => "missing — re_install".into(),
            });
        s.push_str(&format!("{:<9} {} [{}]\n", e.id, st, e.license));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_unique_and_scripts_nonempty() {
        let mut ids: Vec<_> = ENGINES.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ENGINES.len());
        assert!(DISASM_PY.contains("capstone") && EMULATE_PY.contains("unicorn"));
    }
    #[test]
    fn quoting() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
    }
    #[test]
    fn system_engines_are_not_installed_by_us() {
        let e = find("rizin").unwrap();
        assert!(install_script(e).is_err());
        assert!(install_script(find("unicorn").unwrap())
            .unwrap()
            .contains("pip install"));
    }
}
