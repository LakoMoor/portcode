use crate::permission::Decision;
use crate::tool::*;
use async_trait::async_trait;
use globset::Glob;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

const MAX_LINES: usize = 2000;
const MAX_LINE_CHARS: usize = 2000;

fn is_binary(b: &[u8]) -> bool {
    b.iter().take(8192).any(|x| *x == 0)
}

pub struct Read;
#[async_trait]
impl Tool for Read {
    fn name(&self) -> String {
        "read".into()
    }
    fn description(&self) -> String {
        "Read a text file (numbered lines) or list a directory. Use offset/limit for big files. For binaries it reports format, size and sha256 instead of bytes.".into()
    }
    fn example(&self) -> String {
        r#"{"path": "src/main.rs", "offset": 1, "limit": 200}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path": {"type":"string","description":"File or directory, relative to the project"}, "offset": {"type":"integer","description":"1-based first line"}, "limit": {"type":"integer","description":"Max lines (default 2000)"}}),
            &["path"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "read",
            example: &self.example(),
        };
        let path = ctx.path(&a.str("path")?, false).await?;
        let meta = std::fs::metadata(&path).map_err(|e| {
            ToolError(format!(
                "{}: {e}. Use ls or glob to find the right path.",
                ctx.rel(&path)
            ))
        })?;
        if meta.is_dir() {
            return list_dir(ctx, &path);
        }
        let bytes = std::fs::read(&path)?;
        if is_binary(&bytes) {
            let fmt = portcode_evidence::sniff(&bytes);
            return Ok(Out::new(ctx.rel(&path), format!("Binary file: {} ({} bytes, sha256 {}). Not shown as text. To study it use `pin` (identity), `bash` with strings/otool/objdump, or an MCP reverse-engineering tool.", fmt.label(), bytes.len(), portcode_evidence::sha256_hex(&bytes))));
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(m) = meta.modified() {
            lock(&ctx.state).read.insert(path.clone(), m);
        }
        let offset = a.opt_u64("offset").unwrap_or(1).max(1) as usize;
        let limit = (a.opt_u64("limit").unwrap_or(MAX_LINES as u64) as usize).clamp(1, MAX_LINES);
        let total = text.lines().count();
        let mut out = String::new();
        for (i, l) in text.lines().enumerate().skip(offset - 1).take(limit) {
            let l: String = l.chars().take(MAX_LINE_CHARS).collect();
            out += &format!("{:>5}\t{}\n", i + 1, l);
        }
        if offset + limit - 1 < total {
            out += &format!(
                "[showing lines {offset}-{} of {total}; call read again with offset={}]\n",
                offset + limit - 1,
                offset + limit
            );
        }
        if total == 0 {
            out = "[empty file]\n".into();
        }
        Ok(Out::new(
            ctx.rel(&path),
            clip(&out, ctx.output_limit.max(20_000)),
        ))
    }
}

fn list_dir(ctx: &Ctx, path: &Path) -> ToolResult {
    let mut names: Vec<String> = std::fs::read_dir(path)?
        .flatten()
        .map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() {
                format!("{n}/")
            } else {
                n
            }
        })
        .collect();
    names.sort();
    let total = names.len();
    names.truncate(300);
    Ok(Out::new(
        ctx.rel(path),
        format!(
            "{}{}",
            names.join("\n"),
            if total > 300 {
                format!("\n[... {} more]", total - 300)
            } else {
                String::new()
            }
        ),
    ))
}

pub struct Ls;
#[async_trait]
impl Tool for Ls {
    fn name(&self) -> String {
        "ls".into()
    }
    fn description(&self) -> String {
        "List a directory (directories end with /). Defaults to the project root.".into()
    }
    fn example(&self) -> String {
        r#"{"path": "src"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(json!({"path": {"type":"string"}}), &[])
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "ls",
            example: &self.example(),
        };
        let p = ctx
            .path(&a.opt_str("path").unwrap_or_else(|| ".".into()), false)
            .await?;
        if !p.is_dir() {
            return err(format!("{} is not a directory", ctx.rel(&p)));
        }
        list_dir(ctx, &p)
    }
}

fn walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_string_lossy().as_ref(),
                ".git" | "node_modules" | "target" | ".portcode"
            )
        })
        .build()
}

pub struct GlobTool;
#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> String {
        "glob".into()
    }
    fn description(&self) -> String {
        "Find files by glob pattern, newest first. Respects .gitignore. Max 200 results.".into()
    }
    fn example(&self) -> String {
        r#"{"pattern": "**/*.rs"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"pattern": {"type":"string","description":"e.g. **/*.rs or src/**/mod.rs"}, "path": {"type":"string","description":"Directory to search (default: project root)"}}),
            &["pattern"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "glob",
            example: &self.example(),
        };
        let pat = a.str("pattern")?;
        let m = Glob::new(&pat)
            .map_err(|e| ToolError(format!("bad glob '{pat}': {e}")))?
            .compile_matcher();
        let base = ctx
            .path(&a.opt_str("path").unwrap_or_else(|| ".".into()), false)
            .await?;
        let mut hits: Vec<(std::time::SystemTime, String)> = walker(&base)
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let rel = e.path().strip_prefix(&base).ok()?;
                if !m.is_match(rel) {
                    return None;
                }
                Some((e.metadata().ok()?.modified().ok()?, ctx.rel(e.path())))
            })
            .collect();
        hits.sort_by_key(|h| std::cmp::Reverse(h.0));
        let n = hits.len();
        let mut out: Vec<String> = hits.into_iter().take(200).map(|h| h.1).collect();
        if out.is_empty() {
            return Ok(Out::new(
                pat,
                "No files matched. Try a broader pattern like **/*name*.",
            ));
        }
        if n > 200 {
            out.push(format!("[... {} more; narrow the pattern]", n - 200));
        }
        Ok(Out::new(pat, out.join("\n")))
    }
}

pub struct Grep;
#[async_trait]
impl Tool for Grep {
    fn name(&self) -> String {
        "grep".into()
    }
    fn description(&self) -> String {
        "Search file contents with a regular expression. Returns file:line: text. Respects .gitignore. Max 100 matches.".into()
    }
    fn example(&self) -> String {
        r#"{"pattern": "fn main", "include": "*.rs"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"pattern": {"type":"string","description":"Rust-regex syntax"}, "path": {"type":"string"}, "include": {"type":"string","description":"Filename glob, e.g. *.rs"}}),
            &["pattern"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "grep",
            example: &self.example(),
        };
        let pat = a.str("pattern")?;
        let re = Regex::new(&pat).map_err(|e| {
            ToolError(format!(
                "bad regex '{pat}': {e}. Escape special characters with \\."
            ))
        })?;
        let inc = match a.opt_str("include") {
            Some(g) => Some(
                Glob::new(&g)
                    .map_err(|e| ToolError(format!("bad include glob: {e}")))?
                    .compile_matcher(),
            ),
            None => None,
        };
        let base = ctx
            .path(&a.opt_str("path").unwrap_or_else(|| ".".into()), false)
            .await?;
        let files: Vec<std::path::PathBuf> = if base.is_file() {
            vec![base.clone()]
        } else {
            walker(&base)
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.into_path())
                .collect()
        };
        let (mut out, mut n) = (String::new(), 0usize);
        'f: for f in files {
            if let Some(m) = &inc {
                if !f.file_name().map(|x| m.is_match(x)).unwrap_or(false) {
                    continue;
                }
            }
            if std::fs::metadata(&f)
                .map(|m| m.len() > 5_000_000)
                .unwrap_or(true)
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(&f) else {
                continue;
            };
            if is_binary(&bytes) {
                continue;
            }
            for (i, l) in String::from_utf8_lossy(&bytes).lines().enumerate() {
                if re.is_match(l) {
                    n += 1;
                    out += &format!(
                        "{}:{}: {}\n",
                        ctx.rel(&f),
                        i + 1,
                        l.chars().take(300).collect::<String>()
                    );
                    if n >= 100 {
                        out += "[stopped at 100 matches; narrow the pattern or path]\n";
                        break 'f;
                    }
                }
            }
        }
        if n == 0 {
            out = "No matches.".into();
        }
        Ok(Out::new(pat, out))
    }
}

fn stale(ctx: &Ctx, path: &Path) -> Result<(), ToolError> {
    if !path.exists() {
        return Ok(());
    }
    let seen = lock(&ctx.state).read.get(path).copied();
    match seen {
        None => err(format!("You have not read {} in this session. Call read on it first so your edit matches the real content.", ctx.rel(path))),
        Some(t) if std::fs::metadata(path).and_then(|m| m.modified()).map(|m| m > t).unwrap_or(false) => err(format!("{} changed on disk since you read it. Read it again before editing.", ctx.rel(path))),
        _ => Ok(()),
    }
}

fn write_atomic(path: &Path, content: &str) -> Result<(), ToolError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension(format!(
        "{}.pc-tmp",
        path.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, content)?;
    if let Ok(m) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn diff_summary(old: &str, new: &str) -> String {
    let d = similar::TextDiff::from_lines(old, new);
    let mut out = String::new();
    for (i, g) in d.grouped_ops(2).iter().enumerate() {
        if i >= 6 {
            out += "…\n";
            break;
        }
        for op in g {
            for c in d.iter_changes(op) {
                let s = match c.tag() {
                    similar::ChangeTag::Delete => "-",
                    similar::ChangeTag::Insert => "+",
                    similar::ChangeTag::Equal => " ",
                };
                out += &format!("{s}{}", c.value());
                if !c.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}

pub struct Write;
#[async_trait]
impl Tool for Write {
    fn name(&self) -> String {
        "write".into()
    }
    fn description(&self) -> String {
        "Create a file or fully replace one. For existing files you must `read` them first; prefer `edit` for small changes. Long file? Write the first part, then call again with append=true for each next part (keep each call under about 80 lines).".into()
    }
    fn example(&self) -> String {
        r#"{"path": "src/lib.rs", "content": "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"}"#
            .into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path": {"type":"string"}, "content": {"type":"string","description":"Complete file content, or the next part when append is true"}, "append": {"type":"boolean","description":"add to the end of the file instead of replacing it"}}),
            &["path", "content"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "write",
            example: &self.example(),
        };
        let path = ctx.path(&a.str("path")?, true).await?;
        let part = a.str("content")?;
        let append = args.get("append").and_then(Value::as_bool).unwrap_or(false);
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if !append {
            stale(ctx, &path)?;
        }
        let content = if append { format!("{old}{part}") } else { part };
        ctx.permit(
            "edit",
            &ctx.rel(&path),
            Decision::Ask,
            &format!(
                "{} {} bytes",
                if append { "append" } else { "write" },
                content.len() - old.len().min(content.len())
            ),
        )
        .await?;
        write_atomic(&path, &content)?;
        if let Ok(m) = std::fs::metadata(&path).and_then(|m| m.modified()) {
            lock(&ctx.state).read.insert(path.clone(), m);
        }
        Ok(Out::new(
            ctx.rel(&path),
            format!(
                "Wrote {} ({} lines).\n{}",
                ctx.rel(&path),
                content.lines().count(),
                clip(&diff_summary(&old, &content), 3000)
            ),
        ))
    }
}

pub struct Edit;
/// Find `old` in `text`: exact first, then ignoring per-line leading/trailing whitespace (the usual open-model slip).
fn locate(text: &str, old: &str) -> Result<Vec<(usize, usize)>, ()> {
    let exact: Vec<_> = text
        .match_indices(old)
        .map(|(i, m)| (i, i + m.len()))
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    let want: Vec<&str> = old.trim_matches('\n').lines().map(str::trim).collect();
    if want.is_empty() || want.iter().all(|l| l.is_empty()) {
        return Err(());
    }
    let lines: Vec<(usize, &str)> = {
        let mut off = 0;
        text.split_inclusive('\n')
            .map(|l| {
                let o = off;
                off += l.len();
                (o, l)
            })
            .collect()
    };
    let mut hits = Vec::new();
    for i in 0..lines.len().saturating_sub(want.len() - 1) {
        let win = lines.get(i..i + want.len()).unwrap_or(&[]);
        if win.len() == want.len() && win.iter().zip(&want).all(|((_, l), w)| l.trim() == *w) {
            let start = win.first().map(|x| x.0).unwrap_or(0);
            let end = win
                .last()
                .map(|(o, l)| o + l.trim_end_matches('\n').len())
                .unwrap_or(start);
            hits.push((start, end));
        }
    }
    Ok(hits)
}

#[async_trait]
impl Tool for Edit {
    fn name(&self) -> String {
        "edit".into()
    }
    fn description(&self) -> String {
        "Replace exact text in a file. old_string must match once (include 2-3 surrounding lines to make it unique) unless replace_all is true. You must `read` the file first.".into()
    }
    fn example(&self) -> String {
        r#"{"path": "src/main.rs", "old_string": "let x = 1;", "new_string": "let x = 2;"}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"path": {"type":"string"}, "old_string": {"type":"string"}, "new_string": {"type":"string"}, "replace_all": {"type":"boolean"}}),
            &["path", "old_string", "new_string"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let a = Args {
            v: &args,
            tool: "edit",
            example: &self.example(),
        };
        let path = ctx.path(&a.str("path")?, true).await?;
        let (old, new) = (a.str("old_string")?, a.str("new_string")?);
        if old == new {
            return err("old_string and new_string are identical; nothing to change");
        }
        if !path.is_file() {
            return err(format!(
                "{} does not exist. Use write to create it.",
                ctx.rel(&path)
            ));
        }
        stale(ctx, &path)?;
        let text = std::fs::read_to_string(&path)?;
        let hits = locate(&text, &old)
            .map_err(|_| ToolError("old_string is empty or only whitespace".into()))?;
        let all = a.bool("replace_all");
        match (hits.len(), all) {
            (0, _) => return err("old_string not found. Read the file again and copy the exact text, including indentation."),
            (n, false) if n > 1 => return err(format!("old_string matches {n} places. Add more surrounding lines to make it unique, or set replace_all=true.")),
            _ => {}
        }
        ctx.permit("edit", &ctx.rel(&path), Decision::Ask, "edit file")
            .await?;
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (s, e) in &hits {
            out.push_str(text.get(last..*s).unwrap_or(""));
            out.push_str(&new);
            last = *e;
        }
        out.push_str(text.get(last..).unwrap_or(""));
        write_atomic(&path, &out)?;
        if let Ok(m) = std::fs::metadata(&path).and_then(|m| m.modified()) {
            lock(&ctx.state).read.insert(path.clone(), m);
        }
        Ok(Out::new(
            ctx.rel(&path),
            format!(
                "Edited {} ({} replacement{}).\n{}",
                ctx.rel(&path),
                hits.len(),
                if hits.len() == 1 { "" } else { "s" },
                clip(&diff_summary(&text, &out), 3000)
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{AutoGate, Reply, Rules};
    use std::sync::Arc;

    fn ctx(d: &Path) -> Ctx {
        Ctx::new(
            d.to_path_buf(),
            Arc::new(AutoGate(Reply::Once)),
            Rules::default(),
        )
    }

    #[tokio::test]
    async fn read_edit_write_roundtrip_with_guards() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.rs"), "fn a() {\n    let x = 1;\n}\n").unwrap();
        let c = ctx(d.path());
        // editing before reading is refused
        assert!(Edit
            .run(&c, json!({"path":"a.rs","old_string":"1","new_string":"2"}))
            .await
            .is_err());
        let r = Read.run(&c, json!({"path":"a.rs"})).await.unwrap();
        assert!(r.text.contains("    2\t    let x = 1;"));
        // whitespace-tolerant match
        let e = Edit
            .run(
                &c,
                json!({"path":"a.rs","old_string":"let x = 1;","new_string":"let x = 2;"}),
            )
            .await
            .unwrap();
        assert!(e.text.contains("+    let x = 2;"), "{}", e.text);
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.rs")).unwrap(),
            "fn a() {\n    let x = 2;\n}\n"
        );
        // ambiguous match is an error that tells the model what to do
        std::fs::write(d.path().join("b.txt"), "x\nx\n").unwrap();
        Read.run(&c, json!({"path":"b.txt"})).await.unwrap();
        let amb = Edit
            .run(
                &c,
                json!({"path":"b.txt","old_string":"x","new_string":"y"}),
            )
            .await
            .unwrap_err();
        assert!(amb.0.contains("matches 2 places"));
        Edit.run(
            &c,
            json!({"path":"b.txt","old_string":"x","new_string":"y","replace_all":"true"}),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("b.txt")).unwrap(),
            "y\ny\n"
        );
    }
    #[tokio::test]
    async fn write_creates_dirs_and_detects_external_change() {
        let d = tempfile::tempdir().unwrap();
        let c = ctx(d.path());
        Write
            .run(&c, json!({"path":"x/y/z.txt","content":"hi\n"}))
            .await
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(d.path().join("x/y/z.txt"), "changed").unwrap();
        let e = Write
            .run(&c, json!({"path":"x/y/z.txt","content":"new"}))
            .await
            .unwrap_err();
        assert!(e.0.contains("changed on disk"));
    }
    #[tokio::test]
    async fn binary_files_are_summarised_and_grep_glob_work() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("bin"), b"\x7fELF\0\0\0").unwrap();
        std::fs::write(d.path().join("t.rs"), "fn needle() {}\n").unwrap();
        let c = ctx(d.path());
        assert!(Read
            .run(&c, json!({"path":"bin"}))
            .await
            .unwrap()
            .text
            .contains("ELF"));
        assert!(Grep
            .run(&c, json!({"pattern":"needle","include":"*.rs"}))
            .await
            .unwrap()
            .text
            .contains("t.rs:1:"));
        assert!(GlobTool
            .run(&c, json!({"pattern":"*.rs"}))
            .await
            .unwrap()
            .text
            .contains("t.rs"));
        assert!(Grep.run(&c, json!({"pattern":"("})).await.is_err());
    }
}
