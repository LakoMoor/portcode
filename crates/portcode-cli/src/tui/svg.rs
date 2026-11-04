//! Render a ratatui `Buffer` as an SVG "screenshot" (colours, bold, dim, backgrounds). Used by `portcode snapshot`.
use super::theme::{c_bg, c_text, is_light};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

fn rgb(c: Color, default: Color) -> String {
    let c = if c == Color::Reset { default } else { c };
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0x48, 0x4f, 0x58),
        Color::Red => (0xff, 0x7b, 0x72),
        Color::Green => (0x7e, 0xe7, 0x87),
        Color::Yellow => (0xe3, 0xb3, 0x41),
        Color::Blue => (0x79, 0xc0, 0xff),
        Color::Magenta => (0xd2, 0xa8, 0xff),
        Color::Cyan => (0x56, 0xd4, 0xdd),
        Color::Gray => (0xc9, 0xd1, 0xd9),
        Color::DarkGray => (0x6e, 0x76, 0x81),
        Color::White => (0xf0, 0xf6, 0xfc),
        Color::Indexed(n) => {
            let v = 8u8.saturating_add(n.saturating_sub(232).saturating_mul(10));
            (v, v, v)
        }
        _ => (0xee, 0xee, 0xee),
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(' ', "\u{a0}")
}

pub fn buffer_to_svg(buf: &Buffer, title: &str) -> String {
    let (cw, lh, top, pad) = (9.33_f64, 19.0_f64, 34.0_f64, 0.0_f64);
    let (cols, rows) = (buf.area.width as usize, buf.area.height as usize);
    let (w, h) = (cols as f64 * cw + pad * 2.0, top + rows as f64 * lh);
    let bg = rgb(c_bg(), c_bg());
    let (bar, cap) = if is_light() {
        ("#e6e5e1", "#6b6b68")
    } else {
        ("#1b1b1b", "#8b8b8b")
    };
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" viewBox=\"0 0 {w:.0} {h:.0}\" role=\"img\" aria-label=\"{}\">\n<rect width=\"{w:.0}\" height=\"{h:.0}\" rx=\"10\" fill=\"{bg}\"/>\n<rect width=\"{w:.0}\" height=\"{top}\" rx=\"10\" fill=\"{bar}\"/><rect y=\"24\" width=\"{w:.0}\" height=\"10\" fill=\"{bar}\"/>\n<circle cx=\"18\" cy=\"17\" r=\"5\" fill=\"#ff5f56\"/><circle cx=\"36\" cy=\"17\" r=\"5\" fill=\"#ffbd2e\"/><circle cx=\"54\" cy=\"17\" r=\"5\" fill=\"#27c93f\"/>\n<text x=\"{:.0}\" y=\"21\" fill=\"{cap}\" font-family=\"ui-monospace,Menlo,monospace\" font-size=\"12\" text-anchor=\"middle\">{}</text>\n", esc(title), w / 2.0, esc(title));
    let font = "font-family=\"ui-monospace,SFMono-Regular,Menlo,Consolas,'DejaVu Sans Mono',monospace\" font-size=\"15.5\"";
    // backgrounds first (merged runs), then text
    for y in 0..rows {
        let mut x = 0;
        while x < cols {
            let c = &buf[(x as u16, y as u16)];
            let color = rgb(c.bg, c_bg());
            let mut e = x + 1;
            while e < cols && rgb(buf[(e as u16, y as u16)].bg, c_bg()) == color {
                e += 1;
            }
            if color != bg {
                s += &format!("<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{lh}\" fill=\"{color}\"/>\n", pad + x as f64 * cw, top + y as f64 * lh, (e - x) as f64 * cw);
            }
            x = e;
        }
    }
    s += &format!("<g {font} xml:space=\"preserve\">\n");
    for y in 0..rows {
        let mut x = 0;
        while x < cols {
            let c = &buf[(x as u16, y as u16)];
            let key = (rgb(c.fg, c_text()), c.modifier);
            let mut e = x;
            let mut text = String::new();
            while e < cols {
                let n = &buf[(e as u16, y as u16)];
                if (rgb(n.fg, c_text()), n.modifier) != key {
                    break;
                }
                text.push_str(n.symbol());
                e += 1;
            }
            if !text.trim().is_empty() {
                let dim = if key.1.contains(Modifier::DIM) {
                    " opacity=\"0.6\""
                } else {
                    ""
                };
                let bold = if key.1.contains(Modifier::BOLD) {
                    " font-weight=\"700\""
                } else {
                    ""
                };
                let it = if key.1.contains(Modifier::ITALIC) {
                    " font-style=\"italic\""
                } else {
                    ""
                };
                s += &format!("<text x=\"{:.2}\" y=\"{:.2}\" fill=\"{}\" textLength=\"{:.2}\" lengthAdjust=\"spacingAndGlyphs\"{dim}{bold}{it}>{}</text>\n", pad + x as f64 * cw, top + (y as f64 + 1.0) * lh - 5.0, key.0, (e - x) as f64 * cw, esc(&text));
            }
            x = e.max(x + 1);
        }
    }
    s + "</g>\n</svg>\n"
}
