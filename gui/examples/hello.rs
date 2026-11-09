use gpui::{div, prelude::*, px, rgb, App, Bounds, Context, Window, WindowBounds, WindowOptions};
use gpui_platform::application;
struct Hello;
impl Render for Hello {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x15161a))
            .text_color(rgb(0xffffff))
            .text_size(px(32.))
            .child("PortCode hello")
            .child(div().font_family("Menlo").child("mono text"))
    }
}
fn main() {
    env_logger::init();
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, gpui::size(px(600.), px(300.)), cx);
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Hello),
        );
        cx.activate(true);
    });
}
