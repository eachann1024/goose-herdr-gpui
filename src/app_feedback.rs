use super::*;

impl AppView {
    pub(super) fn dismiss_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if let Some(focus) = self.feedback_previous_focus.take() {
            focus.focus(window);
        }
        cx.notify();
    }

    pub(super) fn render_feedback(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(error) = &self.error {
            return Some(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000044))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_4()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id("error-dialog")
                            .key_context("Feedback")
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "enter" | "space" | "escape" => {
                                        this.dismiss_feedback(window, cx)
                                    }
                                    "tab" => this.feedback_focus.focus(window),
                                    _ => {}
                                }
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_feedback(window, cx);
                            }))
                            .on_action(cx.listener(|this, _: &Close, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_feedback(window, cx);
                            }))
                            .w(px(420.))
                            .max_w_full()
                            .max_h_full()
                            .overflow_y_scroll()
                            .p_4()
                            .bg(rgb(p.raised))
                            .border_1()
                            .border_color(rgb(p.line))
                            .rounded(px(8.))
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .text_color(rgb(p.text))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(crate::icons::icon("warning", p.muted))
                                    .child(tr("操作未完成")),
                            )
                            .child(div().text_size(px(13.)).child(tr(error)))
                            .child(
                                div().flex().justify_end().child(
                                    div()
                                        .id("error-ok")
                                        .track_focus(&self.feedback_focus)
                                        .tab_index(0)
                                        .focus(move |s| s.border_color(rgb(p.accent)))
                                        .border_1()
                                        .border_color(rgb(p.line))
                                        .px_4()
                                        .py_1()
                                        .rounded(px(4.))
                                        .bg(rgb(p.active))
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(rgb(p.line)))
                                        .child("OK")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.dismiss_feedback(window, cx);
                                        })),
                                ),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let notice = self.notice.as_ref()?;
        Some(
            div()
                .absolute()
                .right_4()
                .bottom(px(40.))
                .max_w(px(440.))
                .p_2()
                .flex()
                .items_center()
                .gap_2()
                .bg(rgb(p.raised))
                .border_1()
                .border_color(rgb(p.line))
                .rounded(px(6.))
                .shadow_md()
                .text_size(px(13.))
                .text_color(rgb(p.text))
                .child(div().flex_1().min_w_0().child(tr(notice)))
                .child(
                    div()
                        .id("notice-dismiss")
                        .tab_index(0)
                        .size(px(24.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .focus(move |s| s.border_color(rgb(p.accent)))
                        .cursor_pointer()
                        .hover(move |s| s.bg(rgb(p.active)))
                        .child(crate::icons::icon("close", p.muted))
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space" | "escape")
                            {
                                cx.stop_propagation();
                                this.notice = None;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn render_terminal_feedback(
        &self,
        id: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = self.terminals.get(id)?;
        let terminal = pane.terminal.read(cx);
        if !terminal.is_ended() {
            return None;
        }
        let issue = terminal.connection_issue().map(str::to_owned);
        let reason = issue
            .clone()
            .or_else(|| terminal.error())
            .unwrap_or_else(|| "客户端已退出；服务端会话状态请刷新确认。".to_owned());
        let takeover = issue
            .is_some()
            .then(|| {
                id.strip_prefix(&format!("{}:", self.device().id))
                    .map(str::to_owned)
            })
            .flatten();
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .id(SharedString::from(format!("terminal-feedback-{id}")))
                        .w(px(420.))
                        .max_w_full()
                        .max_h_full()
                        .overflow_y_scroll()
                        .p_4()
                        .bg(rgb(p.raised))
                        .border_1()
                        .border_color(rgb(p.line))
                        .rounded(px(8.))
                        .shadow_md()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .text_color(rgb(p.text))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(crate::icons::icon("warning", p.muted))
                                .child(tr(if issue.is_some() {
                                    "会话已被其他客户端连接"
                                } else {
                                    "终端连接已结束"
                                })),
                        )
                        .child(div().text_size(px(13.)).child(tr(&reason)))
                        .when(takeover.is_some(), |d| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(p.muted))
                                    .child(tr("接管会话将断开原客户端的控制连接。")),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .justify_end()
                                .gap_2()
                                .child(self.btn(
                                    format!("reconnect-{id}"),
                                    "重新连接",
                                    UiAction::ReconnectView(id.to_owned()),
                                    p,
                                    cx,
                                ))
                                .when_some(takeover, |d, pane_id| {
                                    d.child(self.btn(
                                        format!("takeover-{id}"),
                                        "接管会话",
                                        UiAction::Attach(pane_id),
                                        p,
                                        cx,
                                    ))
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}
