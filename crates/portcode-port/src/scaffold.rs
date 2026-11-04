use std::path::Path;

const PORT_TEMPLATE: &str = include_str!("../templates/PORT.md");

fn write_new(path: &Path, body: &str, created: &mut Vec<String>) -> Result<(), String> {
    if path.exists() {
        return Err(format!("refusing to overwrite {}", path.display()));
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    std::fs::write(path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    created.push(path.display().to_string());
    Ok(())
}

/// Create a workspace `<name>-core` (headless logic) + `<name>` (binary) with PORT.md, golden and screenshot dirs.
pub fn create(name: &str, dest: &Path) -> Result<Vec<String>, String> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-');
    if !ok {
        return Err(format!(
            "invalid crate name {name:?}: use letters, digits, '-' or '_'"
        ));
    }
    let mut c = Vec::new();
    write_new(&dest.join("Cargo.toml"), "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\"]\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace.lints.rust]\nunsafe_code = \"forbid\"\n", &mut c)?;
    write_new(&dest.join(format!("crates/{name}-core/Cargo.toml")), &format!("[package]\nname = \"{name}-core\"\nversion.workspace = true\nedition.workspace = true\n\n[lints]\nworkspace = true\n"), &mut c)?;
    write_new(&dest.join(format!("crates/{name}-core/src/lib.rs")), "//! Headless, deterministic core. No window, GPU, clock or global state in here.\n#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]\n", &mut c)?;
    let id = name.replace('-', "_");
    write_new(&dest.join(format!("crates/{name}/Cargo.toml")), &format!("[package]\nname = \"{name}\"\nversion.workspace = true\nedition.workspace = true\n\n[dependencies]\n{name}-core = {{ path = \"../{name}-core\" }}\n\n[lints]\nworkspace = true\n"), &mut c)?;
    write_new(
        &dest.join(format!("crates/{name}/src/main.rs")),
        &format!("fn main() {{\n    // thin shell around {id}_core\n}}\n"),
        &mut c,
    )?;
    write_new(
        &dest.join("docs/PORT.md"),
        &PORT_TEMPLATE.replace("{{name}}", name),
        &mut c,
    )?;
    write_new(&dest.join("docs/screenshots/.gitkeep"), "", &mut c)?;
    write_new(&dest.join("tests/golden/.gitkeep"), "", &mut c)?;
    write_new(
        &dest.join(".gitignore"),
        "/target\n.portcode/\nreference.lock.json\n*.sqlite*\n",
        &mut c,
    )?;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaffolds_and_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let made = create("demo", d.path()).unwrap();
        assert!(made.len() >= 8 && d.path().join("docs/PORT.md").is_file());
        assert!(create("demo", d.path()).is_err());
        assert!(create("../evil", d.path()).is_err());
    }
}
