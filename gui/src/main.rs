//! PortCode desktop app. Same agent as the terminal, in a window.
mod backend;
mod markdown;
mod state;
mod theme;
mod view;

use gpui::{
    point, px, size, App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};
use gpui_platform::application;

fn project_root() -> std::path::PathBuf {
    // `portcode-gui [folder]`; started from Finder there is no useful working directory, so use the home folder
    if let Some(p) = std::env::args().nth(1) {
        let p = std::path::PathBuf::from(p);
        if p.is_dir() {
            return p;
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.parent().is_none() {
        if let Some(h) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return h.into();
        }
    }
    cwd
}

fn main() {
    let root = project_root();
    let deep = std::env::args().any(|a| a == "--deep");
    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("PortCode".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(16.), px(14.))),
                }),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| view::Root::new(root, deep, cx));
                let focus = view.read(cx).focus_handle();
                window.focus(&focus, cx);
                view
            },
        );
        if opened.is_err() {
            eprintln!("portcode-gui: could not open a window");
        }
        cx.activate(true);
    });
}
