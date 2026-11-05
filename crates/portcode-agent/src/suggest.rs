//! Start-up suggestion: look at the folder PortCode was opened in and guess what the user wants to do.
//!
//! Only names and the first lines of a fixed list of project files are read (never `.env`, keys or other files),
//! and they go to the model the user already chose. Subscription backends and failures fall back to a local guess.
use crate::config::Config;
use portcode_llm::{Client, Message};
use std::path::Path;
use std::time::Duration;

/// Files whose first lines may be shown to the model.
const PEEK: &[&str] = &[
    "README.md",
    "README",
    "README.rst",
    "README.txt",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "Makefile",
    "CMakeLists.txt",
    "docs/SPEC.md",
];
const SKIP: &[&str] = &[
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
];

/// What the folder looks like, as plain text for the model (small and bounded).
pub fn scan(root: &Path) -> String {
    let mut out = String::new();
    let mut names: Vec<(String, bool)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            let dir = e.file_type().ok()?.is_dir();
            (!n.starts_with('.') && !SKIP.contains(&n.as_str())).then_some((n, dir))
        })
        .collect();
    names.sort();
    if names.is_empty() {
        out += "The folder is empty.\n";
    } else {
        out += "Top level:\n";
    }
    for (n, dir) in names.iter().take(40) {
        if *dir {
            let inner: Vec<String> = std::fs::read_dir(root.join(n))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|x| !x.starts_with('.'))
                .collect();
            let mut inner = inner;
            inner.sort();
            let shown: Vec<&str> = inner.iter().take(8).map(String::as_str).collect();
            out += &format!(
                "  {n}/ ({} entries: {}{})\n",
                inner.len(),
                shown.join(", "),
                if inner.len() > 8 { ", …" } else { "" }
            );
        } else {
            out += &format!("  {n}\n");
        }
    }
    if names.len() > 40 {
        out += &format!("  … and {} more\n", names.len() - 40);
    }
    for f in PEEK {
        if let Ok(t) = std::fs::read_to_string(root.join(f)) {
            let head: String = t.chars().take(600).collect();
            out += &format!("\n--- {f} (start) ---\n{}\n", head.trim());
        }
    }
    if root.join(".portcode/evidence").is_dir() {
        out += "\nPortCode has already worked here (an evidence ledger exists).\n";
    }
    out
}

/// A sensible template without asking a model.
pub fn heuristic(root: &Path) -> String {
    let has = |p: &str| root.join(p).exists();
    let empty = std::fs::read_dir(root)
        .map(|d| {
            d.flatten()
                .all(|e| e.file_name().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false);
    if empty {
        return "I want to recreate something in Rust. Ask me what it is (a website, a program, a game or a file format), where the original is, and how closely I may look inside it.".into();
    }
    if has("original") {
        return "Study the program in original/ from the outside (pin its build, run it, record what you see as evidence), then plan the Rust port module by module with parity checks.".into();
    }
    if has("Cargo.toml") {
        return "Read this Rust project, run its tests, and tell me what is unfinished or broken. Then fix the first failing thing.".into();
    }
    if has("package.json") {
        return "Read this JavaScript project, run its tests or build, and tell me what is broken. Then fix the first problem.".into();
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        return "Read this Python project in a virtual environment, run its tests, and tell me what is broken. Then fix the first problem.".into();
    }
    if has("go.mod") {
        return "Read this Go project, run its tests, and tell me what is broken. Then fix the first problem.".into();
    }
    "Survey this folder: tell me what is in it, what it is for, and what you would do with it next."
        .into()
}

const SYSTEM: &str = "You see the contents of a project folder. Guess what the user most likely wants to do next in it with PortCode, a coding agent that also reverse-engineers programs and ports them to Rust. Reply with ONE message the user could send, 1 to 3 sentences, first person, imperative, naming real files or folders you saw. Use the language of the README or docs; English if there is none. No quotes, no preamble, no questions to the user.";

/// Ask the model for a suggestion; falls back to [`heuristic`]. Never fails and never takes longer than 15 s.
pub async fn suggest(cfg: &Config, root: &Path) -> String {
    let fallback = heuristic(root);
    let Ok(r) = cfg.resolve(None) else {
        return fallback;
    };
    // Subscription backends spawn a whole CLI per call; do not do that just to guess.
    if portcode_llm::is_cli(&r.provider) {
        return fallback;
    }
    // A provider that needs a key and has none cannot answer: do not wait for the network to say so.
    let p = &r.provider;
    let keyless = p.api_key.is_none()
        && p.api_key_env
            .as_deref()
            .is_some_and(|e| std::env::var(e).map_or(true, |v| v.is_empty()));
    if keyless {
        return fallback;
    }
    let Ok(client) = Client::new(r.provider.clone()) else {
        return fallback;
    };
    let mut prof = r.profile.clone();
    prof.non_thinking();
    prof.max_tokens = 220;
    prof.temperature = Some(0.4);
    let msgs = vec![Message::system(SYSTEM), Message::user(scan(root))];
    let mut sink = |_| {};
    let call = client.chat(&r.model, &prof, &msgs, &[], &mut sink);
    match tokio::time::timeout(Duration::from_secs(15), call).await {
        Ok(Ok(a)) => clean(&a.text).unwrap_or(fallback),
        _ => fallback,
    }
}

/// Make a model reply safe to put in the prompt box: no think blocks, quotes or markdown fences; bounded length.
pub fn clean(raw: &str) -> Option<String> {
    let t = portcode_llm::strip_think(raw);
    let t = t
        .trim()
        .trim_matches(|c| c == '"' || c == '`' || c == '“' || c == '”');
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(400).collect();
    (t.chars().count() >= 12).then_some(t)
}

/// Wait this long between turning the feature off in config and it being gone: it is on unless `"suggest": false`.
pub fn enabled(cfg: &Config) -> bool {
    cfg.suggest.unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pc-suggest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn scan_lists_names_peeks_only_allowed_files_and_skips_secrets() {
        let d = tmp("scan");
        std::fs::write(d.join("README.md"), "# Tiny tool\nCounts words.").unwrap();
        std::fs::write(d.join(".env"), "API_KEY=sk-secret").unwrap();
        std::fs::write(d.join("notes.txt"), "private notes").unwrap();
        std::fs::create_dir_all(d.join("original")).unwrap();
        std::fs::write(d.join("original/app.bin"), [0u8; 4]).unwrap();
        std::fs::create_dir_all(d.join("node_modules/x")).unwrap();
        let s = scan(&d);
        assert!(s.contains("README.md") && s.contains("Counts words"));
        assert!(s.contains("original/ (1 entries: app.bin)"));
        assert!(!s.contains("sk-secret") && !s.contains(".env"));
        assert!(!s.contains("private notes"), "only the allowlist is read");
        assert!(!s.contains("node_modules"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn heuristic_follows_what_is_there() {
        let d = tmp("h");
        assert!(heuristic(&d).contains("recreate"), "empty folder");
        std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        assert!(heuristic(&d).contains("Rust project"));
        std::fs::create_dir_all(d.join("original")).unwrap();
        assert!(heuristic(&d).contains("original/"), "an original wins");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn model_replies_are_cleaned_and_junk_is_dropped() {
        assert_eq!(
            clean("<think>hm</think>\n\"Port the parser in src/.\"\n").as_deref(),
            Some("Port the parser in src/.")
        );
        assert!(clean("ok").is_none(), "too short to be useful");
        assert!(clean(&"x ".repeat(500)).is_some_and(|t| t.chars().count() <= 400));
    }
}
