use ignore::WalkBuilder;
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct Survey {
    pub root: String,
    pub languages: BTreeMap<String, (u64, u64)>,
    pub manifests: Vec<String>,
    pub hints: BTreeMap<String, u64>,
    pub assets: BTreeMap<String, u64>,
    pub test_dirs: Vec<String>,
    pub ci: Vec<String>,
    pub hotspots: BTreeMap<String, u64>,
    pub files_scanned: u64,
    pub truncated: bool,
}

const LANGS: &[&str] = &[
    "c", "h", "cpp", "cc", "cxx", "hpp", "cs", "java", "kt", "swift", "m", "mm", "js", "ts", "tsx",
    "py", "lua", "gd", "go", "rs", "glsl", "hlsl", "wgsl", "vert", "frag", "shader",
];
const ASSETS: &[&str] = &[
    "png", "jpg", "jpeg", "tga", "bmp", "dds", "ktx", "wav", "ogg", "mp3", "flac", "ttf", "otf",
    "obj", "fbx", "gltf", "glb", "json", "xml", "yaml", "toml", "ini", "dat", "bin", "sav",
];
const MANIFESTS: &[&str] = &[
    "CMakeLists.txt",
    "Makefile",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "build.gradle",
    "Package.swift",
    "Cargo.toml",
    "meson.build",
    "project.godot",
    "go.mod",
];
const HINTS: &[(&str, &str)] = &[
    ("Unity", r"UnityEngine"),
    ("SDL", r"SDL_|SDL2"),
    ("GLFW", r"GLFW|glfw"),
    ("OpenGL", r"glBind|OpenGL|glad"),
    ("Vulkan", r"vulkan|\bVk[A-Z]"),
    ("Direct3D", r"d3d11|d3d12|DirectX|ID3D"),
    ("Metal", r"\bMTL[A-Z]|import Metal"),
    ("SFML", r"SFML"),
    ("raylib", r"raylib"),
    ("Qt", r"QApplication|Qt[A-Z]\w+"),
    ("ImGui", r"imgui|ImGui"),
    ("Box2D", r"b2World|box2d"),
    ("Bullet", r"btRigid|bullet"),
    ("Audio", r"OpenAL|FMOD|miniaudio"),
    ("pygame", r"pygame"),
    ("three.js", r"THREE\.|three\.js"),
    ("WebGL", r"WebGL"),
    ("Electron", r"electron"),
];
const HOTSPOTS: &[(&str, &str)] = &[
    ("global/static mutable", r"^\s*(static|extern)\s+[^(]*="),
    (
        "raw pointers/malloc",
        r"\bmalloc\(|reinterpret_cast|\(void\s*\*\)",
    ),
    ("threads", r"std::thread|pthread_|threading\.|Task\.Run"),
    ("RNG", r"\brand\(|srand|Random\(|Math\.random|random\."),
    ("floating-point heavy", r"\b(sinf?|cosf?|sqrtf?|powf?)\("),
];

pub fn run(root: &Path, max_files: u64) -> Survey {
    let hint_re: Vec<(&str, Regex)> = HINTS
        .iter()
        .filter_map(|(n, p)| Regex::new(p).ok().map(|r| (*n, r)))
        .collect();
    let hot_re: Vec<(&str, Regex)> = HOTSPOTS
        .iter()
        .filter_map(|(n, p)| Regex::new(p).ok().map(|r| (*n, r)))
        .collect();
    let mut s = Survey {
        root: root.display().to_string(),
        ..Default::default()
    };
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !matches!(
                n.as_ref(),
                ".git" | "node_modules" | "target" | "build" | "Pods" | "Library" | ".portcode"
            )
        })
        .build();
    for ent in walker.flatten() {
        let p = ent.path();
        let rel = p.strip_prefix(root).unwrap_or(p).display().to_string();
        if p.is_dir() {
            let n = ent.file_name().to_string_lossy();
            if matches!(n.as_ref(), "test" | "tests" | "__tests__" | "spec")
                && s.test_dirs.len() < 8
            {
                s.test_dirs.push(rel.clone());
            }
            if rel == ".github/workflows" {
                s.ci.push(rel);
            }
            continue;
        }
        if s.files_scanned >= max_files {
            s.truncated = true;
            break;
        }
        s.files_scanned += 1;
        let name = ent.file_name().to_string_lossy().to_string();
        if (MANIFESTS.contains(&name.as_str())
            || name.ends_with(".csproj")
            || name.ends_with(".sln"))
            && s.manifests.len() < 30
        {
            s.manifests.push(rel.clone());
        }
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ASSETS.contains(&ext.as_str()) {
            *s.assets.entry(ext.clone()).or_default() += 1;
        }
        if !LANGS.contains(&ext.as_str()) {
            continue;
        }
        let size = ent.metadata().map(|m| m.len()).unwrap_or(0);
        if size > 2_000_000 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let e = s.languages.entry(ext).or_default();
        e.0 += 1;
        e.1 += text.lines().count() as u64;
        for (n, r) in &hint_re {
            if r.is_match(&text) {
                *s.hints.entry((*n).into()).or_default() += 1;
            }
        }
        for (n, r) in &hot_re {
            let c = text.lines().filter(|l| r.is_match(l)).count() as u64;
            if c > 0 {
                *s.hotspots.entry((*n).into()).or_default() += c;
            }
        }
    }
    s
}

impl Survey {
    pub fn render(&self) -> String {
        let mut o = format!(
            "== Survey: {} ({} files{}) ==\n\n-- Languages (files / LOC)\n",
            self.root,
            self.files_scanned,
            if self.truncated { ", truncated" } else { "" }
        );
        for (k, (f, l)) in &self.languages {
            o += &format!("  .{k:<6} {f:>6} files {l:>9} LOC\n");
        }
        o += "\n-- Build manifests\n";
        for m in &self.manifests {
            o += &format!("  {m}\n");
        }
        o += "\n-- Engine / framework hints (files)\n";
        for (k, v) in &self.hints {
            o += &format!("  {k:<12} {v}\n");
        }
        o += "\n-- Assets\n";
        for (k, v) in &self.assets {
            o += &format!("  .{k:<6} {v}\n");
        }
        o += &format!(
            "\n-- Tests: {:?}  CI: {:?}\n\n-- Port risk hotspots (matching lines)\n",
            self.test_dirs, self.ci
        );
        for (k, v) in &self.hotspots {
            o += &format!("  {k:<24} {v}\n");
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surveys_a_tiny_tree() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("main.c"),
            "#include <SDL2/SDL.h>\nint x = rand();\n",
        )
        .unwrap();
        std::fs::write(d.path().join("Makefile"), "all:\n").unwrap();
        std::fs::create_dir(d.path().join("tests")).unwrap();
        let s = run(d.path(), 1000);
        assert_eq!(s.languages.get("c").map(|v| v.1), Some(2));
        assert!(s.hints.contains_key("SDL"));
        assert!(s.hotspots.contains_key("RNG"));
        assert!(s.manifests.iter().any(|m| m == "Makefile"));
        assert_eq!(s.test_dirs, vec!["tests"]);
    }
}
