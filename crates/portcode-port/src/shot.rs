//! Terminal "screenshots": render captured command output as an SVG window. Pure Rust, no display needed.
//! Honest by construction: the picture is the captured text, and the caller records its sha256 in the ledger.

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const PALETTE: [&str; 16] = [
    "#484f58", "#ff7b72", "#7ee787", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#c9d1d9",
    "#6e7681", "#ffa198", "#56d364", "#f2cc60", "#a5d6ff", "#e2c5ff", "#b3f0ff", "#f0f6fc",
];

fn xterm(n: u32) -> String {
    match n {
        0..=15 => PALETTE
            .get(n as usize)
            .copied()
            .unwrap_or("#c9d1d9")
            .to_string(),
        16..=231 => {
            let n = n - 16;
            let c = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
            format!("#{:02x}{:02x}{:02x}", c(n / 36), c((n / 6) % 6), c(n % 6))
        }
        _ => {
            let v = 8 + (n.saturating_sub(232)) * 10;
            format!("#{v:02x}{v:02x}{v:02x}")
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct Style {
    fg: Option<String>,
    bold: bool,
    dim: bool,
}

/// Split text into (style, text) spans following SGR escapes (colors 30-37/90-97/38;5;n/38;2;r;g;b, bold, dim, reset).
fn spans(s: &str) -> Vec<(Style, String)> {
    let (mut out, mut st, mut cur) = (Vec::new(), Style::default(), String::new());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' {
            continue;
        }
        if c != '\u{1b}' {
            cur.push(c);
            continue;
        }
        match it.peek() {
            Some('[') => {
                it.next();
                let mut params = String::new();
                let mut fin = ' ';
                for n in it.by_ref() {
                    if ('@'..='~').contains(&n) {
                        fin = n;
                        break;
                    }
                    params.push(n);
                }
                if fin != 'm' {
                    continue;
                }
                if !cur.is_empty() {
                    out.push((st.clone(), std::mem::take(&mut cur)));
                }
                let nums: Vec<u32> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
                let mut i = 0;
                while i < nums.len() {
                    let n = nums.get(i).copied().unwrap_or(0);
                    match n {
                        0 => st = Style::default(),
                        1 => st.bold = true,
                        2 => st.dim = true,
                        22 => {
                            st.bold = false;
                            st.dim = false;
                        }
                        30..=37 => st.fg = Some(xterm(n - 30)),
                        90..=97 => st.fg = Some(xterm(n - 90 + 8)),
                        39 => st.fg = None,
                        38 => match nums.get(i + 1) {
                            Some(5) => {
                                st.fg = Some(xterm(nums.get(i + 2).copied().unwrap_or(7)));
                                i += 2;
                            }
                            Some(2) => {
                                let g = |k: usize| nums.get(i + k).copied().unwrap_or(0).min(255);
                                st.fg = Some(format!("#{:02x}{:02x}{:02x}", g(2), g(3), g(4)));
                                i += 4;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    i += 1;
                }
            }
            Some(']') => {
                it.next();
                for n in it.by_ref() {
                    if n == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {
                it.next();
            }
        }
    }
    if !cur.is_empty() {
        out.push((st, cur));
    }
    out
}

/// Remove ANSI escape sequences (CSI and OSC) from captured output.
pub fn strip_ansi(s: &str) -> String {
    spans(s).into_iter().map(|(_, t)| t).collect()
}

/// Render `$ command` plus its output (ANSI colours kept) as an SVG terminal window.
pub fn render_svg(
    title: &str,
    command: &str,
    output: &str,
    cols: usize,
    max_lines: usize,
) -> String {
    // wrap by visible characters, keeping styles
    let mut lines: Vec<Vec<(Style, String)>> = vec![vec![(
        Style {
            fg: Some("#7ee787".into()),
            ..Default::default()
        },
        format!("$ {command}"),
    )]];
    let mut cur: Vec<(Style, String)> = Vec::new();
    let mut width = 0usize;
    for (st, text) in spans(output) {
        for ch in text.chars() {
            if ch == '\n' {
                lines.push(std::mem::take(&mut cur));
                width = 0;
                continue;
            }
            let ch = if ch == '\t' { ' ' } else { ch };
            if width >= cols.max(20) {
                lines.push(std::mem::take(&mut cur));
                width = 0;
            }
            match cur.last_mut() {
                Some((s2, t)) if *s2 == st => t.push(ch),
                _ => cur.push((st.clone(), ch.to_string())),
            }
            width += 1;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    let truncated = lines.len() > max_lines;
    lines.truncate(max_lines);
    if truncated {
        lines.push(vec![(
            Style {
                fg: Some("#8b949e".into()),
                ..Default::default()
            },
            "… output truncated".into(),
        )]);
    }
    let (cw, lh, pad, top) = (8.8_f64, 19.0_f64, 18.0_f64, 44.0_f64);
    let chars = |l: &Vec<(Style, String)>| l.iter().map(|(_, t)| t.chars().count()).sum::<usize>();
    let w = (lines.iter().map(chars).max().unwrap_or(40).max(60) as f64 * cw + pad * 2.0).ceil();
    let h = (top + lines.len() as f64 * lh + pad).ceil();
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{}\">\n<rect width=\"{w}\" height=\"{h}\" rx=\"10\" fill=\"#0d1117\"/>\n<rect width=\"{w}\" height=\"30\" rx=\"10\" fill=\"#161b22\"/><rect y=\"20\" width=\"{w}\" height=\"10\" fill=\"#161b22\"/>\n<circle cx=\"18\" cy=\"15\" r=\"5\" fill=\"#ff5f56\"/><circle cx=\"36\" cy=\"15\" r=\"5\" fill=\"#ffbd2e\"/><circle cx=\"54\" cy=\"15\" r=\"5\" fill=\"#27c93f\"/>\n<text x=\"{}\" y=\"19\" fill=\"#8b949e\" font-family=\"ui-monospace,Menlo,monospace\" font-size=\"12\" text-anchor=\"middle\">{}</text>\n<g font-family=\"ui-monospace,SFMono-Regular,Menlo,Consolas,monospace\" font-size=\"14\" fill=\"#c9d1d9\" xml:space=\"preserve\">\n",
        esc(title), w / 2.0, esc(title));
    for (i, l) in lines.iter().enumerate() {
        s += &format!(
            "<text x=\"{pad}\" y=\"{}\">",
            top + (i as f64 + 1.0) * lh - 5.0
        );
        for (st, t) in l {
            let fill = st
                .fg
                .as_deref()
                .map(|c| format!(" fill=\"{c}\""))
                .unwrap_or_default();
            let extra = format!(
                "{}{}",
                if st.bold { " font-weight=\"700\"" } else { "" },
                if st.dim { " opacity=\"0.6\"" } else { "" }
            );
            // NBSP keeps column alignment in every SVG renderer (xml:space is ignored by some)
            s += &format!(
                "<tspan{fill}{extra}>{}</tspan>",
                esc(t).replace(' ', "\u{a0}")
            );
        }
        s += "</text>\n";
    }
    s + "</g>\n</svg>\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_ansi_and_escapes_xml() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m\r\n"), "red\n");
        assert!(render_svg("t", "c", "  the     11", 100, 10).contains("\u{a0}\u{a0}the\u{a0}"));
        let colored = render_svg(
            "t",
            "c",
            "\u{1b}[38;5;208mhot\u{1b}[0m plain \u{1b}[2mdim\u{1b}[0m",
            100,
            10,
        );
        assert!(
            colored.contains("fill=\"#ff8700\"")
                && colored.contains("opacity=\"0.6\"")
                && !colored.contains("[38")
        );
        let svg = render_svg("t", "echo <x>", "a & b", 100, 10);
        assert!(
            svg.contains("&lt;x&gt;")
                && svg.contains("a\u{a0}&amp;\u{a0}b")
                && svg.starts_with("<svg")
        );
    }
    #[test]
    fn truncates_long_output() {
        let out = (0..100)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(render_svg("t", "c", &out, 80, 10).contains("output\u{a0}truncated"));
    }
}
