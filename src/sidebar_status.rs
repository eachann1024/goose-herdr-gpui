use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, SharedString, Transformation, div, percentage, prelude::*,
    px, rgb,
};
use gpui_component::{Icon, IconName, Sizable, tooltip::Tooltip};

use crate::i18n::tr;

unsafe extern "C" {
    fn goose_accessibility_reduce_motion() -> bool;
}

pub fn indicator(id: SharedString, status: Option<&str>, unread: bool, dark: bool) -> AnyElement {
    let blue = rgb(if dark { 0x3b82f6 } else { 0x2563eb });
    let (glyph, label) = match status {
        Some("working") => {
            let icon = Icon::new(IconName::LoaderCircle)
                .with_size(px(12.))
                .text_color(blue);
            let glyph = if unsafe { goose_accessibility_reduce_motion() } {
                icon.into_any_element()
            } else {
                // Spinner's fixed easing differs from the existing Swift 0.9s linear turn.
                icon.with_animation(
                    "turn",
                    Animation::new(Duration::from_millis(900)).repeat(),
                    |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
                )
                .into_any_element()
            };
            (glyph, tr("运行中"))
        }
        Some("blocked") => (
            Icon::new(IconName::TriangleAlert)
                .with_size(px(12.))
                .text_color(rgb(if dark { 0xe0b36a } else { 0xb8862e }))
                .into_any_element(),
            tr("需输入"),
        ),
        Some("done") if unread => (
            div()
                .size(px(7.))
                .rounded_full()
                .bg(blue)
                .into_any_element(),
            format!("{} · {}", tr("完成"), tr("未读")),
        ),
        _ => return div().into_any_element(),
    };
    // ponytail: GPUI 0.2.2 exposes no element accessibility names; use its tooltip until supported.
    div()
        .id(id)
        .size(px(12.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        .into_any_element()
}
