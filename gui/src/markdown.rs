//! A small Markdown reader for model replies: headings, paragraphs, bullet lists, fenced code and inline `code` / **bold**.
//! No rendering here; `view` turns the blocks into elements.
use std::ops::Range;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Block {
    Heading(u8, String),
    Para(String),
    Bullets(Vec<String>),
    Code { lang: String, text: String },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Mark {
    Code,
    Bold,
}

pub fn blocks(src: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut bullets: Vec<String> = Vec::new();
    let mut code: Option<(String, Vec<&str>)> = None;
    fn flush_para(out: &mut Vec<Block>, para: &mut Vec<&str>) {
        if !para.is_empty() {
            out.push(Block::Para(para.join(" ")));
            para.clear();
        }
    }
    fn flush_bullets(out: &mut Vec<Block>, b: &mut Vec<String>) {
        if !b.is_empty() {
            out.push(Block::Bullets(std::mem::take(b)));
        }
    }
    for line in src.lines() {
        if let Some((lang, body)) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                let text = body.join("\n");
                out.push(Block::Code {
                    lang: std::mem::take(lang),
                    text,
                });
                code = None;
            } else {
                body.push(line);
            }
            continue;
        }
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            flush_para(&mut out, &mut para);
            flush_bullets(&mut out, &mut bullets);
            code = Some((rest.trim().to_string(), Vec::new()));
        } else if t.is_empty() {
            flush_para(&mut out, &mut para);
            flush_bullets(&mut out, &mut bullets);
        } else if let Some(h) = heading(t) {
            flush_para(&mut out, &mut para);
            flush_bullets(&mut out, &mut bullets);
            out.push(h);
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            flush_para(&mut out, &mut para);
            bullets.push(b.trim().to_string());
        } else {
            flush_bullets(&mut out, &mut bullets);
            para.push(t.trim_end());
        }
    }
    // a reply that is still streaming can end inside a fence: show what there is
    if let Some((lang, body)) = code {
        out.push(Block::Code {
            lang,
            text: body.join("\n"),
        });
    }
    flush_para(&mut out, &mut para);
    flush_bullets(&mut out, &mut bullets);
    out
}

fn heading(t: &str) -> Option<Block> {
    let n = t.chars().take_while(|c| *c == '#').count();
    if (1..=4).contains(&n) && t.chars().nth(n) == Some(' ') {
        let text = t.get(n + 1..)?.trim().to_string();
        return Some(Block::Heading(n as u8, text));
    }
    None
}

/// Inline marks of `s`: the text without the markers, and the byte ranges (in that text) to style.
pub fn inline(s: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let next = rest
            .char_indices()
            .find(|(i, c)| (*c == '`') || (*c == '*' && rest[*i..].starts_with("**")));
        let Some((i, c)) = next else {
            text.push_str(rest);
            break;
        };
        text.push_str(&rest[..i]);
        let (open, mark) = if c == '`' {
            ("`", Mark::Code)
        } else {
            ("**", Mark::Bold)
        };
        let after = &rest[i + open.len()..];
        match after.find(open).filter(|e| *e > 0) {
            Some(e) => {
                let start = text.len();
                text.push_str(&after[..e]);
                marks.push((start..text.len(), mark));
                rest = &after[e + open.len()..];
            }
            None => {
                // not closed (yet): keep the marker as typed
                text.push_str(open);
                rest = after;
            }
        }
    }
    (text, marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_block_kinds() {
        let b =
            blocks("# Title\n\nSome text\nmore text\n\n- a\n- b\n\n```rust\nfn x() {}\n```\nafter");
        assert_eq!(b[0], Block::Heading(1, "Title".into()));
        assert_eq!(b[1], Block::Para("Some text more text".into()));
        assert_eq!(b[2], Block::Bullets(vec!["a".into(), "b".into()]));
        assert_eq!(
            b[3],
            Block::Code {
                lang: "rust".into(),
                text: "fn x() {}".into()
            }
        );
        assert_eq!(b[4], Block::Para("after".into()));
    }

    #[test]
    fn an_open_fence_still_shows_its_code() {
        let b = blocks("run:\n```sh\ncargo te");
        assert_eq!(
            b[1],
            Block::Code {
                lang: "sh".into(),
                text: "cargo te".into()
            }
        );
    }

    #[test]
    fn inline_marks_give_ranges_in_the_clean_text() {
        let (t, m) = inline("use `cargo test` and **fix** it");
        assert_eq!(t, "use cargo test and fix it");
        assert_eq!(m[0], (4..14, Mark::Code));
        assert_eq!(m[1], (19..22, Mark::Bold));
        let (t, m) = inline("a ` b");
        assert_eq!(t, "a ` b");
        assert!(m.is_empty());
    }
}
