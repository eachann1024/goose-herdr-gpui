use super::*;
use crate::i18n::{format as localized_format, tr};

#[derive(Clone, Copy)]
enum InspectorOperation {
    Read,
    Input,
    Prompt,
    RenameAgent,
    RenameTab,
    Keys(&'static [&'static str]),
}

impl AppView {
    pub(super) fn open_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_pane.clone() else {
            self.error = Some(tr("请先选择一个服务端会话"));
            return;
        };
        self.close_overlay(window, cx);
        let pane = self
            .snapshot()
            .and_then(|s| s.panes.iter().find(|p| p.pane_id == id))
            .cloned();
        let device = self.device();
        self.inspector = Some(
            json!({"device_id":device.id,"device_name":device.name,"pane_id":id,"tab_id":pane.as_ref().and_then(|p|p.tab_id.clone()),"agent":pane.as_ref().and_then(|p|p.agent.clone()),"cwd":pane.as_ref().and_then(|p|p.cwd.clone()),"agent_status":pane.as_ref().and_then(|p|p.agent_status.clone()),"loading":false}),
        );
        let input =
            cx.new(|cx| TextInput::new("", tr("要发送的文字（不会自动追加回车）"), window, cx));
        input.focus_handle(cx).focus(window);
        self.inspector_input = Some(input);
        self.fields.remove("inspector-agent-name");
        self.fields.remove("inspector-tab-name");
        self.input(
            "inspector-agent-name",
            String::new(),
            &tr("新的 Agent 身份名称"),
            window,
            cx,
        );
        self.input(
            "inspector-tab-name",
            pane.and_then(|p| p.terminal_title).unwrap_or_default(),
            &tr("标签显示名称，可用中文"),
            window,
            cx,
        );
        self.inspector_operation(InspectorOperation::Read, cx);
    }

    fn inspector_operation(&mut self, operation: InspectorOperation, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let state = self
                .inspector
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(tr("详情已关闭")))?;
            if state["loading"].as_bool() == Some(true) {
                return Ok(());
            }
            let device = state["device_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(tr("设备标识缺失")))?
                .to_owned();
            let pane = state["pane_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!(tr("会话标识缺失")))?
                .to_owned();
            let tab = state["tab_id"].as_str().map(str::to_owned);
            let connection = self
                .states
                .get(&device)
                .and_then(|s| s.connection.clone())
                .ok_or_else(|| anyhow::anyhow!(tr("设备未连接")))?;
            let keys = match operation {
                InspectorOperation::Keys(keys) => keys.iter().map(|s| (*s).to_owned()).collect(),
                _ => Vec::new(),
            };
            let text = match operation {
                InspectorOperation::Read | InspectorOperation::Keys(_) => String::new(),
                InspectorOperation::Input | InspectorOperation::Prompt => self
                    .inspector_input
                    .as_ref()
                    .map(|i| i.read(cx).text().to_owned())
                    .unwrap_or_default(),
                InspectorOperation::RenameAgent => self
                    .fields
                    .get("inspector-agent-name")
                    .map(|i| i.read(cx).text().trim().to_owned())
                    .unwrap_or_default(),
                InspectorOperation::RenameTab => self
                    .fields
                    .get("inspector-tab-name")
                    .map(|i| i.read(cx).text().trim().to_owned())
                    .unwrap_or_default(),
            };
            if !matches!(
                operation,
                InspectorOperation::Read | InspectorOperation::Keys(_)
            ) && text.is_empty()
            {
                bail!(tr("请输入内容"))
            }
            if text.contains('\0') {
                bail!(tr("内容不能包含NUL"))
            }
            if matches!(operation, InspectorOperation::RenameAgent) && !valid_agent_name(&text) {
                bail!(tr(
                    "Agent 身份名称须以小写字母开头，最多32位小写字母、数字、_ 或 -"
                ))
            }
            if matches!(
                operation,
                InspectorOperation::Prompt | InspectorOperation::RenameAgent
            ) && state["agent"].as_str().is_none()
            {
                bail!(tr("当前会话不是 Agent"))
            }
            if matches!(operation, InspectorOperation::RenameTab) && tab.is_none() {
                bail!(tr("当前会话没有标签标识"))
            }
            self.inspector_generation += 1;
            let generation = self.inspector_generation;
            let state = self.inspector.as_mut().unwrap();
            state["loading"] = json!(true);
            state["error"] = Value::Null;
            state["message"] = Value::Null;
            let mut state = state.clone();
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<Value> {
                    match operation {
                        InspectorOperation::Read => {}
                        InspectorOperation::Input => {
                            connection.client.send_input(&pane, &text)?;
                        }
                        InspectorOperation::Keys(_) => {
                            connection.client.send_keys(&pane, &keys)?;
                        }
                        InspectorOperation::Prompt => {
                            connection.client.prompt(&pane, &text)?;
                        }
                        InspectorOperation::RenameAgent => {
                            connection.client.rename_agent(&pane, &text)?;
                        }
                        InspectorOperation::RenameTab => {
                            connection
                                .client
                                .rename_tab(tab.as_deref().unwrap(), &text)?;
                        }
                    }
                    state["loading"] = json!(false);
                    if !matches!(operation, InspectorOperation::Read) {
                        state["message"] = json!(tr("服务端已确认此操作；不会自动重复发送。"));
                    }
                    match connection.client.get_pane(&pane) {
                        Ok(metadata) => {
                            if let Some(pane) = metadata.get("pane") {
                                for key in ["agent", "agent_status", "cwd", "tab_id"] {
                                    if let Some(value) = pane.get(key) {
                                        state[key] = value.clone();
                                    }
                                }
                            }
                            state["metadata"] = metadata;
                        }
                        Err(e) => {
                            state["error"] =
                                json!(localized_format("详情刷新失败：{}", &[&e.to_string()]));
                        }
                    }
                    match connection.client.read_pane(&pane) {
                        Ok(screen) => state["screen"] = screen,
                        Err(e) => {
                            state["error"] =
                                json!(localized_format("可见输出读取失败：{}", &[&e.to_string()]));
                        }
                    }
                    Ok(state)
                })();
                let _ = tx.send(Worker::Inspector(generation, result));
            });
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(state) = self.inspector.as_mut() {
                state["error"] = json!(error.to_string());
            } else {
                self.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    pub(super) fn inspector_result(
        &mut self,
        generation: u64,
        result: Result<Value>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.inspector_generation || self.inspector.is_none() {
            return;
        }
        match result {
            Ok(state) => self.inspector = Some(state),
            Err(error) => {
                let state = self.inspector.as_mut().unwrap();
                state["loading"] = json!(false);
                state["error"] = json!(error.to_string());
            }
        }
        cx.notify();
    }

    fn inspector_button(
        &self,
        id: &'static str,
        label: &'static str,
        operation: InspectorOperation,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let busy = self
            .inspector
            .as_ref()
            .is_some_and(|s| s["loading"].as_bool() == Some(true));
        div()
            .id(id)
            .tab_index(0)
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_size(px(12.))
            .text_color(rgb(if busy { p.muted } else { p.text }))
            .cursor_pointer()
            .hover(move |d| d.bg(rgb(p.active)))
            .focus(move |d| d.bg(rgb(p.active)).border_color(rgb(p.accent)))
            .child(tr(label))
            .on_click(cx.listener(move |this, _, _, cx| this.inspector_operation(operation, cx)))
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.inspector_operation(operation, cx);
                }
            }))
    }

    pub(super) fn render_inspector(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let state = self.inspector.as_ref().unwrap();
        let text = |key: &str| state[key].as_str().unwrap_or("—").to_owned();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_size(px(18.)).child(tr("会话详情")))
                    .child(self.btn("inspector-close", "关闭", UiAction::Dismiss, p, cx)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(format!("{} · {}", text("device_name"), text("pane_id"))),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(localized_format("Agent：{}", &[&text("agent")]))
                    .child(localized_format("状态：{}", &[&text("agent_status")])),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(localized_format("目录：{}", &[&text("cwd")])),
            );
        if let Some(error) = state["error"].as_str() {
            body = body.child(
                div()
                    .text_color(rgb(p.text))
                    .child(localized_format("操作未完成：{}", &[error])),
            );
        }
        if let Some(message) = state["message"].as_str() {
            body = body.child(div().text_color(rgb(p.muted)).child(message.to_owned()));
        }
        let output = state
            .get("screen")
            .map(screen_text)
            .unwrap_or_else(|| tr("尚未读取输出"));
        let copy = output.clone();
        let copy_on_key = output.clone();
        body = body
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section(
                        &tr(if state["loading"].as_bool() == Some(true) {
                            "正在读取 / 执行…"
                        } else {
                            "服务端可见输出"
                        }),
                        p,
                    ))
                    .child(self.inspector_button(
                        "inspector-refresh",
                        "刷新",
                        InspectorOperation::Read,
                        p,
                        cx,
                    ))
                    .child(
                        div()
                            .id("inspector-copy")
                            .tab_index(0)
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .child(tr("复制输出"))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                            })
                            .on_key_down(cx.listener(move |_, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.stop_propagation();
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_on_key.clone(),
                                    ));
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .id("inspector-output")
                    .h(px(210.))
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .bg(rgb(p.bg))
                    .border_1()
                    .border_color(rgb(p.line))
                    .font_family("Menlo")
                    .text_size(px(12.))
                    .child(output),
            );
        if let Some(input) = &self.inspector_input {
            body = body.child(
                div()
                    .border_t_1()
                    .border_color(rgb(p.line))
                    .pt_3()
                    .child(section(&tr("输入内容"), p))
                    .child(input.clone())
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap_2()
                            .child(self.inspector_button(
                                "inspector-input",
                                "发送文字（不追加回车）",
                                InspectorOperation::Input,
                                p,
                                cx,
                            ))
                            .when(state["agent"].as_str().is_some(), |d| {
                                d.child(self.inspector_button(
                                    "inspector-prompt",
                                    "提交 Agent 提示词",
                                    InspectorOperation::Prompt,
                                    p,
                                    cx,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .child(tr("Ctrl-C 会中断当前进程。")),
                    )
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap_2()
                            .child(self.inspector_button(
                                "inspector-up",
                                "↑",
                                InspectorOperation::Keys(&["up"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-down",
                                "↓",
                                InspectorOperation::Keys(&["down"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-left",
                                "←",
                                InspectorOperation::Keys(&["left"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-right",
                                "→",
                                InspectorOperation::Keys(&["right"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-enter",
                                "Enter",
                                InspectorOperation::Keys(&["enter"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-esc",
                                "Esc",
                                InspectorOperation::Keys(&["escape"]),
                                p,
                                cx,
                            ))
                            .child(self.inspector_button(
                                "inspector-ctrl-c",
                                "Ctrl-C",
                                InspectorOperation::Keys(&["ctrl+c"]),
                                p,
                                cx,
                            )),
                    ),
            );
        }
        if state["agent"].as_str().is_some() {
            if let Some(input) = self.fields.get("inspector-agent-name") {
                body = body.child(
                    div()
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .pt_2()
                        .child(section(
                            &tr("Agent 身份名称（用于 CLI 定位，不是显示标签）"),
                            p,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().child(input.clone()))
                                .child(self.inspector_button(
                                    "inspector-rename-agent",
                                    "修改身份",
                                    InspectorOperation::RenameAgent,
                                    p,
                                    cx,
                                )),
                        ),
                );
            }
        }
        if state["tab_id"].as_str().is_some() {
            if let Some(input) = self.fields.get("inspector-tab-name") {
                body = body.child(
                    div()
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .pt_2()
                        .child(section(
                            &tr("标签显示名称（支持中文，不改变 Agent 身份）"),
                            p,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().child(input.clone()))
                                .child(self.inspector_button(
                                    "inspector-rename-tab",
                                    "修改标签",
                                    InspectorOperation::RenameTab,
                                    p,
                                    cx,
                                )),
                        ),
                );
            }
        }
        if let Some(metadata) = state.get("metadata") {
            body = body.child(section(&tr("原始元数据"), p)).child(
                div()
                    .id("inspector-metadata")
                    .h(px(100.))
                    .overflow_y_scroll()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .font_family("Menlo")
                    .child(serde_json::to_string_pretty(metadata).unwrap_or_default()),
            );
        }
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000066))
            .flex()
            .items_center()
            .justify_center()
            .key_context("OverlayPopup")
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.dispatch(UiAction::Dismiss, window, cx);
            }))
            .child(
                div()
                    .id("inspector-dialog")
                    .w(px(720.))
                    .max_w_full()
                    .max_h(relative(0.92))
                    .overflow_y_scroll()
                    .p_5()
                    .bg(rgb(p.raised))
                    .border_1()
                    .border_color(rgb(p.line))
                    .rounded(px(8.))
                    .shadow_lg()
                    .child(body),
            )
            .into_any_element()
    }
}

fn screen_text(value: &Value) -> String {
    value
        .pointer("/read/text")
        .or_else(|| value.get("text"))
        .or_else(|| value.get("output"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .or_else(|| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string_pretty(value).unwrap_or_default())
}
#[cfg(test)]
mod tests {
    use super::screen_text;
    use serde_json::json;
    #[test]
    fn pane_output_projection() {
        assert_eq!(
            screen_text(&json!({"read":{"text":"真实输出","revision":1}})),
            "真实输出"
        );
        assert_eq!(screen_text(&json!("直接输出")), "直接输出");
    }
}
