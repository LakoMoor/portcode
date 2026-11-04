//! Static GitHub Pages site for a port: plan (PORT.md), evidence grouped by feature with screenshots, unknowns.
use portcode_evidence::{Evidence, Kind, Ledger};
use std::path::Path;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn inline(s: &str) -> String {
    // escape first, then re-introduce a safe subset: `code`, **bold**, [text](http(s)://…)
    let mut out = String::new();
    let e = esc(s);
    let mut rest = e.as_str();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('`') {
            if let Some(i) = r.find('`') {
                out += &format!("<code>{}</code>", &r[..i]);
                rest = &r[i + 1..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix("**") {
            if let Some(i) = r.find("**") {
                out += &format!("<strong>{}</strong>", &r[..i]);
                rest = &r[i + 2..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('[') {
            if let (Some(i), true) = (r.find("]("), true) {
                if let Some(j) = r[i + 2..].find(')') {
                    let (text, url) = (&r[..i], &r[i + 2..i + 2 + j]);
                    if url.starts_with("http://")
                        || url.starts_with("https://")
                        || url.starts_with('#')
                        || url.starts_with("screenshots/")
                    {
                        out += &format!("<a href=\"{url}\">{text}</a>");
                        rest = &r[i + 2 + j + 1..];
                        continue;
                    }
                }
            }
        }
        let mut cs = rest.chars();
        if let Some(c) = cs.next() {
            out.push(c);
            rest = cs.as_str();
        }
    }
    out
}

/// Minimal, safe markdown → HTML (headings, lists, tables, fenced code, paragraphs). Raw HTML is escaped.
pub fn markdown(md: &str) -> String {
    let mut out = String::new();
    let (mut in_code, mut in_list, mut in_table, mut para) = (false, false, false, String::new());
    let flush = |out: &mut String, para: &mut String| {
        if !para.is_empty() {
            *out += &format!("<p>{}</p>\n", inline(para.trim()));
            para.clear();
        }
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            flush(&mut out, &mut para);
            out += if in_code {
                "</code></pre>\n"
            } else {
                "<pre><code>"
            };
            in_code = !in_code;
            continue;
        }
        if in_code {
            out += &esc(line);
            out.push('\n');
            continue;
        }
        let t = line.trim();
        let list_item = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "));
        if in_list && list_item.is_none() {
            out += "</ul>\n";
            in_list = false;
        }
        if in_table && !t.starts_with('|') {
            out += "</tbody></table>\n";
            in_table = false;
        }
        if t.is_empty() {
            flush(&mut out, &mut para);
            continue;
        }
        if let Some(h) = t.strip_prefix("# ") {
            flush(&mut out, &mut para);
            out += &format!("<h1>{}</h1>\n", inline(h));
        } else if let Some(h) = t.strip_prefix("## ") {
            flush(&mut out, &mut para);
            out += &format!("<h2>{}</h2>\n", inline(h));
        } else if let Some(h) = t.strip_prefix("### ") {
            flush(&mut out, &mut para);
            out += &format!("<h3>{}</h3>\n", inline(h));
        } else if let Some(i) = list_item {
            flush(&mut out, &mut para);
            if !in_list {
                out += "<ul>\n";
                in_list = true;
            }
            let i = if let Some(r) = i.strip_prefix("[x] ") {
                format!("☑ {r}")
            } else if let Some(r) = i.strip_prefix("[ ] ") {
                format!("☐ {r}")
            } else {
                i.to_string()
            };
            out += &format!("<li>{}</li>\n", inline(&i));
        } else if t.starts_with('|') {
            flush(&mut out, &mut para);
            if t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                continue;
            }
            let cells: Vec<&str> = t.trim_matches('|').split('|').collect();
            if !in_table {
                out += "<table><thead><tr>";
                for c in &cells {
                    out += &format!("<th>{}</th>", inline(c.trim()));
                }
                out += "</tr></thead><tbody>\n";
                in_table = true;
            } else {
                out += "<tr>";
                for c in &cells {
                    out += &format!("<td>{}</td>", inline(c.trim()));
                }
                out += "</tr>\n";
            }
        } else {
            if !para.is_empty() {
                para.push(' ');
            }
            para += t;
        }
    }
    flush(&mut out, &mut para);
    if in_list {
        out += "</ul>\n";
    }
    if in_table {
        out += "</tbody></table>\n";
    }
    if in_code {
        out += "</code></pre>\n";
    }
    out
}

const CSS: &str = ":root{--bg:#f4f3f0;--fg:#121212;--mut:#6c6b67;--acc:#ff7a1a;--card:#fff;--line:rgba(18,18,18,.1)}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 -apple-system,BlinkMacSystemFont,Inter,Segoe UI,system-ui,sans-serif;-webkit-font-smoothing:antialiased}.wrap{max-width:980px;margin:0 auto;padding:0 22px}header{padding:48px 0 18px}header h1{font-size:clamp(2.2rem,5vw,3.4rem);letter-spacing:-.04em;margin:0 0 .3em}h1,h2,h3{line-height:1.15;letter-spacing:-.02em}h2{margin-top:2.2em;font-size:1.8rem}a{color:#2563eb}code{background:rgba(18,18,18,.06);padding:.1em .4em;border-radius:7px;font-size:.88em}pre{background:#111214;color:#e9ebf0;padding:16px 20px;border-radius:16px;overflow:auto}pre code{background:none;padding:0}table{border-collapse:separate;border-spacing:0;width:100%;margin:1em 0;font-size:.92em;background:var(--card);border-radius:16px;overflow:hidden;box-shadow:0 8px 24px rgba(20,20,20,.06)}th,td{border-bottom:1px solid var(--line);padding:10px 14px;text-align:left;vertical-align:top}tr:last-child td{border-bottom:0}th{background:#faf9f7;color:var(--mut);font-weight:600}.ev{background:var(--card);border-radius:20px;padding:16px 20px;margin:12px 0;box-shadow:0 8px 24px rgba(20,20,20,.06)}.tag{display:inline-block;font:600 12px ui-monospace,monospace;padding:2px 10px;border-radius:99px;background:rgba(18,18,18,.07);color:var(--mut);margin-right:6px}.tag.observation{background:#e5f6ec;color:#17784a}.tag.inference{background:#fff1d6;color:#8a5a00}.tag.unknown{background:#fde6e6;color:#b4232a}figure{margin:14px 0}figure img{max-width:100%;border-radius:14px;box-shadow:0 14px 40px rgba(0,0,0,.2)}figcaption{color:var(--mut);font-size:.85em;margin-top:6px}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:14px}footer{color:var(--mut);padding:40px 0;font-size:.85em}@media(prefers-color-scheme:dark){:root{--bg:#0d0e10;--fg:#f2f3f5;--mut:#9399a4;--card:#1a1c20;--line:rgba(255,255,255,.1)}a{color:#7aa7ff}code{background:rgba(255,255,255,.09)}th{background:#202328}.tag{background:rgba(255,255,255,.1)}.tag.observation{background:#12301f;color:#7ee0a8}.tag.inference{background:#3a2e12;color:#f5c969}.tag.unknown{background:#3a1b1d;color:#ff9a9f}table,.ev{box-shadow:none}}";

fn pl(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Observation => "observation",
        Kind::Inference => "inference",
        Kind::Unknown => "unknown",
    }
}

fn evidence_html(e: &Evidence) -> String {
    let mut s = format!("<div class=\"ev\" id=\"{id}\"><span class=\"tag {k}\">{k}</span><span class=\"tag\">{id}</span><span class=\"tag\">{c:?}</span><div>{st}</div>", id = esc(&e.id), k = kind_name(e.kind), c = e.confidence, st = inline(&e.statement));
    if let Some(sha) = &e.source.sha256 {
        s += &format!(
            "<div class=\"tag\">sha256 {}</div>",
            esc(sha.get(..16).unwrap_or(sha))
        );
    }
    if !e.source.tool.is_empty() {
        s += &format!(
            "<div><small>via <code>{}</code> {}</small></div>",
            esc(&e.source.tool),
            esc(&e.source.args)
        );
    }
    for a in &e.attachments {
        if a.starts_with("screenshots/") && !a.contains("..") {
            s += &format!("<figure><img loading=\"lazy\" src=\"{a}\" alt=\"{id}: {st}\"><figcaption>{id} — {a}</figcaption></figure>", a = esc(a), id = esc(&e.id), st = esc(&e.statement));
        }
    }
    s + "</div>\n"
}

/// Wrap rendered markdown in the PortCode docs layout (nav + footer). `css` is the relative path to style.css.
pub fn doc_page(title: &str, md: &str, css: &str, home: &str) -> String {
    format!("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t} — PortCode</title><meta property=\"og:title\" content=\"{t} — PortCode\"><link rel=\"icon\" type=\"image/svg+xml\" href=\"{home}assets/logo.svg\"><script src=\"{home}theme.js\"></script><link rel=\"stylesheet\" href=\"{css}\"></head><body>\n<nav><div class=\"wrap\"><a class=\"brand\" href=\"{home}\"><img src=\"{home}assets/logo.svg\" alt=\"\">PortCode</a><a class=\"l\" href=\"{home}guide/ARCHITECTURE.html\">Architecture</a><a class=\"l\" href=\"{home}guide/PROVIDERS.html\">Providers</a><a class=\"l\" href=\"{home}guide/OPENCODE_MAPPING.html\">OpenCode mapping</a><a class=\"l\" href=\"{home}guide/ENGINES.html\">Engines</a><a class=\"l\" href=\"{home}guide/ROADMAP.html\">Roadmap</a><a class=\"l\" href=\"{home}remote/\">Remote</a><a class=\"gh\" href=\"https://github.com/LakoMoor/portcode\">GitHub</a></div></nav>\n<main class=\"doc\">\n{body}</main>\n<footer><div class=\"wrap\">MIT licensed · generated by <code>portcode port page</code></div></footer></body></html>\n", t = esc(title), css = css, home = home, body = markdown(md))
}

/// Build the site. `project` holds `docs/PORT.md`, `docs/screenshots/`, `.portcode/evidence/`. Returns files written.
pub fn generate(
    project: &Path,
    out: &Path,
    title: &str,
    repo_url: Option<&str>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(out.join("screenshots")).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    let plan = std::fs::read_to_string(project.join("docs/PORT.md"))
        .unwrap_or_else(|_| "_No docs/PORT.md yet — run `portcode port scaffold`._".into());
    let ledger = Ledger::open(project.join(".portcode/evidence"));
    let evs = ledger.all().map_err(|e| e.to_string())?;

    // copy screenshots (png/svg/jpg/webp/gif only; flat names)
    let mut shots: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(project.join("docs/screenshots")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
            if matches!(
                ext.as_str(),
                "png" | "svg" | "jpg" | "jpeg" | "webp" | "gif"
            ) {
                std::fs::copy(e.path(), out.join("screenshots").join(&name))
                    .map_err(|e| e.to_string())?;
                shots.push(name);
            }
        }
    }
    shots.sort();

    let mut subjects: Vec<&str> = evs.iter().map(|e| e.subject.as_str()).collect();
    subjects.sort();
    subjects.dedup();
    let open = ledger.open_unknowns().map_err(|e| e.to_string())?;
    let (obs, inf) = (
        evs.iter().filter(|e| e.kind == Kind::Observation).count(),
        evs.iter().filter(|e| e.kind == Kind::Inference).count(),
    );

    let mut body = format!("<header><div class=\"wrap\"><h1>{}</h1><p>Rust port documented with evidence. {} observation{} · {} inference{} · {} open unknown{} · {} screenshot{}.{}</p></div></header>\n<main class=\"wrap\">\n", esc(title), obs, pl(obs), inf, pl(inf), open.len(), pl(open.len()), shots.len(), pl(shots.len()),
        repo_url.filter(|u| u.starts_with("https://")).map(|u| format!(" <a href=\"{}\">Source ↗</a>", esc(u))).unwrap_or_default());
    body += "<h2 id=\"plan\">Port plan</h2>\n";
    body += &markdown(&plan);
    body += "<h2 id=\"evidence\">Evidence by feature</h2>\n";
    if evs.is_empty() {
        body += "<p>No evidence recorded yet.</p>\n";
    }
    for sub in subjects {
        body += &format!("<h3>{}</h3>\n", esc(sub));
        for e in evs.iter().filter(|e| e.subject == sub) {
            body += &evidence_html(e);
        }
    }
    body += "<h2 id=\"unknowns\">Open unknowns</h2>\n";
    if open.is_empty() {
        body += "<p>None recorded.</p>\n";
    } else {
        for e in &open {
            body += &evidence_html(e);
        }
    }
    body += "<h2 id=\"gallery\">Screenshots</h2>\n<div class=\"grid\">\n";
    for s in &shots {
        body += &format!("<figure><a href=\"screenshots/{n}\"><img loading=\"lazy\" src=\"screenshots/{n}\" alt=\"{n}\"></a><figcaption>{n}</figcaption></figure>\n", n = esc(s));
    }
    body += "</div>\n</main>\n<footer class=\"wrap\">Generated by PortCode. Screenshots are captures of the real original and the real port; unverified claims are marked as inference or unknown.</footer>\n";

    let html = format!("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{t}</title><link rel=\"stylesheet\" href=\"style.css\"></head><body>\n{body}</body></html>\n", t = esc(title));
    for (name, content) in [
        ("index.html", html.as_str()),
        ("style.css", CSS),
        (".nojekyll", ""),
    ] {
        std::fs::write(out.join(name), content).map_err(|e| e.to_string())?;
        written.push(name.to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use portcode_evidence::{Confidence, Source};
    #[test]
    fn markdown_escapes_html_and_blocks_js_links() {
        let h = markdown("# T\n<script>x</script>\n[a](javascript:alert(1)) [b](https://x.y)\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(!h.contains("<script>") && !h.contains("href=\"javascript"));
        assert!(h.contains("<a href=\"https://x.y\">b</a>") && h.contains("<td>1</td>"));
    }
    #[test]
    fn site_includes_evidence_and_screenshots() {
        let p = tempfile::tempdir().unwrap();
        let o = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(p.path().join("docs/screenshots")).unwrap();
        std::fs::write(p.path().join("docs/screenshots/a.svg"), "<svg/>").unwrap();
        Ledger::open(p.path().join(".portcode/evidence"))
            .add(Evidence {
                id: String::new(),
                kind: Kind::Observation,
                subject: "cli".into(),
                statement: "prints <b>hi</b>".into(),
                confidence: Confidence::High,
                source: Source::default(),
                depends_on: vec![],
                attachments: vec!["screenshots/a.svg".into()],
                resolved_by: None,
                created: 0,
            })
            .unwrap();
        generate(p.path(), o.path(), "Demo", None).unwrap();
        let html = std::fs::read_to_string(o.path().join("index.html")).unwrap();
        assert!(
            html.contains("E-0001")
                && html.contains("screenshots/a.svg")
                && !html.contains("<b>hi</b>")
        );
        assert!(
            o.path().join("screenshots/a.svg").is_file() && o.path().join(".nojekyll").is_file()
        );
    }
}
