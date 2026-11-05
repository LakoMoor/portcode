//! Update check: compares this build with the latest GitHub release. Never installs anything by itself.
use std::time::Duration;

const API: &str = "https://api.github.com/repos/LakoMoor/portcode/releases/latest";
#[cfg(not(windows))]
pub const INSTALL: &str =
    "curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh";
#[cfg(windows)]
pub const INSTALL: &str =
    "irm https://raw.githubusercontent.com/LakoMoor/portcode/main/install.ps1 | iex";

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['.', '-', '+'])
        .map_while(|p| p.parse().ok())
        .collect()
}

/// True when `latest` is a higher version than `now`.
pub fn newer(latest: &str, now: &str) -> bool {
    parts(latest) > parts(now)
}

pub async fn latest() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|e| format!("could not set up TLS ({e}). On a minimal Linux install the ca-certificates package is missing"))?;
    let body = client
        .get(API)
        .header("User-Agent", format!("portcode/{}", current()))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("could not reach GitHub: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|t| t.trim_start_matches('v').to_string())
        .ok_or_else(|| "no release published yet".to_string())
}

/// One human line. `manual` also reports "up to date" and errors.
pub async fn report(manual: bool) -> String {
    match latest().await {
        Ok(l) if newer(&l, current()) => notice(&l),
        Ok(_) if manual => format!("PortCode {} is up to date.", current()),
        Ok(_) => String::new(),
        Err(e) if manual => format!("update check failed: {e}"),
        Err(_) => String::new(),
    }
}

pub fn notice(latest: &str) -> String {
    format!(
        "PortCode {latest} is available (you have {}). Type /update to install it, or run `portcode update`.",
        current()
    )
}

fn cache_file() -> Option<std::path::PathBuf> {
    Some(portcode_agent::config::global_dir()?.join("update-check"))
}

/// `"<unix time> <latest version>"`
fn read_cache() -> (u64, String) {
    let t = cache_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_default();
    let mut w = t.split_whitespace();
    let ts = w.next().and_then(|x| x.parse().ok()).unwrap_or(0);
    (ts, w.next().unwrap_or("").to_string())
}

/// Shown at every start while a newer release exists (from the cache, no network), and the cache is refreshed
/// at most once a day in the background. `PORTCODE_NO_UPDATE_CHECK=1` turns it all off.
pub async fn passive() -> Option<String> {
    if std::env::var_os("PORTCODE_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    cleanup_old();
    let file = cache_file()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let (last, known) = read_cache();
    if now.saturating_sub(last) >= 86_400 {
        if let Ok(l) = latest().await {
            let _ = std::fs::create_dir_all(file.parent()?);
            let _ = std::fs::write(&file, format!("{now} {l}"));
            return newer(&l, current()).then(|| notice(&l));
        }
        return None;
    }
    (!known.is_empty() && newer(&known, current())).then(|| notice(&known))
}

fn target() -> Result<(&'static str, &'static str), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok(("aarch64-apple-darwin", "tar.gz")),
        ("macos", "x86_64") => Ok(("x86_64-apple-darwin", "tar.gz")),
        ("linux", "x86_64") => Ok(("x86_64-unknown-linux-gnu", "tar.gz")),
        ("linux", "aarch64") => Ok(("aarch64-unknown-linux-gnu", "tar.gz")),
        ("windows", "x86_64") => Ok(("x86_64-pc-windows-msvc", "zip")),
        ("windows", "aarch64") => Ok(("aarch64-pc-windows-msvc", "zip")),
        (o, a) => Err(format!("no prebuilt PortCode for {o}/{a}")),
    }
}

/// Where this binary came from decides who may replace it.
fn managed_hint(exe: &std::path::Path) -> Option<&'static str> {
    let p = exe.to_string_lossy().replace('\\', "/");
    if p.contains("/Cellar/") || p.contains("/homebrew/") {
        Some("PortCode was installed with Homebrew. Update it with: brew upgrade portcode")
    } else if p.contains("/.cargo/bin/") {
        Some("PortCode was installed with cargo. Update it with: cargo install --git https://github.com/LakoMoor/portcode portcode-cli --force")
    } else {
        None
    }
}

fn old_name(exe: &std::path::Path) -> std::path::PathBuf {
    let mut n = exe.as_os_str().to_owned();
    n.push(".old");
    n.into()
}

/// Windows cannot overwrite a running exe, so the previous one is renamed aside and removed on a later start.
fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_name(&exe));
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let r = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed ({url}): {e}"))?;
    r.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("download failed: {e}"))
}

/// Download the release for this system, verify its sha256, and replace the running binary.
pub async fn install(latest: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if let Some(h) = managed_hint(&exe) {
        return Err(h.into());
    }
    let (target, ext) = target()?;
    let tag = format!("v{}", latest.trim_start_matches('v'));
    let base = format!(
        "https://github.com/LakoMoor/portcode/releases/download/{tag}/portcode-{target}.{ext}"
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent(format!("portcode/{}", current()))
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = download(&client, &base).await?;
    let sum = String::from_utf8_lossy(&download(&client, &format!("{base}.sha256")).await?)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let got: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if sum.is_empty() || got != sum {
        return Err("sha256 mismatch: the download was discarded and nothing was changed".into());
    }
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let archive = tmp.path().join(format!("pc.{ext}"));
    std::fs::write(&archive, &bytes).map_err(|e| e.to_string())?;
    let ok = tokio::process::Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(tmp.path())
        .stdin(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    let bin = tmp.path().join(if cfg!(windows) {
        "portcode.exe"
    } else {
        "portcode"
    });
    if !ok || !bin.is_file() {
        return Err("could not unpack the download (is `tar` available?)".into());
    }
    // the new binary must start before it replaces the old one
    let started = tokio::process::Command::new(&bin)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !started {
        return Err("the downloaded binary did not start; nothing was changed".into());
    }
    let staged = {
        let mut n = exe.as_os_str().to_owned();
        n.push(".new");
        std::path::PathBuf::from(n)
    };
    std::fs::copy(&bin, &staged).map_err(|e| {
        format!(
            "cannot write next to {}: {e}. Re-run the installer instead: {INSTALL}",
            exe.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    let swap = || -> std::io::Result<()> {
        if cfg!(windows) {
            let old = old_name(&exe);
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&exe, &old)?;
            if let Err(e) = std::fs::rename(&staged, &exe) {
                let _ = std::fs::rename(&old, &exe);
                return Err(e);
            }
            Ok(())
        } else {
            std::fs::rename(&staged, &exe)
        }
    };
    swap().map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        format!("could not replace {}: {e}", exe.display())
    })?;
    if let Some(f) = cache_file() {
        let _ = std::fs::write(f, format!("0 {}", latest.trim_start_matches('v')));
    }
    Ok(format!(
        "updated PortCode {} -> {}. Restart PortCode to use the new version.",
        current(),
        latest.trim_start_matches('v')
    ))
}

/// `/update` and `portcode update`: check, and install when a newer release exists.
pub async fn apply() -> String {
    match latest().await {
        Ok(l) if newer(&l, current()) => match install(&l).await {
            Ok(m) => m,
            Err(e) => format!("could not update to {l}: {e}"),
        },
        Ok(_) => format!("PortCode {} is up to date.", current()),
        Err(e) => format!("update check failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_versions() {
        assert!(newer("0.1.1", "0.1.0"));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.2.0"));
    }
    #[test]
    fn package_managers_own_their_binaries() {
        use std::path::Path;
        assert!(managed_hint(Path::new(
            "/opt/homebrew/Cellar/portcode/0.1.7/bin/portcode"
        ))
        .is_some());
        assert!(managed_hint(Path::new("/home/u/.cargo/bin/portcode")).is_some());
        assert!(managed_hint(Path::new("/home/u/.local/bin/portcode")).is_none());
        assert_eq!(
            old_name(Path::new("C:/x/portcode.exe")).to_string_lossy(),
            "C:/x/portcode.exe.old"
        );
    }
    #[test]
    fn notice_tells_how_to_update() {
        assert!(notice("9.9.9").contains("/update"));
    }
}
