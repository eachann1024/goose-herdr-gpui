use super::*;
use crate::i18n::tr;

struct SettingsCategory {
    page: SettingsPage,
    label: &'static str,
    keywords: &'static str,
    sections: &'static [(&'static str, &'static str)],
}
const SETTINGS_CATEGORIES: &[SettingsCategory] = &[
    SettingsCategory {
        page: SettingsPage::Appearance,
        label: "外观",
        keywords: "appearance theme language",
        sections: &[
            ("界面", "主题 语言 theme language interface"),
            ("侧栏", "隐藏空间 优先显示会话 sidebar hide spaces sessions"),
        ],
    },
    SettingsCategory {
        page: SettingsPage::Terminal,
        label: "终端",
        keywords: "terminal",
        sections: &[
            ("字体", "字号 字重 行距 font size weight line spacing"),
            (
                "布局",
                "内边距 外边距 网格 余量 分栏 不透明度 layout padding margin balance opacity split",
            ),
            ("交互", "鼠标 选中 复制 interaction mouse selection copy"),
        ],
    },
    SettingsCategory {
        page: SettingsPage::Agents,
        label: "Agents",
        keywords: "agent",
        sections: &[
            ("启动行为", "启动 权限 launch bypass"),
            (
                "可用 Agents",
                "路径 命令 检测 启用 停用 上移 下移 排序 授权 Cursor available path command detection enable disable order authorization",
            ),
        ],
    },
    SettingsCategory {
        page: SettingsPage::Shortcuts,
        label: "快捷键",
        keywords: "shortcuts keyboard",
        sections: &[
            ("快速新建 Agent", "quick new agent"),
            ("键盘快捷键", "keyboard shortcuts"),
            ("高级", "advanced"),
        ],
    },
    SettingsCategory {
        page: SettingsPage::Notifications,
        label: "通知",
        keywords: "notifications",
        sections: &[
            ("提醒", "提示音 sound alerts"),
            ("系统权限", "权限 授权 system permission"),
        ],
    },
];
impl SettingsCategory {
    fn matching_sections(&self, query: &str, agents: &str) -> Vec<&'static str> {
        let category_match = settings_match(query, self.label, self.keywords);
        self.sections
            .iter()
            .filter(|(label, keywords)| {
                let mut keywords = keywords.to_string();
                if matches!(*label, "可用 Agents" | "快速新建 Agent") {
                    keywords.push_str(agents);
                }
                if matches!(*label, "键盘快捷键" | "高级") {
                    for &(id, action, default) in super::app_shortcuts::SHORTCUTS {
                        if AppView::is_advanced_shortcut(id) == (*label == "高级") {
                            keywords.push_str(&format!(" {id} {action} {} {default}", tr(action)));
                        }
                    }
                }
                category_match
                    || settings_match(
                        query,
                        label,
                        &format!(
                            "{} {} {} {}",
                            self.label,
                            tr(self.label),
                            self.keywords,
                            keywords
                        ),
                    )
            })
            .map(|(label, _)| *label)
            .collect()
    }
    fn matches(&self, query: &str, agents: &str) -> bool {
        settings_match(query, self.label, self.keywords)
            || !self.matching_sections(query, agents).is_empty()
    }
}
struct SettingsNavigation {
    expanded: [bool; 6],
    section: Option<&'static str>,
    keyboard: bool,
    scroll: ScrollHandle,
    anchors: HashMap<&'static str, ScrollAnchor>,
}
impl Default for SettingsNavigation {
    fn default() -> Self {
        let scroll = ScrollHandle::new();
        let anchors = SETTINGS_CATEGORIES
            .iter()
            .flat_map(|category| category.sections.iter())
            .map(|(label, _)| (*label, ScrollAnchor::for_handle(scroll.clone())))
            .collect();
        Self {
            expanded: [true, false, false, false, false, false],
            section: None,
            keyboard: false,
            scroll,
            anchors,
        }
    }
}

fn setting_row(label: &str, description: &str, control: impl IntoElement, p: Palette) -> Div {
    raw_setting_row(&tr(label), &tr(description), control, p)
}
fn raw_setting_row(label: &str, description: &str, control: impl IntoElement, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap_6()
        .py_4()
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_color(rgb(p.text)).child(label.to_owned()))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(description.to_owned()),
                ),
        )
        .child(div().flex_shrink_0().child(control))
}
fn heading(label: &str, p: Palette) -> Div {
    div()
        .mt_6()
        .pb_2()
        .text_sm()
        .text_color(rgb(p.muted))
        .border_b_1()
        .border_color(rgb(p.line))
        .child(tr(label))
}
fn shortcut_reference_row(label: &str, chord: &str, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_6()
        .py_4()
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(rgb(p.text))
                .child(tr(label)),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(p.muted))
                .child(chord.to_owned()),
        )
}
fn short_home_path(path: &str) -> String {
    if let Some(home) = std::env::var_os("HOME").and_then(|v| v.into_string().ok()) {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_owned()
}

impl AppView {
    /// Everything the settings page can toggle: installed agents, kinds the user
    /// pinned a path for, and Cursor when its usage login is authorized.
    fn setting_agent_kinds(&self) -> Vec<String> {
        let catalog = self
            .states
            .get(&Device::local().id)
            .map(|state| state.catalog.clone())
            .unwrap_or_default();
        let mut kinds = available_agent_kinds(&self.settings, &catalog);
        for (kind, path) in &self.settings.agent_binary_overrides {
            if !path.trim().is_empty()
                && kind != "terminal"
                && !kinds.iter().any(|value| value == kind)
            {
                kinds.push(kind.clone());
            }
        }
        if crate::usage::UsageService::cursor_authorized() && !kinds.iter().any(|k| k == "cursor") {
            kinds.push("cursor".into());
        }
        kinds
    }
    fn render_agent_setting_block(
        &mut self,
        kind: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        p: Palette,
    ) -> Div {
        let enabled = !self
            .settings
            .disabled_agent_kinds
            .contains(&kind.to_string());
        let expanded = self.expanded_agent.as_deref() == Some(kind);
        let detected = self
            .states
            .get(&Device::local().id)
            .and_then(|state| state.catalog_paths.get(kind).cloned());
        let override_path = self
            .settings
            .agent_binary_overrides
            .get(kind)
            .cloned()
            .unwrap_or_default();
        let status = if detected.is_some() {
            tr("命令可用")
        } else {
            tr("未找到命令")
        };
        let mut block = div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_6()
                    .py_3()
                    .hover(|d| d.bg(rgb(p.active)))
                    .child({
                        let toggle = UiAction::AgentToggle(kind.to_owned());
                        let key_action = toggle.clone();
                        let tooltip_label = tr(if enabled { "启用" } else { "停用" });
                        div()
                            .id(SharedString::from(format!("agent-enabled-{kind}")))
                            .tab_index(0)
                            .size(px(24.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(p.active)))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_key_down(cx.listener(
                                move |this, event: &KeyDownEvent, window, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        cx.stop_propagation();
                                        this.dispatch(key_action.clone(), window, cx);
                                    }
                                },
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.dispatch(toggle.clone(), window, cx);
                            }))
                            .focus(|d| d.rounded(px(4.)).border_1().border_color(rgb(p.accent)))
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(tooltip_label.clone())
                                    .build(window, cx)
                            })
                            .child(
                                div()
                                    .size(px(16.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(rgb(if enabled { p.accent } else { p.line }))
                                    .bg(rgb(if enabled { p.accent } else { p.bg }))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(enabled, |box_| {
                                        box_.child(crate::icons::icon("check", p.bg).size(px(12.)))
                                    }),
                            )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("agent-expand-{kind}")))
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .tab_index(0)
                            .focus(|d| d.bg(rgb(p.active)))
                            .on_key_down(cx.listener({
                                let kind = kind.to_owned();
                                move |this, event: &KeyDownEvent, window, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        cx.stop_propagation();
                                        this.dispatch(
                                            UiAction::AgentExpand(kind.clone()),
                                            window,
                                            cx,
                                        );
                                    }
                                }
                            }))
                            .on_click(cx.listener({
                                let kind = kind.to_owned();
                                move |this, _, window, cx| {
                                    this.dispatch(UiAction::AgentExpand(kind.clone()), window, cx);
                                }
                            }))
                            .child(crate::icons::agent_icon(kind, p.text))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(rgb(p.text))
                                            .child(agent_display_name(kind)),
                                    )
                                    .child(div().text_sm().text_color(rgb(p.muted)).child(status)),
                            )
                            .child(crate::icons::icon(
                                if expanded {
                                    "chevron_down"
                                } else {
                                    "chevron_right"
                                },
                                p.muted,
                            )),
                    ),
            );
        if expanded {
            let hint = agent_command_hint(kind);
            let input = self.input(
                &format!("agent-path:{kind}"),
                override_path,
                &hint,
                window,
                cx,
            );
            let mut details = div()
                .px_6()
                .pb_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr("CLI 命令")),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(p.muted))
                                .child(tr("命令名或可执行文件路径")),
                        )
                        .child(input)
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(p.muted))
                                .child(tr("留空则自动检测。应用或按 Return 保存。")),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(self.btn(
                                    format!("agent-apply-{kind}"),
                                    "应用路径",
                                    UiAction::AgentApplyPath(kind.to_owned()),
                                    p,
                                    cx,
                                ))
                                .child(self.btn(
                                    format!("agent-up-{kind}"),
                                    "上移",
                                    UiAction::AgentMove(kind.to_owned(), -1),
                                    p,
                                    cx,
                                ))
                                .child(self.btn(
                                    format!("agent-down-{kind}"),
                                    "下移",
                                    UiAction::AgentMove(kind.to_owned(), 1),
                                    p,
                                    cx,
                                )),
                        )
                        .children(detected.as_ref().map(|path| {
                            div()
                                .text_sm()
                                .font_family("Menlo")
                                .text_color(rgb(p.muted))
                                .child(short_home_path(path))
                        })),
                )
                .child(div().h(px(1.)).bg(rgb(p.line)))
                .child(
                    div().flex().flex_col().gap_2().child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("用量账户")),
                    ),
                );
            if kind == "cursor" {
                details = details.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(self.cursor_app_status()),
                );
                details = details.child(self.render_cursor_account(false, p, cx));
            } else if crate::usage::PROVIDERS.contains(&kind) {
                details = details.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("用量跟随此 Agent 现有的本机登录。当前结果见用量菜单。")),
                );
            } else {
                details = details.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("该 Agent 暂不支持用量查询，命令仍可独立使用。")),
                );
            }
            if !enabled {
                details = details.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("Agent 已禁用 · 不会占用快捷键，也不会后台查询用量。")),
                );
            }
            block = block.child(details);
        }
        block
    }
    pub(super) fn save_agent_shortcut_field(
        &mut self,
        kind: &str,
        restore: bool,
        cx: &mut Context<Self>,
    ) {
        let key = format!("agent-shortcut:{kind}");
        let chord = if restore {
            String::new()
        } else {
            self.fields
                .get(&key)
                .map(|input| input.read(cx).text().trim().to_owned())
                .unwrap_or_default()
        };
        match self.set_agent_shortcut(kind, &chord, cx) {
            Ok(()) => {
                self.fields.remove(&key);
                self.error = None;
                self.notice = Some(tr(if restore {
                    "已还原 Agent 快捷键"
                } else {
                    "Agent 快捷键已保存"
                }));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
    pub(super) fn save_shortcut_field(&mut self, id: &str, cx: &mut Context<Self>) {
        let key = format!("shortcut:{id}");
        let Some(input) = self.fields.get(&key) else {
            return;
        };
        let chord = input.read(cx).text().trim().to_owned();
        let previous = self.settings.shortcuts.clone();
        let mut next = previous.clone();
        if !next.is_object() {
            next = json!({});
        }
        next[id] = json!(chord);
        let result = Self::validate_shortcuts(&next).and_then(|_| {
            self.settings.shortcuts = next;
            self.apply_shortcuts(cx)?;
            self.settings.save()
        });
        match result {
            Ok(()) => {
                self.error = None;
                self.notice = Some(tr("快捷键已保存"));
            }
            Err(error) => {
                self.settings.shortcuts = previous;
                let rollback = self.apply_shortcuts(cx);
                self.error = Some(match rollback {
                    Ok(()) => error.to_string(),
                    Err(rollback) => format!("{error}; {}: {rollback}", tr("恢复快捷键失败")),
                });
            }
        }
        cx.notify();
    }
    fn restore_shortcut_fields(&mut self, cx: &mut Context<Self>) {
        match self.restore_shortcuts(cx) {
            Ok(()) => {
                self.fields.retain(|key, _| {
                    !key.starts_with("shortcut:") && !key.starts_with("agent-shortcut:")
                });
                self.error = None;
                self.notice = Some(tr("已还原默认快捷键"));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
    fn shortcut_input(
        &mut self,
        key: &str,
        value: String,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let input = self.input(key, value, placeholder, window, cx);
        input.update(cx, |input, _| input.set_shortcut_capture());
        input
    }
    fn setting_field(
        &mut self,
        key: &'static str,
        label: &str,
        description: &str,
        value: String,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        p: Palette,
    ) -> Div {
        let input = self.input(key, value, &tr(placeholder), window, cx);
        setting_row(
            label,
            description,
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().w(px(190.)).child(input))
                .child(self.btn(
                    format!("save-{key}"),
                    "保存",
                    UiAction::SaveSetting(key),
                    p,
                    cx,
                )),
            p,
        )
    }
    fn setting_toggle(
        &self,
        key: &'static str,
        label: &str,
        description: &str,
        enabled: bool,
        cx: &mut Context<Self>,
        p: Palette,
    ) -> Div {
        setting_row(
            label,
            description,
            self.btn(format!("toggle-{key}"), "", UiAction::Toggle(key), p, cx)
                .flex()
                .items_center()
                .gap_2()
                .p_0()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr(if enabled { "已开启" } else { "已关闭" })),
                )
                .child(
                    div()
                        .w(px(34.))
                        .h(px(20.))
                        .px(px(3.))
                        .flex()
                        .items_center()
                        .when(enabled, |d| d.justify_end())
                        .rounded_full()
                        .bg(rgb(if enabled { p.accent } else { p.line }))
                        .child(div().size(px(14.)).rounded_full().bg(rgb(p.bg))),
                ),
            p,
        )
    }
    fn select_settings_section(
        &mut self,
        page: SettingsPage,
        section: Option<&'static str>,
        navigation: &Entity<SettingsNavigation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_page != page {
            self.dispatch(UiAction::SettingsPage(page), window, cx);
            if page == SettingsPage::Notifications {
                crate::notifications::query_permission();
            }
        }
        let (anchor, scroll) = navigation.update(cx, |state, _| {
            state.section = section;
            if let Some(index) = SETTINGS_CATEGORIES
                .iter()
                .position(|category| category.page == page)
            {
                state.expanded[index] = true;
            }
            (
                section.and_then(|section| state.anchors.get(section).cloned()),
                state.scroll.clone(),
            )
        });
        // Wait for the newly selected page to lay out before reading its anchor origin.
        window.on_next_frame(move |window, cx| {
            if let Some(anchor) = anchor {
                anchor.scroll_to(window, cx);
            } else {
                scroll.set_offset(point(px(0.), px(0.)));
            }
            window.refresh();
        });
        cx.notify();
    }
    fn settings_navigation_row(
        &self,
        id: String,
        page: SettingsPage,
        section: Option<&'static str>,
        navigation: &Entity<SettingsNavigation>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.settings_page == page && navigation.read(cx).section == section;
        let click_state = navigation.clone();
        let key_state = navigation.clone();
        let selected_bg = rgb(p.side).blend(Rgba {
            a: 0.5,
            ..rgb(p.active)
        });
        let hover_bg = rgb(p.side).blend(Rgba {
            a: 0.25,
            ..rgb(p.active)
        });
        div()
            .id(SharedString::from(id))
            .tab_index(0)
            .relative()
            .w_full()
            .min_h(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(if selected { rgb(p.line) } else { rgba(0) })
            .bg(if selected { selected_bg } else { rgb(p.side) })
            .text_size(px(13.))
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(if selected { p.text } else { p.muted }))
            .cursor_pointer()
            .hover(move |style| style.bg(if selected { selected_bg } else { hover_bg }))
            .active(move |style| style.bg(rgb(p.active)))
            .when(navigation.read(cx).keyboard, |row| {
                row.focus(move |style| style.border_color(rgb(p.accent)))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_settings_section(page, section, &click_state, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.select_settings_section(page, section, &key_state, window, cx);
                }
            }))
    }
    pub(super) fn render_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        p: Palette,
    ) -> AnyElement {
        let navigation = window.use_state(cx, |_, _| SettingsNavigation::default());
        let search = self.input("settings-search", String::new(), "搜索设置…", window, cx);
        search.update(cx, |input, _| input.set_search());
        let query = search.read(cx).text().trim().to_lowercase();
        navigation.update(cx, |state, _| {
            if state.section.is_some_and(|section| {
                !SETTINGS_CATEGORIES.iter().any(|category| {
                    category.page == self.settings_page
                        && category.sections.iter().any(|(label, _)| *label == section)
                })
            }) {
                state.section = None;
            }
        });
        let agent_keywords = self
            .setting_agent_kinds()
            .iter()
            .map(|kind| format!(" {kind} {}", agent_display_name(kind)))
            .collect::<String>();
        let matches: Vec<_> = SETTINGS_CATEGORIES
            .iter()
            .enumerate()
            .filter(|(_, category)| category.matches(&query, &agent_keywords))
            .collect();
        if !query.is_empty() {
            if !matches
                .iter()
                .any(|(_, category)| category.page == self.settings_page)
            {
                if let Some((_, category)) = matches.first() {
                    self.select_settings_section(category.page, None, &navigation, window, cx);
                }
            }
            let visible = matches
                .iter()
                .find(|(_, category)| category.page == self.settings_page)
                .map(|(_, category)| category.matching_sections(&query, &agent_keywords))
                .unwrap_or_default();
            navigation.update(cx, |state, _| {
                if state
                    .section
                    .is_some_and(|section| !visible.contains(&section))
                {
                    state.section = None;
                }
            });
        }
        let scroll = navigation.read(cx).scroll.clone();
        let anchors = navigation.read(cx).anchors.clone();
        let heading = |label: &str, p| {
            heading(label, p)
                .id(SharedString::from(format!("settings-heading-{label}")))
                .anchor_scroll(anchors.get(label).cloned())
        };
        let mut nav = div()
            .id("settings-navigation")
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .px_3()
            .pb_3()
            .overflow_y_scroll()
            .bg(rgb(p.side))
            .border_r_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .h(px(36.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    }),
            )
            .child(div().mb_3().child(search));
        for (index, category) in &matches {
            let index = *index;
            let page = category.page;
            let open = !query.is_empty() || navigation.read(cx).expanded[index];
            let mut row = self.settings_navigation_row(
                format!("settings-category-{index}"),
                page,
                None,
                &navigation,
                p,
                cx,
            );
            if !category.sections.is_empty() {
                let click_state = navigation.clone();
                let key_state = navigation.clone();
                row = row.child(
                    div()
                        .id(SharedString::from(format!("settings-expand-{index}")))
                        .tab_index(0)
                        .size(px(26.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .when(navigation.read(cx).keyboard, |row| {
                            row.focus(|style| style.bg(rgb(p.active)))
                        })
                        .child(crate::icons::icon(
                            if open {
                                "chevron_down"
                            } else {
                                "chevron_right"
                            },
                            p.muted,
                        ))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            click_state.update(cx, |state, _| {
                                state.expanded[index] = !state.expanded[index];
                                if !state.expanded[index]
                                    && SETTINGS_CATEGORIES[index]
                                        .sections
                                        .iter()
                                        .any(|(label, _)| Some(*label) == state.section)
                                {
                                    state.section = None;
                                }
                            });
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(move |_, event: &KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                key_state.update(cx, |state, _| {
                                    state.expanded[index] = !state.expanded[index];
                                    if !state.expanded[index]
                                        && SETTINGS_CATEGORIES[index]
                                            .sections
                                            .iter()
                                            .any(|(label, _)| Some(*label) == state.section)
                                    {
                                        state.section = None;
                                    }
                                });
                                cx.notify();
                            }
                        })),
                );
            } else {
                row = row.child(div().w(px(26.)).flex_shrink_0());
            }
            nav = nav.child(
                row.child(
                    div()
                        .pl(px(5.))
                        .flex_1()
                        .min_w_0()
                        .child(tr(category.label)),
                ),
            );
            if open {
                for section in category.matching_sections(&query, &agent_keywords) {
                    nav = nav.child(
                        self.settings_navigation_row(
                            format!("settings-section-{index}-{section}"),
                            page,
                            Some(section),
                            &navigation,
                            p,
                            cx,
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(13.))
                                .top(px(-1.))
                                .bottom(px(-1.))
                                .w(px(1.))
                                .bg(rgb(p.line)),
                        )
                        .child(div().pl(px(31.)).flex_1().min_w_0().child(tr(section))),
                    );
                }
            }
        }
        if matches.is_empty() {
            nav = nav.child(
                div()
                    .px_2()
                    .py_3()
                    .text_sm()
                    .text_color(rgb(p.muted))
                    .child(tr("没有匹配的设置")),
            );
        }
        let title = SETTINGS_CATEGORIES
            .iter()
            .find(|category| category.page == self.settings_page)
            .map(|category| category.label)
            .unwrap_or("设置");
        let mut content = div()
            .w_full()
            .max_w(px(1040.))
            .px_8()
            .pt(px(36.))
            .pb_8()
            .text_color(rgb(p.text))
            .child(div().text_xl().child(tr(title)));
        match self.settings_page {
            SettingsPage::Appearance => {
                let mut themes = div().flex().p(px(2.)).rounded(px(5.)).bg(rgb(p.side));
                for (key, label) in [("system", "跟随系统"), ("light", "浅色"), ("dark", "深色")]
                {
                    themes = themes.child(
                        self.btn(format!("theme-{key}"), label, UiAction::Theme(key), p, cx)
                            .bg(rgb(if self.settings.theme == key {
                                p.active
                            } else {
                                p.side
                            })),
                    );
                }
                content = content.child(heading("界面", p)).child(setting_row(
                    "主题",
                    "跟随系统，或始终使用浅色、深色外观。",
                    themes,
                    p,
                ));
                let mut languages = div().flex().p(px(2.)).rounded(px(5.)).bg(rgb(p.side));
                for (key, label) in [
                    ("system", "跟随系统"),
                    ("en", "English"),
                    ("zh-Hans", "简体中文"),
                ] {
                    let selected = self.settings.language == key
                        || (key == "system"
                            && (self.settings.language.is_empty()
                                || self.settings.language == "system"));
                    languages = languages.child(
                        self.btn(
                            format!("language-{key}"),
                            label,
                            UiAction::Language(key),
                            p,
                            cx,
                        )
                        .bg(rgb(if selected { p.active } else { p.side })),
                    );
                }
                content = content.child(setting_row(
                    "语言",
                    "跟随系统，或始终使用英语、简体中文。",
                    languages,
                    p,
                ));
                content = content
                    .child(heading("侧栏", p))
                    .child(self.setting_toggle(
                        "spaces",
                        "隐藏空间列表",
                        "仅改变侧栏显示，不删除空间或结束会话。",
                        self.settings.spaces_hidden,
                        cx,
                        p,
                    ))
                    .child(self.setting_toggle(
                        "priority",
                        "优先显示会话",
                        "优先关注正在进行的会话。",
                        self.settings.priority_sessions,
                        cx,
                        p,
                    ));
            }
            SettingsPage::Terminal => {
                content = content.child(heading("字体", p));
                for (key, label, description, value, placeholder) in [
                    (
                        "font-name",
                        "字体",
                        "留空使用系统等宽字体。",
                        self.settings.font_name.clone(),
                        "系统等宽字体",
                    ),
                    (
                        "font-size",
                        "字号",
                        "允许范围 9–22。",
                        self.settings.font_size.to_string(),
                        "12.5",
                    ),
                    (
                        "font-weight",
                        "字重",
                        "允许范围 −1–1；0 为常规，0.4 为粗体。",
                        self.settings.font_weight.to_string(),
                        "0",
                    ),
                    (
                        "line-spacing",
                        "行距",
                        "行高倍率，允许范围 1–1.4。",
                        self.settings.line_spacing.to_string(),
                        "1",
                    ),
                ] {
                    content = content.child(self.setting_field(
                        key,
                        label,
                        description,
                        value,
                        placeholder,
                        window,
                        cx,
                        p,
                    ));
                }
                content = content.child(heading("布局", p));
                for (key, label, description, value, placeholder) in [
                    (
                        "padding-x",
                        "左右内边距",
                        "终端文字与左右边缘的距离，单位为逻辑像素，范围 0–64。",
                        self.settings.terminal_padding_x.to_string(),
                        "8",
                    ),
                    (
                        "padding-y",
                        "上下内边距",
                        "终端文字与上下边缘的距离，单位为逻辑像素，范围 0–64。",
                        self.settings.terminal_padding_y.to_string(),
                        "8",
                    ),
                    (
                        "terminal-margin",
                        "终端外边距",
                        "终端区域与分栏边框之间的留白，范围 0–64。",
                        self.settings.terminal_margin.to_string(),
                        "0",
                    ),
                    (
                        "unfocused-split-opacity",
                        "非活动分栏不透明度",
                        "范围 0.15–1；1 表示不淡化，只影响非活动分栏。",
                        self.settings.unfocused_split_opacity.to_string(),
                        "1",
                    ),
                ] {
                    content = content.child(self.setting_field(
                        key,
                        label,
                        description,
                        value,
                        placeholder,
                        window,
                        cx,
                        p,
                    ));
                }
                content = content
                    .child(self.setting_toggle(
                        "padding-balance",
                        "均分网格余量",
                        "将不足一格的剩余空间均分到两侧，不改变设置的基础内边距。",
                        self.settings.terminal_padding_balance,
                        cx,
                        p,
                    ))
                    .child(heading("交互", p))
                    .child(self.setting_toggle(
                        "copy-on-select",
                        "选中即复制",
                        "选中文本后写入系统剪贴板；默认关闭。",
                        self.settings.copy_on_select,
                        cx,
                        p,
                    ))
                    .child(self.setting_toggle(
                        "mouse",
                        "鼠标报告",
                        "向支持鼠标的终端程序发送鼠标事件。",
                        self.settings.mouse_reporting,
                        cx,
                        p,
                    ));
            }
            SettingsPage::Agents => {
                content = content
                    .child(heading("启动行为", p))
                    .child(self.setting_toggle(
                        "bypass",
                        "默认跳过 Agent 权限确认",
                        "仅在你信任的工作目录中启用；Agent 可能无需再次询问就修改文件或执行命令。",
                        self.settings.agent_bypass_default,
                        cx,
                        p,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(heading("可用 Agents", p))
                            .child(self.btn(
                                "check-agents",
                                "检查命令",
                                UiAction::AgentCatalog,
                                p,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .mt_2()
                            .px_6()
                            .text_sm()
                            .text_color(rgb(p.muted))
                            .child(tr("只有已启用的 Agent 会出现在快捷键中。")),
                    );
                let kinds = self.setting_agent_kinds();
                for kind in kinds {
                    content = content.child(self.render_agent_setting_block(&kind, window, cx, p));
                }
            }
            SettingsPage::Shortcuts => {
                content = content
                    .child(heading("快速新建 Agent", p))
                    .child(div().mt_3().text_sm().text_color(rgb(p.muted)).child(tr(
                        "点击输入框后直接按下组合键即可绑定；也可输入 cmd-shift-t 后保存。冲突不会覆盖原配置。",
                    )));
                let shortcut_catalog = self
                    .states
                    .get(&Device::local().id)
                    .map(|state| state.catalog.clone())
                    .unwrap_or_default();
                for kind in enabled_agent_kinds(&self.settings, &shortcut_catalog) {
                    let value =
                        super::app_shortcuts::agent_shortcut_text(&kind).unwrap_or_default();
                    let default = if kind == "pi" { "alt-p" } else { "未绑定" };
                    let input = self.shortcut_input(
                        &format!("agent-shortcut:{kind}"),
                        value,
                        &tr(default),
                        window,
                        cx,
                    );
                    let mut controls = div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().w(px(190.)).child(input));
                    for (restore, label) in [(false, "保存"), (true, "还原")] {
                        let click_kind = kind.clone();
                        let key_kind = kind.clone();
                        let button = div()
                            .id(SharedString::from(format!(
                                "agent-shortcut-{kind}-{restore}"
                            )))
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(p.raised))
                            .border_1()
                            .border_color(rgb(p.line))
                            .cursor_pointer()
                            .tab_index(0)
                            .focus(|s| s.border_color(rgb(p.accent)))
                            .hover(|s| s.bg(rgb(p.active)))
                            .child(tr(label))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" || event.keystroke.key == "space"
                                {
                                    this.save_agent_shortcut_field(&key_kind, restore, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.save_agent_shortcut_field(&click_kind, restore, cx)
                            }));
                        controls = controls.child(button);
                    }
                    content = content.child(raw_setting_row(
                        &kind,
                        &tr("默认：{value}；还原仅影响此 Agent。").replace("{value}", &tr(default)),
                        controls,
                        p,
                    ));
                }
                let reset = div()
                    .id("restore-shortcuts")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(p.raised))
                    .border_1()
                    .border_color(rgb(p.line))
                    .cursor_pointer()
                    .tab_index(0)
                    .focus(|s| s.border_color(rgb(p.accent)))
                    .hover(|s| s.bg(rgb(p.active)))
                    .child(tr("还原默认快捷键"))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            this.restore_shortcut_fields(cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.restore_shortcut_fields(cx)));
                content = content.child(div().mt_4().child(reset));
                content = content.child(heading("键盘快捷键", p)).child(
                    div()
                        .mt_3()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("以下为工作台固定快捷键，不能在此修改。")),
                );
                let mut shown_advanced = false;
                for &(id, label, default) in super::app_shortcuts::SHORTCUTS {
                    if Self::is_advanced_shortcut(id) && !shown_advanced {
                        shown_advanced = true;
                        content = content.child(heading("高级", p));
                    }
                    let chord = super::app_shortcuts::shortcut_text(&self.settings.shortcuts, id)
                        .unwrap_or_else(|_| super::app_shortcuts::platform_default(default));
                    content = content.child(shortcut_reference_row(label, &chord, p));
                }
            }
            SettingsPage::Notifications => {
                content = content
                    .child(heading("提醒", p))
                    .child(self.setting_toggle(
                        "notifications",
                        "桌面通知",
                        "Agent 完成任务或等待处理时发送提醒。",
                        self.settings.notifications_enabled,
                        cx,
                        p,
                    ))
                    .child(self.setting_toggle(
                        "sound",
                        "提示音",
                        "通知开启时播放提示音。",
                        self.settings.notifications_sound,
                        cx,
                        p,
                    ));
                let permission_state = crate::notifications::permission();
                let permission = match permission_state {
                    crate::notifications::Permission::Unknown => "尚未读取授权状态",
                    crate::notifications::Permission::NotDetermined => "尚未请求通知权限",
                    crate::notifications::Permission::Denied => "系统已拒绝通知权限",
                    crate::notifications::Permission::Authorized => "系统已允许通知",
                    crate::notifications::Permission::Provisional => "已获得临时通知授权",
                };
                let request = div()
                    .id("notification-permission")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(p.raised))
                    .border_1()
                    .border_color(rgb(p.line))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(p.active)))
                    .child(tr("请求权限"))
                    .tab_index(0)
                    .focus(|s| s.border_color(rgb(p.accent)))
                    .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            crate::notifications::request_permission();
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(|_, _, _, cx| {
                        crate::notifications::request_permission();
                        cx.notify();
                    }));
                let system = div()
                    .id("notification-system-settings")
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(p.raised))
                    .border_1()
                    .border_color(rgb(p.line))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(p.active)))
                    .child(tr("系统设置…"))
                    .tab_index(0).focus(|s|s.border_color(rgb(p.accent)))
                    .on_key_down(cx.listener(|_,event:&KeyDownEvent,_,cx|{
                        if event.keystroke.key=="enter" || event.keystroke.key=="space" {
                            cx.open_url("x-apple.systempreferences:com.apple.Notifications-Settings.extension");cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.open_url(
                            "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
                        );
                    }));
                content = content
                    .child(heading("系统权限", p))
                    .child(setting_row(
                        "通知授权",
                        permission,
                        match permission_state {
                            crate::notifications::Permission::Unknown
                            | crate::notifications::Permission::NotDetermined => request,
                            crate::notifications::Permission::Denied
                            | crate::notifications::Permission::Authorized
                            | crate::notifications::Permission::Provisional => system,
                        },
                        p,
                    ))
                    .when(
                        permission_state == crate::notifications::Permission::Denied,
                        |content| {
                            content.child(div().mt_3().text_sm().text_color(rgb(p.muted)).child(tr(
                            "若权限被拒绝，请在系统设置中允许通知。应用内开关不会绕过系统授权。",
                        )))
                        },
                    );
            }
        }
        if matches.is_empty() {
            content = div()
                .px_8()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .text_color(rgb(p.muted))
                .child(crate::icons::illustration("search", 128.))
                .child(tr("没有匹配的设置"));
        }
        div()
            .id("settings-page")
            .capture_any_mouse_down({
                let navigation = navigation.clone();
                cx.listener(move |_, _, _, cx| {
                    navigation.update(cx, |state, _| state.keyboard = false);
                    cx.notify();
                })
            })
            .capture_key_down({
                let navigation = navigation.clone();
                cx.listener(move |_, _, _, cx| {
                    navigation.update(cx, |state, _| state.keyboard = true);
                    cx.notify();
                })
            })
            .tab_group()
            .flex()
            .size_full()
            .bg(rgb(p.bg))
            .child(nav)
            .child(
                div()
                    .id("settings-content")
                    .track_scroll(&scroll)
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .child(content),
            )
            .into_any_element()
    }
}

// ponytail: search indexes categories and sections; index individual rows when per-setting results are needed.
fn settings_match(query: &str, label: &str, keywords: &str) -> bool {
    query.split_whitespace().all(|word| {
        format!("{} {} {}", label, tr(label), keywords)
            .to_lowercase()
            .contains(word)
    })
}

#[cfg(test)]
mod settings_search_tests {
    use super::{SETTINGS_CATEGORIES, SettingsPage, settings_match};
    #[test]
    fn navigation_sections_match_real_pages_and_search() {
        let terminal = SETTINGS_CATEGORIES
            .iter()
            .find(|category| category.page == SettingsPage::Terminal)
            .unwrap();
        assert_eq!(terminal.matching_sections("font", ""), vec!["字体"]);
        assert_eq!(
            terminal.matching_sections("terminal mouse", ""),
            vec!["交互"]
        );
        assert_eq!(terminal.matching_sections("终端", "").len(), 3);
        assert!(!terminal.matches("sound", ""));
        assert!(!terminal.matching_sections("font", "").contains(&"交互"));
        let agents = SETTINGS_CATEGORIES
            .iter()
            .find(|category| category.page == SettingsPage::Agents)
            .unwrap();
        assert_eq!(
            agents.matching_sections("custom-cli", " custom-cli"),
            vec!["可用 Agents"]
        );
        let shortcuts = SETTINGS_CATEGORIES
            .iter()
            .find(|category| category.page == SettingsPage::Shortcuts)
            .unwrap();
        assert_eq!(shortcuts.matching_sections("焦点左移", ""), vec!["高级"]);
        assert_eq!(shortcuts.matching_sections("cmd-d", ""), vec!["键盘快捷键"]);
        assert_eq!(
            shortcuts
                .sections
                .iter()
                .map(|(label, _)| *label)
                .collect::<Vec<_>>(),
            vec!["快速新建 Agent", "键盘快捷键", "高级"]
        );
        let labels: Vec<_> = SETTINGS_CATEGORIES
            .iter()
            .flat_map(|category| category.sections.iter().map(|(label, _)| label))
            .collect();
        assert_eq!(labels.len(), 12);
        assert!(!labels.contains(&&"刷新策略"));
        assert!(!terminal.matches("细笔画", ""));
        assert!(!terminal.matches("thin strokes", ""));
        assert!(!agents.matches("refresh", ""));
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }
    #[test]
    fn settings_search_matches_all_terms() {
        assert!(settings_match("", "终端", "font mouse"));
        assert!(settings_match("font mouse", "终端", "font mouse"));
        assert!(settings_match("终端", "终端", "font mouse"));
        assert!(!settings_match("font sound", "终端", "font mouse"));
    }
}
