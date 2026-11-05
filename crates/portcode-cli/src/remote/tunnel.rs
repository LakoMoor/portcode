//! Cloudflare quick tunnel for remote control: no account, no config. `cloudflared` is found on PATH or in the
//! engines folder; `install()` downloads it (only when the user agreed) and verifies the sha256 GitHub publishes.
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const API: &str = "https://api.github.com/repos/cloudflare/cloudflared/releases/latest";

pub struct Tunnel {
    pub url: String,
    _child: Child,
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "cloudflared.exe"
    } else {
        "cloudflared"
    }
}

pub fn find() -> Option<PathBuf> {
    let own = portcode_agent::engines::dir().join(exe_name());
    if own.is_file() {
        return Some(own);
    }
    portcode_agent::engines::which("cloudflared")
}

/// First `https://….trycloudflare.com` in a line of cloudflared output.
pub fn parse_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = line.get(start..)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || "-.:/".contains(c)))
        .unwrap_or(rest.len());
    let url = rest.get(..end)?;
    url.ends_with(".trycloudflare.com").then(|| url.to_string())
}

/// `wss://…` for a tunnel URL.
pub fn ws_url(https: &str) -> String {
    format!("wss://{}", https.trim_start_matches("https://"))
}

/// The link that opens the remote page already connected: the fragment never leaves the browser.
pub fn pairing_link(https: &str, key: &str) -> String {
    format!(
        "{}/portcode/remote/#url={}&key={}",
        super::PAGES_ORIGIN,
        ws_url(https),
        key
    )
}

/// A full-scope key for the pairing link: the first one that exists, else a new one.
pub fn link_key() -> Result<String, String> {
    let all = super::keys::load();
    if let Some(k) = all.iter().find(|k| k.scope == "full") {
        return Ok(super::keys::pairing(k));
    }
    Ok(super::keys::pairing(&super::keys::create("full")?))
}

/// The link as a QR code made of terminal block characters, for scanning with a phone.
pub fn qr(link: &str) -> String {
    use qrcode::render::unicode::Dense1x2;
    qrcode::QrCode::new(link.as_bytes())
        .map(|c| c.render::<Dense1x2>().quiet_zone(true).build())
        .unwrap_or_default()
}

pub async fn start(port: u16) -> Result<Tunnel, String> {
    let bin = find().ok_or("cloudflared is not installed. Run: portcode remote tunnel-install")?;
    let mut child = Command::new(bin)
        .args(["tunnel", "--no-autoupdate", "--url"])
        .arg(format!("http://127.0.0.1:{port}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot start cloudflared: {e}"))?;
    let err = child.stderr.take().ok_or("no output from cloudflared")?;
    let mut lines = BufReader::new(err).lines();
    let found = tokio::time::timeout(Duration::from_secs(40), async {
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(u) = parse_url(&l) {
                return Some(u);
            }
        }
        None
    })
    .await;
    match found {
        Ok(Some(url)) => {
            // keep draining so cloudflared never blocks on a full pipe
            tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
            Ok(Tunnel { url, _child: child })
        }
        _ => Err("cloudflared did not give a tunnel address in 40 s (no internet?)".into()),
    }
}

fn asset_name() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("cloudflared-darwin-arm64.tgz"),
        ("macos", "x86_64") => Ok("cloudflared-darwin-amd64.tgz"),
        ("linux", "x86_64") => Ok("cloudflared-linux-amd64"),
        ("linux", "aarch64") => Ok("cloudflared-linux-arm64"),
        ("windows", "x86_64") => Ok("cloudflared-windows-amd64.exe"),
        (o, a) => Err(format!(
            "no cloudflared build for {o}/{a}; install it yourself and run again"
        )),
    }
}

/// Download cloudflared into the engines folder. Refuses a release that publishes no sha256.
pub async fn install() -> Result<PathBuf, String> {
    let want = asset_name()?;
    let client = reqwest::Client::builder()
        .user_agent("portcode")
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let rel: serde_json::Value = client
        .get(API)
        .send()
        .await
        .map_err(|e| format!("cannot reach GitHub: {e}"))?
        .json()
        .await
        .map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    let asset = rel["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == want))
        .ok_or_else(|| format!("latest cloudflared release has no {want}"))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or("asset without a download url")?;
    let sha = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or("the release publishes no sha256; refusing to install an unverified download")?
        .to_lowercase();
    let bytes = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    let got: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got != sha {
        return Err("sha256 mismatch; download discarded".into());
    }
    let dir = portcode_agent::engines::dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(exe_name());
    if want.ends_with(".tgz") {
        let tmp = dir.join("cloudflared.tgz");
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        let ok = Command::new("tar")
            .arg("-xzf")
            .arg(&tmp)
            .arg("-C")
            .arg(&dir)
            .stdin(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&tmp);
        if !ok {
            return Err("could not unpack cloudflared (is `tar` available?)".into());
        }
    } else {
        std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    if dest.is_file() {
        Ok(dest)
    } else {
        Err("cloudflared was not found after unpacking".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_the_url_in_cloudflared_output() {
        let l = "2026-01-01T00:00:00Z INF |  https://quiet-lake-1234.trycloudflare.com                |";
        assert_eq!(
            parse_url(l).as_deref(),
            Some("https://quiet-lake-1234.trycloudflare.com")
        );
        assert_eq!(parse_url("INF see https://www.cloudflare.com/docs"), None);
        assert_eq!(parse_url("no url here"), None);
    }
    #[test]
    fn link_keeps_secret_in_the_fragment() {
        let l = pairing_link("https://a-b.trycloudflare.com", "pc1.x.y");
        assert!(l.contains("/portcode/remote/#url=wss://a-b.trycloudflare.com&key=pc1.x.y"));
    }
}
