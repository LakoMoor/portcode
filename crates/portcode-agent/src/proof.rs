use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 50_000;
const MAX_BYTES: u64 = 1 << 30;

fn ignored(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    matches!(
        name.as_ref(),
        ".git"
            | ".portcode"
            | ".venv"
            | "target"
            | "dist"
            | "node_modules"
            | "engines"
            | "original"
    ) || name.starts_with(".env")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("id_rsa")
}

pub fn fingerprint_set(roots: &[PathBuf]) -> Result<String, String> {
    let mut hasher = Sha256::new();
    for root in roots {
        hasher.update(fingerprint(root)?.as_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Hash the files that can invalidate a build or parity verdict. Build products, sessions,
/// downloaded engines and the original game are deliberately outside this fingerprint.
pub fn fingerprint(root: &Path) -> Result<String, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("cannot fingerprint {}: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("project is not a folder: {}", root.display()));
    }
    let walker = ignore::WalkBuilder::new(&root)
        .hidden(false)
        .follow_links(false)
        .filter_entry(|e| !ignored(e.path()))
        .build();
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|e| format!("cannot scan project: {e}"))?;
        if entry.file_type().is_some_and(|t| t.is_file()) {
            files.push(entry.into_path());
            if files.len() > MAX_FILES {
                return Err(format!("project has more than {MAX_FILES} files; set the parity/project_check `project` argument to the port folder"));
            }
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_BYTES {
            return Err("project is larger than 1 GiB; set the parity/project_check `project` argument to the source folder (keep original assets outside it)".into());
        }
        hasher.update(rel.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_through_any_writer_invalidate_the_fingerprint() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("src.rs"), "fn main() {}").unwrap();
        let before = fingerprint(d.path()).unwrap();
        std::fs::write(
            d.path().join("src.rs"),
            "fn main() { println!(\"changed\"); }",
        )
        .unwrap();
        assert_ne!(before, fingerprint(d.path()).unwrap());
        std::fs::create_dir(d.path().join("target")).unwrap();
        std::fs::write(d.path().join("target/build.log"), "ignored").unwrap();
        assert_eq!(before, {
            std::fs::write(d.path().join("src.rs"), "fn main() {}").unwrap();
            fingerprint(d.path()).unwrap()
        });
    }
}
