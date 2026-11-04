//! OpenCode-like dark theme (warm peach accent on near-black).
use ratatui::style::{Color, Modifier, Style};

pub const BG: Color = Color::Rgb(10, 10, 10);
pub const PANEL: Color = Color::Rgb(20, 20, 20);
pub const PANEL2: Color = Color::Rgb(30, 30, 30);
pub const BORDER: Color = Color::Rgb(72, 72, 72);
pub const TEXT: Color = Color::Rgb(238, 238, 238);
pub const MUTED: Color = Color::Rgb(128, 128, 128);
pub const ACCENT: Color = Color::Rgb(250, 178, 131);
pub const BLUE: Color = Color::Rgb(92, 156, 245);
pub const GREEN: Color = Color::Rgb(127, 216, 143);
pub const RED: Color = Color::Rgb(224, 108, 117);
pub const YELLOW: Color = Color::Rgb(245, 167, 66);
pub const PURPLE: Color = Color::Rgb(157, 124, 216);
pub const CYAN: Color = Color::Rgb(86, 182, 194);

pub fn text() -> Style {
    Style::new().fg(TEXT)
}
pub fn muted() -> Style {
    Style::new().fg(MUTED)
}
pub fn bold(c: Color) -> Style {
    Style::new().fg(c).add_modifier(Modifier::BOLD)
}
pub fn agent_color(name: &str) -> Color {
    match name {
        "build" => ACCENT,
        "port" => PURPLE,
        "reverse" => CYAN,
        "plan" => BLUE,
        _ => MUTED,
    }
}
