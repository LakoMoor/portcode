//! Rendering: home screen, session view (transcript + prompt + footer + sidebar), popups, permission dialog.
use super::app::{summarize, App, Item, ToolState, SLASH};
use super::theme::*;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn wid(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// Greedy word-wrap of styled text to `width` columns; continuation lines get `indent` spaces.
fn wrap(spans: Vec<Span<'static>>, width: usize, indent: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(8);
    let flat: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();
    let (mut lines, mut start) = (Vec::new(), 0usize);
    let mut first = true;
    while start < flat.len() || first {
        let avail = if first {
            width
        } else {
            width.saturating_sub(indent).max(4)
        };
        let (mut w, mut end, mut last_space) = (0usize, start, None);
        while let Some((c, _)) = flat.get(end) {
            if w + wid(*c) > avail {
                break;
            }
            w += wid(*c);
            if *c == ' ' {
                last_space = Some(end);
            }
            end += 1;
        }
        let cut = if end < flat.len() && end > start {
            last_space
                .filter(|s| *s > start)
                .map(|s| s + 1)
                .unwrap_or(end)
        } else {
            end
        };
        let cut = if cut == start && start < flat.len() {
            start + 1
        } else {
            cut
        };
        let mut line: Vec<Span<'static>> = Vec::new();
        if !first {
            line.push(Span::raw(" ".repeat(indent)));
        }
        let mut cur = String::new();
        let mut cs = None;
        for (c, st) in flat.get(start..cut).unwrap_or(&[]) {
            if cs != Some(*st) {
                if let Some(s) = cs {
                    line.push(Span::styled(std::mem::take(&mut cur), s));
                }
                cs = Some(*st);
            }
            cur.push(*c);
        }
        if let Some(s) = cs {
            line.push(Span::styled(cur.trim_end().to_string(), s));
        }
        lines.push(line);
        start = cut;
        first = false;
        if start >= flat.len() {
            break;
        }
    }
    lines
}

fn inline_md(text: &str, base: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    for (i, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            out.push(Span::styled(
                part.to_string(),
                Style::new().fg(ACCENT).bg(PANEL2),
            ));
        } else {
            for (j, b) in part.split("**").enumerate() {
                if b.is_empty() {
                    continue;
                }
                out.push(Span::styled(
                    b.to_string(),
                    if j % 2 == 1 {
                        base.add_modifier(Modifier::BOLD)
                    } else {
                        base
                    },
                ));
            }
        }
    }
    out
}

fn md_lines(text: &str, width: usize, indent: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut code = false;
    for raw in text.lines() {
        if raw.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        let pad = Span::raw(indent.to_string());
        if code {
            out.push(Line::from(vec![
                pad,
                Span::styled(format!(" {raw} "), Style::new().fg(TEXT).bg(PANEL2)),
            ]));
            continue;
        }
        let (spans, ind) = if let Some(h) = raw
            .strip_prefix("# ")
            .or_else(|| raw.strip_prefix("## "))
            .or_else(|| raw.strip_prefix("### "))
        {
            (vec![Span::styled(h.to_string(), bold(ACCENT))], 0)
        } else if let Some(r) = raw
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| raw.trim_start().strip_prefix("* "))
        {
            let mut v = vec![Span::styled("• ", Style::new().fg(ACCENT))];
            v.extend(inline_md(r, text_style()));
            (v, 2)
        } else {
            (inline_md(raw, text_style()), 0)
        };
        for l in wrap(spans, width.saturating_sub(indent.chars().count()), ind) {
            let mut v = vec![Span::raw(indent.to_string())];
            v.extend(l);
            out.push(Line::from(v));
        }
        if raw.is_empty() {
            out.push(Line::from(""));
        }
    }
    out
}
fn text_style() -> Style {
    text()
}

fn pad_line(
    mut spans: Vec<Span<'static>>,
    width: usize,
    bg: ratatui::style::Color,
) -> Line<'static> {
    let used: usize = spans
        .iter()
        .map(|s| s.content.chars().map(wid).sum::<usize>())
        .sum();
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
    Line::from(spans).style(Style::new().bg(bg))
}

fn transcript(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let spin = SPIN[(app.spinner / 2) % SPIN.len()];
    for it in &app.items {
        match it {
            Item::User(t) => {
                out.push(Line::from(""));
                out.push(pad_line(
                    vec![Span::styled("┃", Style::new().fg(BLUE).bg(PANEL))],
                    width,
                    PANEL,
                ));
                for raw in t.lines() {
                    for l in wrap(
                        vec![Span::styled(raw.to_string(), text())],
                        width.saturating_sub(3),
                        0,
                    ) {
                        let mut v = vec![Span::styled("┃ ", Style::new().fg(BLUE).bg(PANEL))];
                        v.extend(
                            l.into_iter()
                                .map(|s| Span::styled(s.content, s.style.bg(PANEL))),
                        );
                        out.push(pad_line(v, width, PANEL));
                    }
                }
                out.push(pad_line(
                    vec![Span::styled("┃", Style::new().fg(BLUE).bg(PANEL))],
                    width,
                    PANEL,
                ));
                out.push(Line::from(""));
            }
            Item::Assistant(t) => {
                if out.last().is_some_and(|l| !l.spans.is_empty()) {
                    out.push(Line::from(""));
                }
                out.extend(md_lines(t, width, "  "));
                out.push(Line::from(""));
            }
            Item::Reasoning(t) => {
                if app.show_reasoning {
                    for l in wrap(
                        vec![Span::styled(
                            format!("✻ {}", t.trim()),
                            muted().add_modifier(Modifier::ITALIC),
                        )],
                        width.saturating_sub(2),
                        2,
                    ) {
                        let mut v = vec![Span::raw("  ")];
                        v.extend(l);
                        out.push(Line::from(v));
                    }
                } else {
                    out.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("✻ thinking… {} chars", t.chars().count()), muted()),
                    ]));
                }
            }
            Item::Tool {
                name,
                args,
                state,
                title,
                text: body,
            } => {
                let (icon, col) = match state {
                    ToolState::Running => (spin, YELLOW),
                    ToolState::Ok => ("✓", GREEN),
                    ToolState::Err => ("✗", RED),
                };
                let sum =
                    if matches!(state, ToolState::Running) || title.is_empty() || title == name {
                        summarize(name, args)
                    } else {
                        title.clone()
                    };
                let head = vec![
                    Span::raw("  "),
                    Span::styled(format!("{icon} "), Style::new().fg(col)),
                    Span::styled(name.clone(), bold(tool_color(name))),
                    Span::styled(format!("  {sum}"), muted()),
                ];
                out.extend(wrap(head, width, 6).into_iter().map(Line::from));
                if !matches!(state, ToolState::Running) && !body.trim().is_empty() {
                    let max = if *state == ToolState::Err { 4 } else { 3 };
                    let lines: Vec<&str> = body.lines().collect();
                    for l in lines.iter().take(max) {
                        let l: String = l.chars().take(width.saturating_sub(8)).collect();
                        let st = if l.starts_with('+') && !l.starts_with("++") {
                            Style::new().fg(GREEN)
                        } else if (l.starts_with('-') && !l.starts_with("--"))
                            || *state == ToolState::Err
                        {
                            Style::new().fg(RED)
                        } else {
                            muted()
                        };
                        out.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled("│ ", Style::new().fg(RULE)),
                            Span::styled(l, st),
                        ]));
                    }
                    if lines.len() > max {
                        out.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled("╰ ", Style::new().fg(RULE)),
                            Span::styled(format!("{} more lines", lines.len() - max), muted()),
                        ]));
                    }
                }
            }
            Item::Info(t) => {
                out.extend(
                    wrap(
                        vec![Span::styled(format!("· {t}"), muted())],
                        width.saturating_sub(2),
                        2,
                    )
                    .into_iter()
                    .map(|l| {
                        let mut v = vec![Span::raw("  ")];
                        v.extend(l);
                        Line::from(v)
                    }),
                );
            }
            Item::Error(t) => {
                out.extend(
                    wrap(
                        vec![Span::styled(format!("✗ {t}"), Style::new().fg(RED))],
                        width.saturating_sub(2),
                        2,
                    )
                    .into_iter()
                    .map(|l| {
                        let mut v = vec![Span::raw("  ")];
                        v.extend(l);
                        Line::from(v)
                    }),
                );
                out.push(Line::from(""));
            }
            Item::Footer(t) => {
                out.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("▣ ", Style::new().fg(agent_color(&app.agent))),
                    Span::styled(t.clone(), muted()),
                ]));
                out.push(Line::from(""));
            }
        }
    }
    if app.busy
        && !matches!(
            app.items.last(),
            Some(Item::Tool {
                state: ToolState::Running,
                ..
            })
        )
    {
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{spin} working…"),
                Style::new().fg(agent_color(&app.agent)),
            ),
        ]));
    }
    out
}

fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(4);
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let (mut cur, mut w) = (String::new(), 0);
        for c in raw.chars() {
            if w + wid(c) > width {
                out.push(std::mem::take(&mut cur));
                w = 0;
            }
            cur.push(c);
            w += wid(c);
        }
        out.push(cur);
    }
    out
}

fn cursor_pos(text: &str, cursor: usize, width: usize) -> (usize, usize) {
    let before: String = text.chars().take(cursor).collect();
    let lines = hard_wrap(&before, width);
    // a cursor sitting exactly at the wrap boundary moves to the next row
    let last = lines.last().cloned().unwrap_or_default();
    let w: usize = last.chars().map(wid).sum();
    if w >= width.max(4) {
        (lines.len(), 0)
    } else {
        (lines.len().saturating_sub(1), w)
    }
}

fn prompt(f: &mut Frame, app: &App, area: Rect, focus: bool) -> (u16, u16) {
    let col = agent_color(&app.agent);
    let inner_w = area.width.saturating_sub(4) as usize;
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_type(BorderType::Thick)
        .border_style(Style::new().fg(col))
        .style(Style::new().bg(PANEL));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = hard_wrap(&app.input, inner_w);
    let mut lines: Vec<Line> = vec![Line::from("")];
    if app.input.is_empty() {
        let hint = if app.busy {
            "working… esc to interrupt"
        } else {
            match app.agent.as_str() {
                "reverse" => {
                    "Ask anything… \"What file format is save.dat? Record it as evidence\""
                }
                "plan" => "Ask anything… \"Plan the port of this module, read-only\"",
                "build" => "Ask anything… \"Fix the failing test in crates/core\"",
                _ => "Ask anything… \"Port this CLI to Rust, following rustport\"",
            }
        };
        lines.push(Line::from(Span::styled(format!(" {hint}"), muted())));
    } else {
        for r in rows.iter().take(8) {
            lines.push(Line::from(Span::styled(format!(" {r}"), text())));
        }
    }
    while lines.len() < inner.height.saturating_sub(2) as usize {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(""));
    let mut meta = vec![
        Span::raw(" "),
        Span::styled(cap(&app.agent), bold(col)),
        Span::styled(format!("  {}", app.model), text()),
        Span::styled(format!("  {}", app.provider), muted()),
        Span::raw("   "),
    ];
    meta.extend(mode_chip(app));
    lines.push(Line::from(meta));
    f.render_widget(Paragraph::new(lines), inner);
    let (r, c) = cursor_pos(&app.input, app.cursor, inner_w);
    if focus && app.perm.is_none() {
        f.set_cursor_position((inner.x + 1 + c as u16, inner.y + 1 + r.min(7) as u16));
    }
    (area.x, area.y)
}

/// The mode chip: sealed teal for clean-room, a filled amber chip for deep (it is the riskier mode, so it shouts).
fn mode_chip(app: &App) -> Vec<Span<'static>> {
    if app.deep {
        vec![Span::styled(
            " ◉ deep ",
            Style::new().fg(BG).bg(YELLOW).add_modifier(Modifier::BOLD),
        )]
    } else {
        vec![Span::styled("▣ clean-room", Style::new().fg(TEAL))]
    }
}

/// `▰▰▰▱▱▱` bar for a 0..=100 percentage.
fn bar(pct: u64, cells: usize, on: ratatui::style::Color) -> Vec<Span<'static>> {
    let filled = ((pct.min(100) as usize * cells) + 50) / 100;
    vec![
        Span::styled("▰".repeat(filled), Style::new().fg(on)),
        Span::styled(
            "▱".repeat(cells - filled.min(cells)),
            Style::new().fg(BORDER),
        ),
    ]
}

/// Section title with a hairline rule filling the row.
fn section(title: &str, w: usize) -> Line<'static> {
    let used = title.chars().count() + 1;
    Line::from(vec![
        Span::styled(title.to_string(), bold(TEXT)),
        Span::styled(
            format!(" {}", "─".repeat(w.saturating_sub(used))),
            Style::new().fg(RULE),
        ),
    ])
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|h| h.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn prompt_height(app: &App, width: u16) -> u16 {
    let rows = hard_wrap(&app.input, width.saturating_sub(4) as usize)
        .len()
        .clamp(1, 8) as u16;
    rows + 3
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn footer(f: &mut Frame, app: &App, area: Rect) {
    let hints = if app.busy {
        "esc interrupt".to_string()
    } else {
        "tab agents · / commands · ctrl+b sidebar".to_string()
    };
    let mut left = vec![Span::styled(format!(" {}", app.cwd), muted())];
    if let Some((ok, t)) = &app.parity {
        left.push(Span::styled(
            format!("  {} {t}", if *ok { "✓" } else { "✗" }),
            Style::new().fg(if *ok { GREEN } else { RED }),
        ));
    }
    if let Some(a) = &app.remote_addr {
        left.push(Span::styled(
            format!("  ● remote {a}"),
            Style::new().fg(CYAN),
        ));
    }
    let used: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let right = format!("{hints}  ");
    let gap = (area.width as usize).saturating_sub(used + right.chars().count());
    left.push(Span::raw(" ".repeat(gap)));
    left.push(Span::styled(right, muted()));
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

fn sidebar(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(Style::new().fg(RULE))
        .style(Style::new().bg(PANEL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    let mut l: Vec<Line> = Vec::new();
    for r in wrap(
        vec![Span::styled(
            app.title.clone().unwrap_or_else(|| "New session".into()),
            bold(ACCENT),
        )],
        w,
        0,
    )
    .into_iter()
    .take(2)
    {
        l.push(Line::from(r));
    }
    l.push(Line::from(""));

    l.push(section("Mode", w));
    let mut m = mode_chip(app);
    m.push(Span::styled(
        if app.deep {
            "  inside allowed"
        } else {
            "  outside only"
        },
        muted(),
    ));
    l.push(Line::from(m));
    l.push(Line::from(""));

    l.push(section("Context", w));
    let pct = app.context_pct();
    let mut c = bar(
        pct,
        14,
        if pct > 80 {
            RED
        } else if pct > 60 {
            YELLOW
        } else {
            ACCENT
        },
    );
    c.push(Span::styled(format!(" {pct}%"), muted()));
    l.push(Line::from(c));
    l.push(Line::from(Span::styled(
        format!(
            "{} of {} · {} in / {} out",
            fmt_tokens(app.last_prompt),
            fmt_tokens(u64::from(app.ctx_window)),
            fmt_tokens(app.tokens_in),
            fmt_tokens(app.tokens_out)
        ),
        muted(),
    )));
    l.push(Line::from(""));

    l.push(section("Evidence", w));
    let (o, i, u) = app.ledger;
    let total = (o + i + u).max(1);
    let cells = 14usize;
    let seg = |n: usize| (n * cells + total / 2) / total;
    let (so, si) = (seg(o), seg(i));
    let su = cells.saturating_sub(so + si);
    if o + i + u > 0 {
        l.push(Line::from(vec![
            Span::styled("█".repeat(so), Style::new().fg(GREEN)),
            Span::styled("█".repeat(si), Style::new().fg(YELLOW)),
            Span::styled("█".repeat(su), Style::new().fg(RED)),
        ]));
    }
    l.push(Line::from(vec![
        Span::styled(format!("{o} seen  "), Style::new().fg(GREEN)),
        Span::styled(format!("{i} inferred  "), Style::new().fg(YELLOW)),
        Span::styled(format!("{u} open"), Style::new().fg(RED)),
    ]));
    for un in app.unknowns.iter().take(2) {
        for r in wrap(
            vec![Span::styled(un.clone(), muted())],
            w.saturating_sub(2),
            0,
        )
        .into_iter()
        .take(2)
        {
            let mut v = vec![Span::styled("? ", Style::new().fg(RED))];
            v.extend(r);
            l.push(Line::from(v));
        }
    }
    l.push(Line::from(""));

    l.push(section("Proof", w));
    l.push(Line::from(match &app.parity {
        Some((true, t)) => Span::styled(format!("✓ {t}"), Style::new().fg(GREEN)),
        Some((false, t)) => Span::styled(format!("✗ {t}"), Style::new().fg(RED)),
        None => Span::styled("no parity run yet", muted()),
    }));

    if !app.todos.is_empty() {
        l.push(Line::from(""));
        l.push(section("Todo", w));
        for t in &app.todos {
            let (m, c) = match t.status.as_str() {
                "completed" => ("☑", GREEN),
                "in_progress" => ("◐", YELLOW),
                _ => ("☐", MUTED),
            };
            let st = if t.status == "completed" {
                muted()
            } else {
                text()
            };
            for (k, r) in wrap(
                vec![Span::styled(t.content.clone(), st)],
                w.saturating_sub(2),
                0,
            )
            .into_iter()
            .enumerate()
            {
                let mut v = vec![if k == 0 {
                    Span::styled(format!("{m} "), Style::new().fg(c))
                } else {
                    Span::raw("  ")
                }];
                v.extend(r);
                l.push(Line::from(v));
            }
        }
    }
    f.render_widget(Paragraph::new(l), inner);
    let tag = Line::from(vec![
        Span::styled("port", muted()),
        Span::styled("code ", bold(ACCENT)),
        Span::styled(env!("CARGO_PKG_VERSION"), muted()),
    ]);
    f.render_widget(
        Paragraph::new(tag),
        Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        },
    );
}

fn popup(f: &mut Frame, app: &App, above: Rect) {
    let m = app.slash_matches();
    if m.is_empty() {
        return;
    }
    let h = m.len().min(8) as u16 + 2;
    let area = Rect {
        x: above.x,
        y: above.y.saturating_sub(h),
        width: above.width,
        height: h,
    };
    f.render_widget(Clear, area);
    let lines: Vec<Line> = m
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, (n, d))| {
            Line::from(vec![
                Span::styled(
                    format!(" /{n:<10}"),
                    if i == 0 { bold(ACCENT) } else { text() },
                ),
                Span::styled((*d).to_string(), muted()),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(BORDER))
                .style(Style::new().bg(PANEL2)),
        ),
        area,
    );
}

fn permission(f: &mut Frame, app: &App, full: Rect) {
    let Some(p) = &app.perm else { return };
    let w = full.width.clamp(30, 76);
    let target: Vec<String> = hard_wrap(&p.req.target, (w as usize).saturating_sub(6));
    let h = (target.len().min(6) as u16) + 6 + u16::from(!p.req.reason.is_empty());
    let area = Rect {
        x: full.x + (full.width.saturating_sub(w)) / 2,
        y: full.y + full.height.saturating_sub(h) / 2,
        width: w,
        height: h.min(full.height),
    };
    f.render_widget(Clear, area);
    let mut l = vec![
        Line::from(vec![
            Span::styled(" Permission required ", bold(YELLOW)),
            Span::styled(format!(" {}", p.req.tool), bold(TEXT)),
        ]),
        Line::from(""),
    ];
    for t in target.iter().take(6) {
        l.push(Line::from(Span::styled(
            format!(" {t}"),
            Style::new().fg(TEXT).bg(PANEL2),
        )));
    }
    if !p.req.reason.is_empty() {
        l.push(Line::from(Span::styled(
            format!(" {}", p.req.reason),
            muted(),
        )));
    }
    l.push(Line::from(""));
    l.push(Line::from(vec![
        Span::styled(" y ", Style::new().fg(BG).bg(GREEN)),
        Span::styled(" allow once   ", text()),
        Span::styled(" a ", Style::new().fg(BG).bg(ACCENT)),
        Span::styled(" always   ", text()),
        Span::styled(" n ", Style::new().fg(BG).bg(RED)),
        Span::styled(" reject", text()),
    ]));
    f.render_widget(
        Paragraph::new(l).block(
            Block::new()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(YELLOW))
                .style(Style::new().bg(PANEL)),
        ),
        area,
    );
}

const LOGO: [(&str, &str); 3] = [
    ("┌─┐┌─┐┬─┐┌┬┐", "┌─┐┌─┐┌┬┐┌─┐"),
    ("├─┘│ │├┬┘ │ ", "│  │ │ ││├┤ "),
    ("┴  └─┘┴└─ ┴ ", "└─┘└─┘─┴┘└─┘"),
];

fn home(f: &mut Frame, app: &App, area: Rect) {
    let box_w = area.width.saturating_sub(4).min(74);
    let ph = prompt_height(app, box_w);
    let total = 3 + 2 + 1 + ph + 2;
    let top = area.y + area.height.saturating_sub(total) / 2;
    let cx = |w: u16| area.x + area.width.saturating_sub(w) / 2;
    let logo: Vec<Line> = LOGO
        .iter()
        .map(|(a, b)| {
            Line::from(vec![
                Span::styled(*a, muted()),
                Span::styled(*b, bold(TEXT)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(logo),
        Rect {
            x: cx(26),
            y: top,
            width: 26,
            height: 3,
        },
    );
    let tag = "port anything to Rust, and show the proof";
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(tag, muted()))),
        Rect {
            x: cx(tag.chars().count() as u16),
            y: top + 4,
            width: tag.chars().count() as u16,
            height: 1,
        },
    );
    let pa = Rect {
        x: cx(box_w),
        y: top + 6,
        width: box_w,
        height: ph,
    };
    prompt(f, app, pa, true);
    popup(f, app, pa);
    let tips = "tab agents   /deep look inside   /  commands   ctrl+c quit";
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(tips, muted()))),
        Rect {
            x: cx(tips.chars().count() as u16),
            y: pa.y + pa.height + 1,
            width: tips.chars().count() as u16,
            height: 1,
        },
    );
    footer(
        f,
        app,
        Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        },
    );
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(TEXT)), area);
    if app.items.is_empty() {
        home(f, app, area);
        permission(f, app, area);
        return;
    }
    let side = app.sidebar && area.width >= 110;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if side {
            vec![Constraint::Min(40), Constraint::Length(38)]
        } else {
            vec![Constraint::Min(40)]
        })
        .split(area);
    let main = cols[0];
    let ph = prompt_height(app, main.width.saturating_sub(2));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(ph),
            Constraint::Length(1),
        ])
        .split(main);
    let (tr, pr, fr) = (rows[0], rows[1], rows[2]);
    let lines = transcript(app, tr.width.saturating_sub(2) as usize);
    let h = tr.height as usize;
    let max_scroll = lines.len().saturating_sub(h);
    app.scroll = app.scroll.min(max_scroll);
    let start = lines.len().saturating_sub(h + app.scroll);
    let vis: Vec<Line> = lines.into_iter().skip(start).take(h).collect();
    f.render_widget(
        Paragraph::new(vis),
        Rect {
            x: tr.x + 1,
            y: tr.y,
            width: tr.width.saturating_sub(2),
            height: tr.height,
        },
    );
    let pa = Rect {
        x: pr.x + 1,
        y: pr.y,
        width: pr.width.saturating_sub(2),
        height: pr.height,
    };
    prompt(f, app, pa, true);
    popup(f, app, pa);
    footer(f, app, fr);
    if side {
        sidebar(f, app, cols[1]);
    }
    permission(f, app, area);
    let _ = SLASH;
}
