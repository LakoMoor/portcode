//! Colours and fonts. One idea carries the identity: the three kinds of evidence (observed, inferred, unknown)
//! each have a colour of their own and nothing else in the window competes with them. Ember is the brand accent.
use gpui::{rgb, Rgba};

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Rgba,
    pub panel: Rgba,
    pub raised: Rgba,
    pub rule: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub accent: Rgba,
    pub seen: Rgba,
    pub inferred: Rgba,
    pub unknown: Rgba,
    pub blue: Rgba,
    pub purple: Rgba,
    pub on_accent: Rgba,
    pub dark: bool,
}

pub fn dark() -> Theme {
    Theme {
        bg: rgb(0x101319),
        panel: rgb(0x161a22),
        raised: rgb(0x1d222c),
        rule: rgb(0x2a303c),
        text: rgb(0xe9e6df),
        muted: rgb(0x9097a6),
        faint: rgb(0x5f6676),
        accent: rgb(0xf2a65a),
        seen: rgb(0x5ccfa0),
        inferred: rgb(0xf0b54a),
        unknown: rgb(0xf07167),
        blue: rgb(0x6aa9f5),
        purple: rgb(0xa48bea),
        on_accent: rgb(0x1a1206),
        dark: true,
    }
}

pub fn light() -> Theme {
    Theme {
        bg: rgb(0xf4f5f7),
        panel: rgb(0xffffff),
        raised: rgb(0xeceef2),
        rule: rgb(0xdde0e6),
        text: rgb(0x1b1f27),
        muted: rgb(0x646b7a),
        faint: rgb(0x9aa1ae),
        accent: rgb(0xc4621c),
        seen: rgb(0x1e8a5e),
        inferred: rgb(0xa86b00),
        unknown: rgb(0xc2392f),
        blue: rgb(0x2a6fd6),
        purple: rgb(0x6b4fc7),
        on_accent: rgb(0xffffff),
        dark: false,
    }
}

pub fn agent_color(t: &Theme, name: &str) -> Rgba {
    match name {
        "build" => t.accent,
        "port" => t.purple,
        "reverse" => t.seen,
        "plan" => t.blue,
        _ => t.muted,
    }
}

/// The colour of a tool name: what kind of evidence it produces.
pub fn tool_color(t: &Theme, name: &str) -> Rgba {
    match name {
        "disasm" | "emulate" | "decompile" | "hexdump" | "functions" | "unity" | "dotnet"
        | "unity_assets" | "re_install" | "re_status" => t.inferred,
        "evidence_add" | "evidence_list" | "pin" | "parity" => t.seen,
        "write" | "edit" | "scaffold" => t.accent,
        _ => t.text,
    }
}

#[cfg(target_os = "macos")]
pub const UI_FONT: &str = "Helvetica Neue";
#[cfg(target_os = "windows")]
pub const UI_FONT: &str = "Segoe UI";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const UI_FONT: &str = "Noto Sans";

#[cfg(target_os = "macos")]
pub const MONO_FONT: &str = "Menlo";
#[cfg(target_os = "windows")]
pub const MONO_FONT: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const MONO_FONT: &str = "DejaVu Sans Mono";
