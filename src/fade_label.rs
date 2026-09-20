use gpui::{
    App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId, Hitbox,
    InspectorElementId, Interactivity, IntoElement, LayoutId, Pixels, ShapedLine, SharedString,
    Style, Window, point, px, size,
};

/// Single-line display text; only truncated text gets a full-title tooltip.
pub fn fade_label(text: impl Into<SharedString>) -> impl IntoElement {
    FadeLabel(text.into(), Interactivity::new())
}

struct FadeLabel(SharedString, Interactivity);

impl IntoElement for FadeLabel {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

fn measured_width(
    text_width: Pixels,
    known_width: Option<Pixels>,
    available_width: AvailableSpace,
) -> Pixels {
    // Match GPUI's native text measurement without expanding a constrained slot.
    let width = text_width.ceil();
    known_width
        .unwrap_or_else(|| match available_width {
            AvailableSpace::Definite(available) => width.min(available),
            _ => width,
        })
        .max(px(0.))
}

fn fade_start(text_width: Pixels, available_width: Pixels) -> Option<Pixels> {
    // Taffy rounds layout bounds; subpixel rounding alone is not truncation.
    (available_width > px(0.) && text_width.round() > available_width.round())
        .then(|| (available_width - px(24.)).max(px(0.)))
}

impl Element for FadeLabel {
    type RequestLayoutState = ShapedLine;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.0.clone().into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ShapedLine) {
        let style = window.text_style();
        // External display names can contain line breaks; do not feed them to shape_line.
        let text: SharedString = if self.0.contains(['\n', '\r']) {
            self.0.replace(['\n', '\r'], " ").into()
        } else {
            self.0.clone()
        };
        let line = window.text_system().shape_line(
            text.clone(),
            style.font_size.to_pixels(window.rem_size()),
            &[style.to_run(text.len())],
            None,
        );
        let width = line.width;
        let height = window.line_height();
        let mut layout_style = Style::default();
        layout_style.min_size.width = px(0.).into();
        let layout = self
            .1
            .request_layout(id, inspector, window, cx, |_, window, _| {
                window.request_measured_layout(layout_style, move |known, available, _, _| {
                    size(
                        measured_width(width, known.width, available.width),
                        known.height.unwrap_or(height),
                    )
                })
            });
        (layout, line)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        line: &mut ShapedLine,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Hitbox> {
        if fade_start(line.width, bounds.size.width).is_some() {
            let text = self.0.clone();
            self.1.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(text.clone()).build(window, cx)
            });
        }
        self.1.prepaint(
            id,
            inspector,
            bounds,
            bounds.size,
            window,
            cx,
            |_, _, hitbox, _, _| hitbox,
        )
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        line: &mut ShapedLine,
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.1.paint(
            id,
            inspector,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |_, _, _| {},
        );
        if bounds.size.width <= px(0.) {
            return;
        }
        let line_height = window.line_height();
        let paint = |line: &ShapedLine, clip: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
            window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
                if let Err(error) = line.paint(bounds.origin, line_height, window, cx) {
                    log::error!("Failed to paint title: {error}");
                }
            });
        };
        let Some(start) = fade_start(line.width, bounds.size.width) else {
            paint(line, bounds, window, cx);
            return;
        };
        paint(
            line,
            Bounds::new(bounds.origin, size(start, bounds.size.height)),
            window,
            cx,
        );
        let style = window.text_style();
        let run = style.to_run(line.text.len());
        // ponytail: GPUI 0.2.2 has no public alpha mask; cap the tail at 16 strips
        // with cached text layouts until a native gradient text mask is available.
        for strip in 0..16 {
            let left = start + (bounds.size.width - start) * (strip as f32 / 16.);
            let right = start + (bounds.size.width - start) * ((strip + 1) as f32 / 16.);
            let alpha = (bounds.size.width - (left + right) / 2.) / (bounds.size.width - start);
            let mut faded_run = run.clone();
            faded_run.color = run.color.opacity(alpha);
            let faded = window.text_system().shape_line(
                line.text.clone(),
                line.font_size,
                &[faded_run],
                None,
            );
            paint(
                &faded,
                Bounds::new(
                    point(bounds.left() + left, bounds.top()),
                    size(right - left, bounds.size.height),
                ),
                window,
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_text_width_reserves_space_without_expanding_constraints() {
        let natural = measured_width(px(18.49), None, AvailableSpace::MaxContent);
        assert_eq!(natural, px(19.));
        assert_eq!(fade_start(px(18.49), natural), None);
        let known_fractional =
            measured_width(px(18.75), Some(px(18.8)), AvailableSpace::MaxContent);
        assert_eq!(known_fractional, px(18.8));
        assert_eq!(fade_start(px(18.75), known_fractional), None);
        assert_eq!(
            measured_width(px(18.49), None, AvailableSpace::Definite(px(18.25))),
            px(18.25)
        );
        assert_eq!(
            measured_width(px(18.49), Some(px(12.25)), AvailableSpace::MaxContent),
            px(12.25)
        );
        assert_eq!(
            measured_width(px(18.49), None, AvailableSpace::Definite(px(-1.))),
            px(0.)
        );
    }

    #[test]
    fn only_overflowing_positive_width_fades() {
        assert_eq!(fade_start(px(80.), px(100.)), None);
        assert_eq!(fade_start(px(100.), px(100.)), None);
        assert_eq!(fade_start(px(18.75), px(18.8)), None);
        assert_eq!(fade_start(px(18.49), px(18.)), None);
        assert_eq!(fade_start(px(100.49), px(100.)), None);
        assert_eq!(fade_start(px(100.5), px(100.)), Some(px(76.)));
        assert_eq!(fade_start(px(101.), px(100.)), Some(px(76.)));
        assert_eq!(fade_start(px(20.), px(10.)), Some(px(0.)));
        assert_eq!(fade_start(px(20.), px(0.)), None);
    }
}
