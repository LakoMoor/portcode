//! Two palettes: dark (warm peach accent on near-black, default) and light. `/theme` switches.
use ratatui::style::{Color, Modifier, Style};
use std::sync::atomic::{AtomicBool, Ordering};

static LIGHT: AtomicBool = AtomicBool::new(false);

pub fn set_light(on: bool) {
    LIGHT.store(on, Ordering::Relaxed);
}
pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}
fn pick(dark: (u8, u8, u8), light: (u8, u8, u8)) -> Color {
    let (r, g, b) = if is_light() { light } else { dark };
    Color::Rgb(r, g, b)
}
pub fn c_bg() -> Color {
    pick((10, 10, 10), (250, 250, 248))
}
pub fn c_panel() -> Color {
    pick((20, 20, 20), (240, 239, 236))
}
pub fn c_panel2() -> Color {
    pick((30, 30, 30), (232, 231, 227))
}
pub fn c_pass_bg() -> Color {
    pick((20, 46, 30), (214, 240, 222))
}
pub fn c_fail_bg() -> Color {
    pick((56, 24, 28), (250, 218, 220))
}
pub fn c_select() -> Color {
    pick((52, 40, 34), (252, 228, 210))
}
pub fn c_border() -> Color {
    pick((72, 72, 72), (190, 190, 186))
}
pub fn c_text() -> Color {
    pick((238, 238, 238), (30, 30, 30))
}
pub fn c_muted() -> Color {
    pick((128, 128, 128), (110, 110, 106))
}
pub fn c_accent() -> Color {
    pick((250, 178, 131), (196, 98, 28))
}
pub fn c_blue() -> Color {
    pick((92, 156, 245), (30, 90, 200))
}
pub fn c_green() -> Color {
    pick((127, 216, 143), (28, 130, 60))
}
pub fn c_red() -> Color {
    pick((224, 108, 117), (200, 50, 60))
}
pub fn c_yellow() -> Color {
    pick((245, 167, 66), (176, 108, 0))
}
pub fn c_purple() -> Color {
    pick((157, 124, 216), (110, 70, 190))
}
pub fn c_teal() -> Color {
    pick((112, 200, 170), (20, 128, 108))
}
pub fn c_rule() -> Color {
    pick((46, 46, 46), (222, 222, 216))
}
pub fn c_cyan() -> Color {
    pick((86, 182, 194), (10, 120, 140))
}

pub fn text() -> Style {
    Style::new().fg(c_text())
}
pub fn muted() -> Style {
    Style::new().fg(c_muted())
}
pub fn bold(c: Color) -> Style {
    Style::new().fg(c).add_modifier(Modifier::BOLD)
}
pub fn agent_color(name: &str) -> Color {
    match name {
        "build" => c_accent(),
        "port" => c_purple(),
        "reverse" => c_cyan(),
        "plan" => c_blue(),
        _ => c_muted(),
    }
}

/// Colour of a tool name in the transcript: what kind of evidence it produces.
pub fn tool_color(name: &str) -> Color {
    match name {
        "disasm" | "emulate" | "decompile" | "hexdump" | "functions" | "unity" | "dotnet"
        | "unity_assets" | "re_install" | "re_status" => c_yellow(),
        "evidence_add" | "evidence_list" | "pin" | "parity" => c_green(),
        "write" | "edit" | "scaffold" => c_accent(),
        _ => c_text(),
    }
}
