//! Run with `cargo run --example text_probe`; no Herdr connection or saved settings.
use gpui::{prelude::*, *};
struct TextProbe;
impl Render for TextProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut rows = div()
            .size_full()
            .bg(rgb(0x191b1e))
            .p_6()
            .flex()
            .flex_col()
            .gap_4();
        for family in [
            ".AppleSystemUIFont",
            ".SystemUIFont",
            "Helvetica",
            "Menlo",
            "PingFang SC",
        ] {
            rows = rows.child(
                div()
                    .bg(rgb(0x292c32))
                    .text_color(rgb(0xffffff))
                    .text_size(px(20.))
                    .font_family(family)
                    .child(format!("{family}: Hello 世界 123 ✓ 😀")),
            );
        }
        rows.child(
            canvas(
                |_, _, _| (),
                |bounds, _, window, cx| {
                    let text: SharedString = "DIRECT Hello 世界 😀".into();
                    let run = TextRun {
                        len: text.len(),
                        font: font("Helvetica"),
                        color: rgb(0xffffff).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(text, px(24.), &[run], None);
                    assert!(
                        line.width > px(0.) && !line.runs.is_empty(),
                        "Native text shaping failed"
                    );
                    line.paint(bounds.origin, px(36.), window, cx)
                        .expect("Native glyph painting failed");
                },
            )
            .w_full()
            .h(px(48.)),
        )
    }
}
fn main() {
    Application::new().run(|cx| {
        // Missing gpui/font-kit silently selects NoopTextSystem on macOS.
        assert!(
            !cx.text_system().all_font_names().is_empty(),
            "Native font backend missing: enable gpui/font-kit"
        );
        cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| TextProbe))
            .unwrap();
        cx.activate(true);
    });
}
