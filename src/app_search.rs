use super::*;
use crate::i18n::tr;

type SearchResult = (
    usize,
    Option<String>,
    Option<String>,
    String,
    &'static str,
    Option<String>,
);

impl AppView {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous_focus = self
            .search_previous_focus
            .clone()
            .or_else(|| window.focused(cx));
        self.close_overlay(window, cx);
        self.search_previous_focus = previous_focus;
        let input = cx.new(|cx| TextInput::new("", tr("搜索名称、设备、目录…"), window, cx));
        input.update(cx, |input, _| input.set_search());
        self.subscriptions.push(
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                let count = this.search_results(cx).len();
                match event {
                    InputEvent::Changed => this.search_index = 0,
                    InputEvent::Up if count > 0 => {
                        this.search_index = (this.search_index.min(count - 1) + count - 1) % count
                    }
                    InputEvent::Down if count > 0 => {
                        this.search_index = (this.search_index + 1) % count
                    }
                    InputEvent::Submit => this.choose_search(this.search_index, window, cx),
                    _ => (),
                }
                cx.notify();
            }),
        );
        input.focus_handle(cx).focus(window);
        self.search = Some(input);
        self.search_index = 0;
        // Refresh live connections only; searching must not connect remote devices.
        for device in self.devices.clone() {
            let Some(state) = self.states.get_mut(&device.id) else {
                continue;
            };
            let Some(connection) = state.connection.clone() else {
                continue;
            };
            if state.loading {
                continue;
            }
            state.loading = true;
            let tx = self.tx.clone();
            let generation = state.generation;
            std::thread::spawn(move || {
                let _ = tx.send(Worker::Snapshot(
                    device.id,
                    generation,
                    connection.client.snapshot().map_err(Into::into),
                ));
            });
        }
    }

    pub(super) fn search_results(&self, cx: &App) -> Vec<SearchResult> {
        let query = self
            .search
            .as_ref()
            .map(|s| s.read(cx).text().trim().to_lowercase())
            .unwrap_or_default();
        let mut agents = Vec::new();
        let mut terminals = Vec::new();
        let mut spaces = Vec::new();
        for (index, device) in self.devices.iter().enumerate() {
            let Some(snapshot) = self
                .states
                .get(&device.id)
                .and_then(|s| s.snapshot.as_ref())
            else {
                continue;
            };
            for pane in &snapshot.panes {
                let title = snapshot_session_title(snapshot, &pane.pane_id);
                let unread = self
                    .unread
                    .contains(&(device.id.clone(), pane.pane_id.clone()));
                let status = tr(match pane.agent_status.as_deref() {
                    Some("blocked") => "需输入",
                    Some("done") if unread => "未读",
                    Some("done") => "完成",
                    Some("working") => "运行中",
                    _ => "终端",
                });
                let workspace = snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.workspace_id == pane.workspace_id)
                    .map(|w| w.label.as_str())
                    .unwrap_or("");
                let label = format!(
                    "{title} · {status} · {} / {workspace} · {}",
                    device.name,
                    pane.cwd.as_deref().unwrap_or("")
                );
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == pane.pane_id);
                let tab_label = pane.tab_id.as_deref().and_then(|tab_id| {
                    snapshot
                        .tabs
                        .iter()
                        .find(|tab| tab.tab_id == tab_id)
                        .and_then(|tab| {
                            tab.custom_label
                                .as_deref()
                                .filter(|label| !label.is_empty())
                                .or(Some(tab.label.as_str()).filter(|label| !label.is_empty()))
                        })
                });
                let search_label = format!(
                    "{label} · {} · {} · {} · {}",
                    pane.agent.as_deref().unwrap_or(""),
                    agent.and_then(|agent| agent.name.as_deref()).unwrap_or(""),
                    agent.and_then(|agent| agent.title.as_deref()).unwrap_or(""),
                    tab_label.unwrap_or("")
                );
                if let Some(score) = match_rank(&title, &search_label, &query) {
                    let icon = crate::icons::pane_kind(&pane.extra, pane.agent.as_deref());
                    let row = (
                        index,
                        Some(pane.pane_id.clone()),
                        Some(pane.workspace_id.clone()),
                        label,
                    );
                    if pane.agent.is_some() {
                        let priority = match pane.agent_status.as_deref() {
                            Some("blocked") => 0,
                            Some("done") if unread => 1,
                            Some("working") => 2,
                            Some("done") => 3,
                            _ => 4,
                        };
                        agents.push((
                            (priority, score, index),
                            (row.0, row.1, row.2, row.3, "agent", icon),
                        ));
                    } else {
                        terminals.push((score, (row.0, row.1, row.2, row.3, "terminal", None)));
                    }
                }
            }
            for workspace in &snapshot.workspaces {
                let label = tr(&format!(
                    "空间 · {} · {} · {}",
                    workspace.label,
                    device.name,
                    workspace.cwd.as_deref().unwrap_or("")
                ));
                if match_rank(&workspace.label, &label, &query).is_some() {
                    spaces.push((
                        index,
                        None,
                        Some(workspace.workspace_id.clone()),
                        label,
                        "space",
                        Some("all-spaces".into()),
                    ));
                }
            }
        }
        agents.sort_by(|a, b| a.0.cmp(&b.0));
        terminals.sort_by(|a, b| a.0.cmp(&b.0));
        agents
            .into_iter()
            .map(|(_, row)| row)
            .chain(terminals.into_iter().map(|(_, row)| row))
            .chain(spaces)
            .collect()
    }

    fn choose_search(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let results = self.search_results(cx);
        let Some((device, pane, workspace, _, _, _)) = results
            .get(index.min(results.len().saturating_sub(1)))
            .cloned()
        else {
            return;
        };
        self.selected_device = device;
        self.workspace = workspace;
        self.selected_pane = pane.clone();
        self.screen = Screen::Workspace;
        self.search_previous_focus = None;
        self.close_overlay(window, cx);
        if let Some(pane) = pane {
            self.attach(pane);
        } else if !self
            .states
            .get(&self.device().id)
            .is_some_and(|s| s.loading || s.connection.is_some())
        {
            self.connect();
        }
        cx.notify();
    }

    pub(super) fn render_search(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let all = self.search_results(cx);
        let count = all.len();
        let active = self.search_index.min(count.saturating_sub(1));
        // Keep the keyboard-selected row visible without a second scroll-position state.
        let start = active.saturating_sub(5).min(count.saturating_sub(10));
        let mut results = div().id("search-results").flex().flex_col().gap_1();
        let mut last_kind = "";
        for (index, (device_index, _, workspace, label, kind, icon)) in
            all.into_iter().enumerate().skip(start).take(10)
        {
            if kind != last_kind {
                last_kind = kind;
                results = results.child(
                    div()
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .child(tr(match kind {
                            "agent" => "Agents",
                            "terminal" => "终端",
                            _ => "空间",
                        })),
                );
            }
            results = results.child(
                div()
                    .id(("search-result", index))
                    .px_2()
                    .py_2()
                    .rounded(px(4.))
                    .text_size(px(13.))
                    .min_w_0()
                    .when(index == active, |d| d.bg(rgb(p.active)))
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(move |d| d.bg(rgb(p.active)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_shrink_0().child(if kind == "space" {
                        self.project_icon(
                            &self.devices[device_index],
                            workspace.as_deref().unwrap_or(""),
                            13.,
                            p.text,
                        )
                    } else {
                        crate::icons::agent_mark(icon.as_deref(), 13., p.text).into_any_element()
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(crate::fade_label::fade_label(label.clone())),
                    )
                    .on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.choose_search(index, window, cx)
                        }),
                    )
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            this.choose_search(index, window, cx);
                        }
                    })),
            );
        }
        let loading = self.states.values().any(|state| state.loading);
        let device_ready = |state: &DeviceState| {
            search_device_ready(state.connection.is_some(), state.loading, &state.status)
        };
        let ready = self
            .devices
            .iter()
            .all(|device| self.states.get(&device.id).is_some_and(device_ready));
        if count == 0 && (loading || ready) {
            results = results.child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(p.muted))
                    .when(ready, |d| {
                        d.child(crate::icons::illustration("search", 88.))
                    })
                    .child(tr(if loading {
                        "正在获取设备快照…"
                    } else {
                        "没有匹配的设备、空间或会话"
                    })),
            );
        }
        let mut statuses = div()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(11.))
            .text_color(rgb(p.muted));
        for device in &self.devices {
            let state = self.states.get(&device.id);
            if !state.is_some_and(device_ready) {
                statuses = statuses.child(format!(
                    "{} · {}",
                    device.name,
                    tr(match state {
                        Some(state) if state.loading => "查询中…",
                        Some(state) => &state.status,
                        None => "未连接",
                    })
                ));
            }
        }
        let footer = if count == 0 {
            "↑↓ 选择 · Enter 打开 · Esc 关闭".to_owned()
        } else {
            format!("{} / {count} · ↑↓ 选择 · Enter 打开 · Esc 关闭", active + 1)
        };
        self.centered_overlay(
            div()
                .id("global-search-panel")
                .max_h_full()
                .overflow_y_scroll()
                .w(px(560.))
                .max_w_full()
                .h_auto()
                .bg(rgb(p.side))
                .border_1()
                .border_color(rgb(p.line))
                .rounded(px(8.))
                .shadow_lg()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().children(self.search.clone()))
                        .child(self.btn("search-dismiss", "Esc", UiAction::Dismiss, p, cx)),
                )
                .child(results)
                .child(statuses)
                .child(
                    div()
                        .pt_2()
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .text_size(px(11.))
                        .text_color(rgb(p.muted))
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().child(tr(&footer)))
                        .when(count > 10, |d| {
                            d.child(
                                div()
                                    .id("search-prev-page")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .tab_index(0)
                                    .hover(move |d| d.bg(rgb(p.active)))
                                    .child(tr("上一页"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_index = this.search_index.saturating_sub(10);
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                cx.stop_propagation();
                                                this.search_index =
                                                    this.search_index.saturating_sub(10);
                                                cx.notify();
                                            }
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .id("search-next-page")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .tab_index(0)
                                    .hover(move |d| d.bg(rgb(p.active)))
                                    .child(tr("下一页"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let count = this.search_results(cx).len();
                                        this.search_index =
                                            (this.search_index + 10).min(count.saturating_sub(1));
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                cx.stop_propagation();
                                                let count = this.search_results(cx).len();
                                                this.search_index = (this.search_index + 10)
                                                    .min(count.saturating_sub(1));
                                                cx.notify();
                                            }
                                        },
                                    )),
                            )
                        }),
                ),
            cx,
        )
    }

    pub(super) fn open_terminal_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .active_terminal
            .as_ref()
            .is_none_or(|id| !self.terminals.contains_key(id))
        {
            self.error = Some(tr("请先打开一个终端"));
            return;
        }
        self.show_form(
            FormKind::TerminalSearch,
            &tr("搜索当前终端"),
            vec![(&tr("正则表达式（区分大小写）"), String::new())],
            window,
            cx,
        );
    }

    pub(super) fn submit_terminal_search(
        &mut self,
        query: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if query.is_empty() {
            bail!(tr("请输入搜索内容"));
        }
        let terminal = self
            .active_terminal
            .as_ref()
            .and_then(|id| self.terminals.get(id))
            .ok_or_else(|| anyhow::anyhow!(tr("终端已关闭")))?
            .terminal
            .clone();
        let found = terminal.update(cx, |terminal, cx| {
            let result = terminal.search(query, cx);
            cx.notify();
            result
        })?;
        self.notice = Some(tr(if found {
            "已定位并选中匹配文本"
        } else {
            "当前终端中没有匹配文本"
        }));
        Ok(())
    }
}

fn search_device_ready(connected: bool, loading: bool, status: &str) -> bool {
    connected && !loading && status.starts_with("已连接")
}

fn match_rank(title: &str, label: &str, query: &str) -> Option<u8> {
    let title = title.to_lowercase();
    if query.is_empty() || title == query {
        Some(0)
    } else if title.starts_with(query) {
        Some(1)
    } else if title.contains(query) {
        Some(2)
    } else if query
        .split_whitespace()
        .all(|part| label.to_lowercase().contains(part))
    {
        Some(3)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_empty_state_requires_a_healthy_completed_connection() {
        assert!(super::search_device_ready(true, false, "已连接 · 1"));
        assert!(!super::search_device_ready(true, true, "已连接 · 1"));
        assert!(!super::search_device_ready(false, false, "已连接 · 1"));
        for status in [
            "未连接",
            "连接中…",
            "连接失败：test",
            "控制连接中断：test",
            "事件流断开：test",
        ] {
            assert!(!super::search_device_ready(true, false, status));
        }
    }

    #[test]
    fn ranking_prefers_exact_then_prefix_and_supports_multiword_context() {
        assert_eq!(
            super::match_rank("Build", "Build · Mac · /src", "build"),
            Some(0)
        );
        assert_eq!(super::match_rank("Build app", "", "build"), Some(1));
        assert_eq!(
            super::match_rank("app", "app · Mac · /src", "mac src"),
            Some(3)
        );
        assert_eq!(super::match_rank("app", "app · Mac", "linux"), None);
    }

    #[test]
    fn agent_metadata_is_queryable_without_changing_the_visible_label() {
        let query_text = "Terminal · Mac · /repo · codex · eachann · Fix sidebar · Review";
        for query in ["codex", "eachann", "review"] {
            assert_eq!(super::match_rank("Terminal", query_text, query), Some(3));
        }
    }
}
