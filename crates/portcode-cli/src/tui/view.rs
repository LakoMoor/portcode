//! Rendering: home screen, session view (transcript + prompt + footer + sidebar), popups, permission dialog.
use super::app::{summarize, App, Item, ToolState, SLASH};
use super::theme::*;
use super::workbench::Tab;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
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
    let shown = if app.key_for.is_some() {
        "•".repeat(app.input.chars().count())
    } else {
        app.input.clone()
    };
    let rows = hard_wrap(&shown, inner_w);
    let mut lines: Vec<Line> = vec![Line::from("")];
    if app.input.is_empty() {
        let key_hint = app
            .key_for
            .as_ref()
            .map(|p| format!("paste the API key for {p} · enter saves · esc cancels"));
        let hint = if let Some(h) = &key_hint {
            h.as_str()
        } else if app.busy {
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
    let m = app.menu();
    if m.is_empty() {
        return;
    }
    let sel = app.slash_sel.min(m.len() - 1);
    let rows = 8usize;
    // keep the highlighted row inside the visible window
    let start = sel.saturating_sub(rows - 1);
    let h = m.len().min(rows) as u16 + 2;
    let area = Rect {
        x: above.x,
        y: above.y.saturating_sub(h),
        width: above.width,
        height: h,
    };
    f.render_widget(Clear, area);
    let w = m
        .iter()
        .map(|i| i.label.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 28);
    let lines: Vec<Line> = m
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .map(|(i, it)| {
            let on = i == sel;
            let row = Style::new().bg(if on { SELECT } else { PANEL2 });
            Line::from(vec![
                Span::styled(if on { " ▸ " } else { "   " }, row.fg(ACCENT)),
                Span::styled(
                    format!("{:<w$}  ", it.label),
                    if on {
                        bold(ACCENT).bg(SELECT)
                    } else {
                        text().bg(PANEL2)
                    },
                ),
                Span::styled(
                    it.desc.clone(),
                    muted().bg(if on { SELECT } else { PANEL2 }),
                ),
            ])
        })
        .collect();
    let more = if m.len() > rows {
        format!(" {}/{} ", sel + 1, m.len())
    } else {
        String::new()
    };
    f.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(BORDER))
                .title_bottom(Span::styled(more, muted()))
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
        }
        .intersection(area),
    );
    let tag = "port anything to Rust, and show the proof";
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(tag, muted()))),
        Rect {
            x: cx(tag.chars().count() as u16),
            y: top + 4,
            width: tag.chars().count() as u16,
            height: 1,
        }
        .intersection(area),
    );
    let pa = Rect {
        x: cx(box_w),
        y: top + 6,
        width: box_w,
        height: ph,
    }
    .intersection(area);
    prompt(f, app, pa, true);
    popup(f, app, pa);
    let tips = "tab agents   /deep look inside   /  commands   ctrl+c quit";
    if let Some(h) = &app.hint {
        let w = (h.chars().count() as u16).min(area.width.saturating_sub(2));
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(h.clone(), Style::new().fg(YELLOW)))),
            Rect {
                x: cx(w),
                y: pa.y + pa.height + 3,
                width: w,
                height: 1,
            }
            .intersection(area),
        );
    }
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(tips, muted()))),
        Rect {
            x: cx(tips.chars().count() as u16),
            y: pa.y + pa.height + 1,
            width: tips.chars().count() as u16,
            height: 1,
        }
        .intersection(area),
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
    if app.wiz.is_some() {
        wizard(f, app, area);
        return;
    }
    if app.wb.is_some() {
        workbench(f, app, area);
        permission(f, app, area);
        return;
    }
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

const C_KW: [&str; 14] = [
    "void", "int", "long", "char", "if", "else", "do", "while", "for", "return", "uint", "short",
    "double", "struct",
];

/// Colour one line of code for the tab it came from. Cheap and tolerant: a wrong colour is cosmetic, never an error.
fn code_line(tab: Tab, raw: &str, num: Option<usize>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(n) = num {
        spans.push(Span::styled(format!("{n:>4}  "), Style::new().fg(BORDER)));
    }
    match tab {
        Tab::Disasm => {
            // "100000460: d10683ff   sub sp, sp, #0x1a0"
            let (a, rest) = raw.split_once(": ").unwrap_or(("", raw));
            let mut it = rest.split_whitespace();
            let bytes = it.next().unwrap_or("");
            let mnem = it.next().unwrap_or("");
            let ops = it.collect::<Vec<_>>().join(" ");
            spans.push(Span::styled(format!("{a:<10} "), muted()));
            spans.push(Span::styled(
                format!("{bytes:<18}"),
                Style::new().fg(BORDER),
            ));
            spans.push(Span::styled(format!("{mnem:<7} "), Style::new().fg(BLUE)));
            spans.push(Span::styled(ops, Style::new().fg(TEXT)));
        }
        Tab::Hex => {
            let (off, rest) = raw.split_once("  ").unwrap_or(("", raw));
            let (hex, asc) = rest.split_once("  |").unwrap_or((rest, ""));
            spans.push(Span::styled(format!("{off}  "), muted()));
            spans.push(Span::styled(format!("{hex}  "), Style::new().fg(TEAL)));
            if !asc.is_empty() {
                spans.push(Span::styled(format!("|{asc}"), Style::new().fg(MUTED)));
            }
        }
        Tab::Pseudo => {
            let t = raw.trim_start();
            if t.starts_with("//") || t.starts_with("/*") {
                spans.push(Span::styled(
                    raw.to_string(),
                    Style::new().fg(MUTED).add_modifier(Modifier::ITALIC),
                ));
            } else {
                // split into words and punctuation, colour keywords, numbers and strings
                let (mut cur, mut in_str) = (String::new(), false);
                let flush = |cur: &mut String, spans: &mut Vec<Span<'static>>, in_str: bool| {
                    if cur.is_empty() {
                        return;
                    }
                    let c = std::mem::take(cur);
                    let st = if in_str {
                        Style::new().fg(GREEN)
                    } else if C_KW.contains(&c.as_str()) {
                        Style::new().fg(PURPLE)
                    } else if c.starts_with(|x: char| x.is_ascii_digit()) {
                        Style::new().fg(YELLOW)
                    } else {
                        Style::new().fg(TEXT)
                    };
                    spans.push(Span::styled(c, st));
                };
                for ch in raw.chars() {
                    if ch == '"' {
                        if in_str {
                            cur.push(ch);
                            flush(&mut cur, &mut spans, true);
                            in_str = false;
                        } else {
                            flush(&mut cur, &mut spans, false);
                            cur.push(ch);
                            in_str = true;
                        }
                    } else if in_str || ch.is_alphanumeric() || ch == '_' {
                        cur.push(ch);
                    } else {
                        flush(&mut cur, &mut spans, false);
                        spans.push(Span::styled(ch.to_string(), Style::new().fg(MUTED)));
                    }
                }
                flush(&mut cur, &mut spans, in_str);
            }
        }
    }
    Line::from(spans)
}

/// Word wrap for plain text (hard breaks only inside words longer than the width).
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let (mut out, mut cur) = (Vec::new(), String::new());
    for w in text.split_whitespace() {
        let need = cur.chars().count() + w.chars().count() + usize::from(!cur.is_empty());
        if need > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if w.chars().count() > width {
            let mut parts = hard_wrap(w, width);
            if let Some(last) = parts.pop() {
                out.extend(parts);
                cur = last;
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn clip_to(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(w.saturating_sub(1)).collect::<String>()
        )
    }
}

/// The deep-mode workbench: functions | code | evidence, with the agent underneath.
fn workbench(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(wb) = app.wb.as_ref() else {
        return;
    };
    let ph = prompt_height(app, area.width.saturating_sub(2));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(3),
            Constraint::Length(ph),
            Constraint::Length(1),
        ])
        .split(area);
    // header
    let mut head = vec![
        Span::styled(" workbench ", bold(ACCENT)),
        Span::styled(format!(" {} ", wb.file), text()),
        Span::styled(
            format!("{} {}  ", wb.format.to_lowercase(), wb.arch),
            muted(),
        ),
    ];
    head.extend(mode_chip(app));
    f.render_widget(Paragraph::new(Line::from(head)), rows[0]);

    let right_w: u16 = if area.width >= 120 { 38 } else { 0 };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30),
            Constraint::Min(30),
            Constraint::Length(right_w),
        ])
        .split(rows[1]);

    // functions
    let vis = wb.visible();
    let lw = cols[0].width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(" Functions ", bold(TEXT)),
            Span::styled(
                format!("{}", wb.funcs.len()),
                Style::new().fg(MUTED).bg(PANEL2),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                " {}{}",
                if wb.filter.is_empty() && wb.focus != super::workbench::Focus::Filter {
                    "/ filter…".to_string()
                } else {
                    format!(
                        "/ {}{}",
                        wb.filter,
                        if wb.focus == super::workbench::Focus::Filter {
                            "▏"
                        } else {
                            ""
                        }
                    )
                },
                ""
            ),
            if wb.focus == super::workbench::Focus::Filter {
                text()
            } else {
                muted()
            },
        )),
        Line::from(""),
    ];
    if let Some(st) = &wb.status {
        lines.push(Line::from(Span::styled(
            format!(" {st}"),
            Style::new().fg(YELLOW),
        )));
    }
    let per = (cols[0].height as usize).saturating_sub(lines.len()) / 2;
    let first = wb.sel.saturating_sub(per.saturating_sub(1));
    for (n, &i) in vis.iter().enumerate().skip(first).take(per.max(1)) {
        let Some(fu) = wb.funcs.get(i) else { continue };
        let on = n == wb.sel;
        let bg = if on { PANEL2 } else { BG };
        let size = if fu.size > 0 {
            format!("size {}", fu.size)
        } else {
            String::new()
        };
        let a = format!("0x{:x}", fu.addr);
        let gap = lw.saturating_sub(a.len().min(lw) + size.len() + 1);
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}", clip_to(&a, lw)),
                Style::new().fg(if on { BLUE } else { MUTED }).bg(bg),
            ),
            Span::styled(" ".repeat(gap), Style::new().bg(bg)),
            Span::styled(format!("{size} "), Style::new().fg(MUTED).bg(bg)),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                " {}{}",
                clip_to(&fu.name, lw.saturating_sub(1)),
                " ".repeat(lw.saturating_sub(fu.name.chars().count() + 1))
            ),
            Style::new().fg(if on { ACCENT } else { TEXT }).bg(bg),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .borders(Borders::RIGHT)
                .border_style(Style::new().fg(RULE)),
        ),
        cols[0],
    );

    // code
    let cw = cols[1].width.saturating_sub(2) as usize;
    let mut tabs: Vec<Span> = vec![Span::raw(" ")];
    for (i, t) in Tab::ALL.iter().enumerate() {
        let st = if *t == wb.tab {
            bold(ACCENT).add_modifier(Modifier::UNDERLINED)
        } else {
            muted()
        };
        tabs.push(Span::styled(format!("{} {}", i + 1, t.label()), st));
        tabs.push(Span::raw("   "));
    }
    let mut code: Vec<Line> = vec![Line::from(tabs)];
    if let Some(fu) = wb.current() {
        code.push(Line::from(vec![
            Span::styled(format!(" {}", fu.name), bold(TEXT)),
            Span::styled(
                format!(
                    "  0x{:x}{}{}",
                    fu.addr,
                    if fu.size > 0 {
                        format!(" · {} bytes", fu.size)
                    } else {
                        String::new()
                    },
                    fu.offset
                        .map(|o| format!(" · file offset {o}"))
                        .unwrap_or_default()
                ),
                muted(),
            ),
        ]));
    }
    code.push(Line::from(""));
    let avail = (cols[1].height as usize).saturating_sub(code.len());
    match wb.body() {
        Some(super::workbench::Body::Text(t)) => {
            let all: Vec<&str> = t.lines().collect();
            let max = all.len().saturating_sub(avail);
            let sc = wb.scroll.min(max);
            let numbered = wb.tab == Tab::Pseudo;
            for (i, l) in all.iter().enumerate().skip(sc).take(avail) {
                let mut ln = code_line(wb.tab, l, if numbered { Some(i + 1) } else { None });
                if !numbered {
                    ln.spans.insert(0, Span::raw(" "));
                }
                code.push(ln);
            }
        }
        Some(super::workbench::Body::Err(e)) => {
            for l in hard_wrap(e, cw) {
                code.push(Line::from(Span::styled(
                    format!(" {l}"),
                    Style::new().fg(RED),
                )));
            }
        }
        Some(super::workbench::Body::Loading) => {
            let sp = SPIN[app.spinner / 2 % SPIN.len()];
            code.push(Line::from(Span::styled(
                if wb.tab == Tab::Pseudo {
                    format!(" {sp} Ghidra is decompiling. The first function of a binary also analyses it, which can take a while.")
                } else {
                    format!(" {sp} reading…")
                },
                muted(),
            )));
        }
        None => code.push(Line::from(Span::styled(" select a function", muted()))),
    }
    f.render_widget(Paragraph::new(code), cols[1]);

    // evidence
    if right_w > 0 {
        let ew = cols[2].width.saturating_sub(5) as usize;
        let mut ev: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(" Evidence ", bold(TEXT)),
                Span::styled(
                    format!("{}", wb.notes.len()),
                    Style::new().fg(MUTED).bg(PANEL2),
                ),
            ]),
            Line::from(""),
        ];
        if wb.notes.is_empty() {
            for l in hard_wrap(
                "nothing recorded yet. Press a to ask the agent about this function.",
                ew,
            ) {
                ev.push(Line::from(Span::styled(format!(" {l}"), muted())));
            }
        }
        let me = wb.current().map(|f| f.name.clone()).unwrap_or_default();
        for n in &wb.notes {
            let col = match n.kind {
                "observation" => GREEN,
                "inference" => YELLOW,
                _ => RED,
            };
            let hot = !me.is_empty() && n.text.contains(&me);
            ev.push(Line::from(vec![
                Span::styled(" ● ", Style::new().fg(col)),
                Span::styled(n.id.clone(), bold(if hot { ACCENT } else { TEXT })),
                Span::styled(format!("  {}", n.kind), Style::new().fg(col)),
            ]));
            for l in word_wrap(&n.text, ew).into_iter().take(4) {
                ev.push(Line::from(Span::styled(
                    format!("   {l}"),
                    Style::new().fg(MUTED),
                )));
            }
            ev.push(Line::from(""));
        }
        f.render_widget(
            Paragraph::new(ev).block(
                Block::new()
                    .borders(Borders::LEFT)
                    .border_style(Style::new().fg(RULE)),
            ),
            cols[2],
        );
    }

    // the agent's last answer and the prompt
    let mut ans: Vec<Line> = vec![Line::from("")];
    if app.busy {
        ans.push(Line::from(Span::styled(
            format!(
                " {} {} is working…",
                SPIN[app.spinner / 2 % SPIN.len()],
                cap(&app.agent)
            ),
            Style::new().fg(agent_color(&app.agent)),
        )));
    } else if let Some(Item::Assistant(t)) = app
        .items
        .iter()
        .rev()
        .find(|i| matches!(i, Item::Assistant(_)))
    {
        for l in hard_wrap(t.trim(), area.width.saturating_sub(4) as usize)
            .into_iter()
            .take(2)
        {
            ans.push(Line::from(Span::styled(format!(" {l}"), text())));
        }
    }
    f.render_widget(Paragraph::new(ans), rows[2]);
    let pa = Rect {
        x: rows[3].x + 1,
        y: rows[3].y,
        width: rows[3].width.saturating_sub(2),
        height: rows[3].height,
    };
    let chat = wb.focus == super::workbench::Focus::Chat;
    prompt(f, app, pa, chat);
    let hints = if chat {
        " enter send · esc back to the code"
    } else {
        " j/k function · 1 2 3 view · / filter · a ask the agent · i chat · r reload · q close"
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hints, muted()))),
        rows[4],
    );
}

/// `/create`: the guided start, drawn as one scrollable column of lines.
fn wizard(f: &mut Frame, app: &mut App, area: Rect) {
    use super::wizard::{Field, Kind};
    let Some(w) = app.wiz.as_ref() else {
        return;
    };
    let width = (area.width as usize).saturating_sub(4).clamp(40, 104);
    let left = area.x + (area.width.saturating_sub(width as u16)) / 2;
    let on = |c: bool| {
        if c {
            Style::new()
                .fg(TEXT)
                .bg(Color::Rgb(22, 38, 78))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(MUTED).bg(PANEL2)
        }
    };
    let focus_mark = |fld: Field| {
        if w.focus == fld {
            Span::styled("▎", Style::new().fg(ACCENT))
        } else {
            Span::raw(" ")
        }
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut focus_line = 0usize;
    let mark = |lines: &Vec<Line>, fld: Field, fl: &mut usize| {
        if w.focus == fld {
            *fl = lines.len();
        }
    };
    lines.push(Line::from(vec![
        Span::styled("New project", bold(TEXT)),
        Span::styled(
            "   enter or ctrl+s starts · tab moves · esc closes",
            muted(),
        ),
    ]));
    lines.push(Line::from(""));
    // goal
    mark(&lines, Field::Goal, &mut focus_line);
    lines.push(Line::from(vec![
        focus_mark(Field::Goal),
        Span::styled(" Make my own version ", on(!w.look_only)),
        Span::raw(" "),
        Span::styled(" Just look inside ", on(w.look_only)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if w.look_only {
            "What do you want to look into?"
        } else {
            "What do you want to make?"
        },
        bold(TEXT),
    )));
    lines.push(Line::from(Span::styled(
        if w.look_only {
            "Pick it, then press Start. You get docs/SPEC.md and the evidence ledger, no port code."
        } else {
            "Choose what to recreate, say what you want, then press Start."
        },
        muted(),
    )));
    lines.push(Line::from(""));
    // 1
    lines.push(Line::from(vec![
        Span::styled(" 1 ", Style::new().fg(TEXT).bg(PANEL2)),
        Span::styled(
            if w.look_only {
                "  What do you want to look into?"
            } else {
                "  What do you want to recreate?"
            },
            bold(TEXT),
        ),
    ]));
    lines.push(Line::from(""));
    mark(&lines, Field::Kind, &mut focus_line);
    let mut row: Vec<Span> = vec![focus_mark(Field::Kind)];
    for (i, k) in Kind::ALL.iter().enumerate() {
        let sel = *k == w.kind;
        row.push(Span::styled(
            format!(
                " {} {}{} ",
                k.icon(),
                k.label(),
                if sel { " ✓" } else { "" }
            ),
            on(sel),
        ));
        row.push(Span::raw(" "));
        let _ = i;
    }
    lines.push(Line::from(row));
    if let Some(lbl) = w.kind.source_label() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!(" {lbl}"), muted())));
        mark(&lines, Field::Source, &mut focus_line);
        let shown = if w.source.is_empty() {
            Span::styled(format!(" {}", w.kind.placeholder()), muted())
        } else {
            Span::styled(format!(" {}", w.source), text())
        };
        let mut sp = vec![focus_mark(Field::Source), shown];
        if w.focus == Field::Source {
            sp.push(Span::styled("▏", Style::new().fg(ACCENT)));
        }
        let used: usize = sp.iter().map(|s| s.content.chars().count()).sum();
        sp.push(Span::styled(
            " ".repeat(width.saturating_sub(used).min(60)),
            Style::new().bg(PANEL2),
        ));
        lines.push(Line::from(sp).style(Style::new().bg(PANEL2)));
        if let Some(n) = &w.note {
            lines.push(Line::from(Span::styled(
                format!("  {n}"),
                Style::new().fg(if n.contains("outside") || n.contains("not found") {
                    YELLOW
                } else {
                    TEAL
                }),
            )));
        }
    }
    lines.push(Line::from(""));
    // closeness
    lines.push(Line::from(Span::styled(
        if w.look_only {
            " How closely may we look at it? Looking inside means deep."
        } else {
            " How closely may we look at it?"
        },
        muted(),
    )));
    mark(&lines, Field::Look, &mut focus_line);
    let cw = (width.saturating_sub(4)) / 2;
    let card = |sel: bool, title: &str, body: &[&str]| -> Vec<String> {
        let mut v = vec![format!(
            " {} {}{}",
            if title.starts_with("Clean") {
                "▣"
            } else {
                "◉"
            },
            title,
            if sel { " ✓" } else { "" }
        )];
        for b in body {
            v.push(format!("   {b}"));
        }
        v.into_iter()
            .map(|s| format!("{:<cw$}", clip_to(&s, cw)))
            .collect()
    };
    let clean = card(
        !w.deep,
        "Clean room",
        &[
            "Nothing inside it is read. We work from",
            "what you can see: running it, its output.",
        ],
    );
    let deep = card(
        w.deep,
        "Deep",
        &[
            "We read its code and files with every tool:",
            "Ghidra, Capstone, dotnet. For private use.",
        ],
    );
    for i in 0..3 {
        let a = clean.get(i).cloned().unwrap_or_default();
        let b = deep.get(i).cloned().unwrap_or_default();
        lines.push(Line::from(vec![
            focus_mark(Field::Look),
            Span::styled(a, on(!w.deep).remove_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(b, on(w.deep).remove_modifier(Modifier::BOLD)),
        ]));
    }
    lines.push(Line::from(""));
    // stack
    mark(&lines, Field::Stack, &mut focus_line);
    let mut st: Vec<Span> = vec![
        focus_mark(Field::Stack),
        Span::styled(" Build with  ", muted()),
    ];
    for (i, (l, _)) in w.kind.stacks().iter().enumerate() {
        st.push(Span::styled(format!(" {l} "), on(i == w.stack)));
        st.push(Span::raw(" "));
    }
    lines.push(Line::from(st));
    mark(&lines, Field::Options, &mut focus_line);
    if !w.look_only {
        let box_ = |b: bool| if b { "[x]" } else { "[ ]" };
        lines.push(Line::from(vec![
            focus_mark(Field::Options),
            Span::styled(
                format!(" 1 {} Its own design and art ", box_(w.own_art)),
                Style::new().fg(if w.own_art { TEXT } else { MUTED }),
            ),
            Span::styled(
                format!("  2 {} Screenshots and a site ", box_(w.proof)),
                Style::new().fg(if w.proof { TEXT } else { MUTED }),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            if w.own_art {
                "   Same features, new look. Nothing from the original's assets, text or code."
            } else {
                "   1:1: goldens from the original, parity PASS before it counts as done."
            },
            muted(),
        )));
    }
    lines.push(Line::from(""));
    // 2
    lines.push(Line::from(vec![
        Span::styled(" 2 ", Style::new().fg(TEXT).bg(PANEL2)),
        Span::styled("  Say what you want", bold(TEXT)),
    ]));
    lines.push(Line::from(""));
    mark(&lines, Field::Text, &mut focus_line);
    let body = if w.text.is_empty() {
        w.placeholder_text().to_string()
    } else {
        w.text.clone()
    };
    let body_style = if w.text.is_empty() { muted() } else { text() };
    let shown: Vec<String> = hard_wrap(&body, width.saturating_sub(4));
    for (i, l) in shown.iter().take(5).enumerate() {
        let mut sp = vec![
            focus_mark(Field::Text),
            Span::styled(format!(" {l}"), body_style),
        ];
        if i + 1 == shown.len().min(5) && w.focus == Field::Text {
            sp.push(Span::styled("▏", Style::new().fg(ACCENT)));
        }
        lines.push(Line::from(sp).style(Style::new().bg(PANEL2)));
    }
    lines.push(Line::from(Span::styled(
        "  Enter to start · Alt+Enter for a new line",
        muted(),
    )));
    lines.push(Line::from(""));
    mark(&lines, Field::Start, &mut focus_line);
    let who = if w.kind == Kind::Fresh {
        "build"
    } else if w.look_only {
        "reverse"
    } else {
        "port"
    };
    lines.push(Line::from(vec![
        focus_mark(Field::Start),
        Span::styled(" Built by ", muted()),
        Span::styled(format!("{}/{}", app.provider, app.model), text()),
        Span::styled(
            format!("  agent {who}  "),
            Style::new().fg(agent_color(who)),
        ),
        Span::styled(
            if w.look_only || w.deep {
                " ◉ deep "
            } else {
                " ▣ clean-room "
            },
            if w.look_only || w.deep {
                Style::new().fg(BG).bg(YELLOW).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(TEAL)
            },
        ),
        Span::raw("   "),
        Span::styled(
            "  ✦ Start  ",
            if w.focus == Field::Start {
                Style::new().fg(BG).bg(TEXT).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(TEXT).bg(PANEL2)
            },
        ),
    ]));
    if let Some(e) = &w.error {
        lines.push(Line::from(Span::styled(
            format!(" {e}"),
            Style::new().fg(RED),
        )));
    }
    let h = area.height as usize;
    let off = focus_line
        .saturating_sub(h.saturating_sub(6))
        .min(lines.len().saturating_sub(h));
    let vis: Vec<Line> = lines.into_iter().skip(off).take(h).collect();
    f.render_widget(
        Paragraph::new(vis),
        Rect {
            x: left,
            y: area.y,
            width: width as u16,
            height: area.height,
        },
    );
}
