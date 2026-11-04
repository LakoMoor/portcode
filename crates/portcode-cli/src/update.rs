//! Update check: compares this build with the latest GitHub release. Never installs anything by itself.
use std::time::Duration;

const API: &str = "https://api.github.com/repos/LakoMoor/portcode/releases/latest";
pub const INSTALL: &str =
    "curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh";

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
        Ok(l) if newer(&l, current()) => format!(
            "PortCode {l} is available (you have {}). Update: {INSTALL}",
            current()
        ),
        Ok(_) if manual => format!("PortCode {} is up to date.", current()),
        Ok(_) => String::new(),
        Err(e) if manual => format!("update check failed: {e}"),
        Err(_) => String::new(),
    }
}

/// At most once a day, silent unless a newer version exists. `PORTCODE_NO_UPDATE_CHECK=1` turns it off.
pub async fn passive() -> Option<String> {
    if std::env::var_os("PORTCODE_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    let file = portcode_agent::config::global_dir()?.join("update-check");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let last: u64 = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    if now.saturating_sub(last) < 86_400 {
        return None;
    }
    let _ = std::fs::create_dir_all(file.parent()?);
    let _ = std::fs::write(&file, now.to_string());
    let m = report(false).await;
    (!m.is_empty()).then_some(m)
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
}
