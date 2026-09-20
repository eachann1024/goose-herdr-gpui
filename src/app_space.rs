use super::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _,
    input::{Input, InputEvent as ComponentInputEvent, InputState},
};

pub(super) struct SpaceForm {
    pub input: Entity<InputState>,
    pub scope: FocusHandle,
    pub previous_focus: Option<FocusHandle>,
    pub error: Option<String>,
    pub index: usize,
    pub scroll: ScrollHandle,
}

fn space_picker_label(workspace: &herdr::Workspace) -> String {
    let label = workspace.label.trim();
    if !label.is_empty() {
        return label.to_owned();
    }
    workspace
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .and_then(|cwd| {
            std::path::Path::new(cwd)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| workspace.workspace_id.clone())
}

#[derive(Clone, Debug, PartialEq)]
struct SpacePickerEntry {
    id: String,
    label: String,
    history: bool,
    folder: Option<String>,
}

fn is_retained(workspace: &herdr::Workspace) -> bool {
    workspace.extra.get("retained") == Some(&Value::Bool(true))
}

fn folder_key(path: &str) -> String {
    path.trim().trim_end_matches('/').to_owned()
}

fn space_picker_entries(
    workspaces: &[herdr::Workspace],
    recents: &[(String, String)],
    query: &str,
) -> Vec<SpacePickerEntry> {
    let query = query.trim().to_lowercase();
    let matches_query = |label: &str, cwd: &str, id: &str| {
        query.is_empty()
            || label.to_lowercase().contains(&query)
            || cwd.to_lowercase().contains(&query)
            || id.to_lowercase().contains(&query)
    };
    let mut live = Vec::new();
    let mut live_folders = std::collections::HashSet::new();
    let mut live_ids = std::collections::HashSet::new();
    for workspace in workspaces
        .iter()
        .filter(|workspace| !is_retained(workspace))
    {
        let label = space_picker_label(workspace);
        let cwd = workspace.cwd.as_deref().unwrap_or("");
        if !matches_query(&label, cwd, &workspace.workspace_id) {
            continue;
        }
        live_ids.insert(workspace.workspace_id.clone());
        let key = folder_key(cwd);
        if !key.is_empty() {
            live_folders.insert(key);
        }
        live.push(SpacePickerEntry {
            id: workspace.workspace_id.clone(),
            label,
            history: false,
            folder: workspace.cwd.clone(),
        });
    }
    let mut history = Vec::new();
    let mut seen_folders = live_folders.clone();
    for workspace in workspaces.iter().filter(|workspace| is_retained(workspace)) {
        if live_ids.contains(&workspace.workspace_id) {
            continue;
        }
        let cwd = workspace.cwd.as_deref().unwrap_or("");
        let key = folder_key(cwd);
        if !key.is_empty() && seen_folders.contains(&key) {
            continue;
        }
        let label = space_picker_label(workspace);
        if !matches_query(&label, cwd, &workspace.workspace_id) {
            continue;
        }
        if !key.is_empty() {
            seen_folders.insert(key);
        }
        history.push(SpacePickerEntry {
            id: workspace.workspace_id.clone(),
            label,
            history: true,
            folder: workspace.cwd.clone(),
        });
    }
    for (cwd, label) in recents {
        let key = folder_key(cwd);
        if key.is_empty() || seen_folders.contains(&key) {
            continue;
        }
        if !matches_query(label, cwd, "") {
            continue;
        }
        seen_folders.insert(key.clone());
        history.push(SpacePickerEntry {
            id: format!("recent:{key}"),
            label: label.clone(),
            history: true,
            folder: Some(cwd.clone()),
        });
    }
    live.extend(history);
    live
}

impl AppView {
    fn space_picker_matches(&self, query: &str) -> Vec<SpacePickerEntry> {
        let recents = app_persistence::recent_folders(&self.device().id).unwrap_or_default();
        self.snapshot()
            .map(|snapshot| space_picker_entries(&snapshot.workspaces, &recents, query))
            .unwrap_or_else(|| space_picker_entries(&[], &recents, query))
    }

    pub(crate) fn render_space_picker(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.form.as_ref()?;
        let state = form.space_form.as_ref()?;
        let p = self.palette(window);
        let device = self.device();
        let query = state.input.read(cx).value().to_string();
        let matches = self.space_picker_matches(&query);
        let selected = if matches.is_empty() {
            0
        } else {
            state.index.min(matches.len() - 1)
        };
        let mut list = div()
            .id("space-picker-list")
            .w_full()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .flex()
            .flex_col();
        if matches.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .h(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(tr("没有匹配的项目")),
            );
        }
        let mut matches_were_history = false;
        for (index, entry) in matches.into_iter().enumerate() {
            if index > 0 && entry.history && !matches_were_history {
                list = list.child(
                    div()
                        .mx_1()
                        .my(px(6.))
                        .h(px(1.))
                        .flex_shrink_0()
                        .bg(rgb(p.line)),
                );
            }
            let selected = index == selected;
            let history = entry.history;
            let open = entry.clone();
            let row_id = entry.id.clone();
            let label = entry.label.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("space-picker-row-{row_id}")))
                    .w_full()
                    .h(px(28.))
                    .flex_shrink_0()
                    .px_2()
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(rgb(p.active)))
                    .hover(|d| d.bg(rgb(p.active)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_space_picker_entry(&open, window, cx);
                    }))
                    .child(self.project_icon(&device, &row_id, 16., p.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_size(px(13.))
                            .text_color(rgb(p.text))
                            .id(SharedString::from(format!("space-picker-label-{row_id}")))
                            .child(fade_label(label)),
                    ),
            );
            matches_were_history = history;
        }
        let mut picker = div()
            .id("space-picker")
            .key_context("SpaceModal")
            .w(px(480.))
            .max_w_full()
            .max_h_full()
            .p_1()
            .bg(rgb(p.raised))
            .border_1()
            .border_color(rgb(p.line))
            .rounded(px(8.))
            .shadow_md()
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &SubmitSpace, window, cx| {
                this.confirm_space_picker(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &OpenFolder, window, cx| {
                    this.pick_space_directory(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.close_overlay(window, cx)))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.close_overlay(window, cx)
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if !matches!(key, "up" | "down") {
                    return;
                }
                cx.stop_propagation();
                window.prevent_default();
                let query = this
                    .form
                    .as_ref()
                    .and_then(|form| form.space_form.as_ref())
                    .map(|state| state.input.read(cx).value().to_string())
                    .unwrap_or_default();
                let len = this.space_picker_matches(&query).len();
                let Some(state) = this.form.as_mut().and_then(|form| form.space_form.as_mut())
                else {
                    return;
                };
                if len == 0 {
                    return;
                }
                state.index = menu_step(state.index.min(len - 1), len, key == "down");
                state.scroll.scroll_to_item(state.index);
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .px_1()
                    .pt_1()
                    .pb_1()
                    .flex_shrink_0()
                    .child(Input::new(&state.input).small().appearance(false)),
            );
        if let Some(error) = &state.error {
            picker = picker.child(
                div()
                    .px_2()
                    .pb_1()
                    .text_size(px(12.))
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        picker = picker.child(list);
        if device.is_local() {
            picker = picker
                .child(
                    div()
                        .mx_1()
                        .my(px(6.))
                        .h(px(1.))
                        .flex_shrink_0()
                        .bg(rgb(p.line)),
                )
                .child({
                    let shortcut = gpui_component::kbd::Kbd::binding_for_action(
                        &OpenFolder,
                        Some("SpaceModal"),
                        window,
                    )
                    .map(|kbd| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(rgb(p.muted))
                            .child(kbd.appearance(false))
                    });
                    div()
                        .id("space-picker-open")
                        .w_full()
                        .h(px(28.))
                        .flex_shrink_0()
                        .px_2()
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .cursor_pointer()
                        .text_size(px(13.))
                        .text_color(rgb(p.text))
                        .hover(|d| d.bg(rgb(p.active)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_space_directory(window, cx);
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .flex_1()
                                .child(crate::icons::icon("plus", p.muted))
                                .child(div().flex_1().min_w_0().child(tr("打开文件夹…"))),
                        )
                        .children(shortcut)
                });
        }
        Some(self.centered_overlay(picker, cx))
    }

    fn confirm_space_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.form.as_ref().and_then(|form| form.space_form.as_ref()) else {
            return;
        };
        if state.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        let query = state.input.read(cx).value().to_string();
        let matches = self.space_picker_matches(&query);
        if !matches.is_empty() {
            let index = state.index.min(matches.len() - 1);
            let entry = matches[index].clone();
            self.open_space_picker_entry(&entry, window, cx);
            return;
        }
        self.submit_space_dialog(window, cx);
    }

    fn open_space_picker_entry(
        &mut self,
        entry: &SpacePickerEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(folder) = entry
            .folder
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            if entry.id.starts_with("recent:")
                || !self.snapshot().is_some_and(|snapshot| {
                    snapshot
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.workspace_id == entry.id)
                })
            {
                match self.create_empty_space(folder) {
                    Ok(id) => {
                        self.close_overlay(window, cx);
                        self.dispatch(UiAction::SelectWorkspace(id), window, cx);
                    }
                    Err(error) => {
                        if let Some(state) =
                            self.form.as_mut().and_then(|form| form.space_form.as_mut())
                        {
                            state.error = Some(error.to_string());
                        } else {
                            self.error = Some(error.to_string());
                        }
                        cx.notify();
                    }
                }
                return;
            }
        }
        self.close_overlay(window, cx);
        self.dispatch(UiAction::SelectWorkspace(entry.id.clone()), window, cx);
    }

    pub(super) fn show_space_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous_focus = window.focused(cx);
        self.close_overlay(window, cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("搜索项目…")));
        self.subscriptions.push(
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                ComponentInputEvent::Change => {
                    if let Some(state) =
                        this.form.as_mut().and_then(|form| form.space_form.as_mut())
                    {
                        state.error = None;
                        state.index = 0;
                        state.scroll.scroll_to_item(0);
                    }
                    window.refresh();
                    cx.notify();
                }
                ComponentInputEvent::PressEnter { .. } => {
                    this.confirm_space_picker(window, cx);
                }
                _ => {}
            }),
        );
        self.form = Some(Form {
            kind: FormKind::NewSpace,
            title: tr("新建空间"),
            message: None,
            submit: tr("创建空间"),
            fields: Vec::new(),
            workspace: None,
            agent_kind: None,
            device_form: None,
            space_form: Some(SpaceForm {
                input: input.clone(),
                scope: cx.focus_handle(),
                previous_focus,
                error: None,
                index: 0,
                scroll: ScrollHandle::new(),
            }),
            item_scope: None,
        });
        input.focus_handle(cx).focus(window);
        cx.notify();
    }

    pub(super) fn render_new_item_modal(
        &self,
        form: &Form,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let device = self.device();
        let spaces = self
            .snapshot()
            .map(|s| {
                s.workspaces
                    .iter()
                    .filter(|w| w.extra.get("retained") != Some(&Value::Bool(true)))
                    .map(|w| (w.workspace_id.clone(), space_picker_label(w)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let catalog = self
            .states
            .get(&device.id)
            .map(|s| s.catalog.as_slice())
            .unwrap_or_default();
        let kinds = std::iter::once("terminal".to_owned())
            .chain(enabled_agent_kinds(&self.settings, catalog));
        let selected_workspace = form.workspace.clone();
        let selected_kind = form.agent_kind.clone();
        let mut space_row = div().flex().flex_wrap().gap_2();
        if spaces.is_empty() {
            space_row = space_row.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr("没有可用空间，请先新建。")),
            );
        }
        for (id, label) in spaces {
            let selected = selected_workspace.as_deref() == Some(id.as_str());
            space_row = space_row.child(
                self.btn_raw(
                    format!("new-item-space-{id}"),
                    "",
                    UiAction::FormSelectWorkspace(id.clone()),
                    p,
                    cx,
                )
                .flex()
                .items_center()
                .gap_2()
                .when(selected, |d| {
                    d.bg(rgb(p.active)).border_1().border_color(rgb(p.accent))
                })
                .child(self.project_icon(&device, &id, 16., p.muted))
                .child(label),
            );
        }
        let mut kind_row = div().flex().flex_wrap().gap_2();
        for kind in kinds {
            let selected = selected_kind.as_deref() == Some(kind.as_str());
            let label = if kind == "terminal" {
                tr("终端")
            } else {
                kind.clone()
            };
            kind_row = kind_row.child(
                self.btn_raw(
                    format!("new-item-kind-{kind}"),
                    "",
                    UiAction::FormSelectKind(kind.clone()),
                    p,
                    cx,
                )
                .flex()
                .items_center()
                .gap_2()
                .when(selected, |d| {
                    d.bg(rgb(p.active)).border_1().border_color(rgb(p.accent))
                })
                .child(crate::icons::agent_icon(&kind, p.muted))
                .child(label),
            );
        }
        let submit = if form.submit.is_empty() {
            tr("新增")
        } else {
            form.submit.clone()
        };
        let can_submit = selected_workspace.is_some() && selected_kind.is_some();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("空间")),
                    )
                    .child(space_row),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr("Agent")),
                    )
                    .child(kind_row),
            );
        if let Some(message) = &form.message {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(tr(message)),
            );
        }
        let scope = form.item_scope.clone();
        div()
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x1a1c1f33))
            .pt(px(80.))
            .flex()
            .justify_center()
            .items_start()
            .key_context("NewItemModal")
            .when_some(scope.as_ref(), |d, scope| d.track_focus(scope))
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.dispatch(UiAction::Dismiss, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.dispatch(UiAction::Dismiss, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.dispatch(UiAction::Dismiss, window, cx);
                }),
            )
            .child(
                div()
                    .key_context("OverlayPopup")
                    .w(px(480.))
                    .max_w_full()
                    .bg(rgb(p.raised))
                    .border_1()
                    .border_color(rgb(p.line))
                    .rounded(px(8.))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .w_full()
                            .px_3()
                            .pt_2()
                            .pb_1()
                            .text_size(px(13.))
                            .text_color(rgb(p.muted))
                            .child(tr(&form.title)),
                    )
                    .child(
                        div()
                            .w_full()
                            .px_3()
                            .py_2()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(body),
                    )
                    .child(
                        div()
                            .w_full()
                            .border_t_1()
                            .border_color(rgb(p.line))
                            .p_2()
                            .flex()
                            .justify_end()
                            .gap_1()
                            .child(self.btn("form-cancel", "取消", UiAction::Dismiss, p, cx))
                            .child(
                                self.btn("form-submit", submit, UiAction::Submit, p, cx)
                                    .when(!can_submit, |d| d.opacity(0.45)),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn show_new_item_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlay(window, cx);
        let spaces = self
            .snapshot()
            .map(|s| {
                s.workspaces
                    .iter()
                    .filter(|workspace| workspace.extra.get("retained") != Some(&Value::Bool(true)))
                    .map(|w| w.workspace_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let workspace = self
            .workspace
            .clone()
            .filter(|id| spaces.iter().any(|space| space == id))
            .or_else(|| spaces.first().cloned());
        let catalog = self
            .states
            .get(&self.device().id)
            .map(|state| state.catalog.as_slice())
            .unwrap_or_default();
        let agent_kind = std::iter::once("terminal".to_owned())
            .chain(enabled_agent_kinds(&self.settings, catalog))
            .next();
        self.form = Some(Form {
            kind: FormKind::NewItem,
            title: tr("新增"),
            message: None,
            submit: tr("新增"),
            fields: Vec::new(),
            workspace,
            agent_kind,
            device_form: None,
            space_form: None,
            item_scope: Some(cx.focus_handle()),
        });
        if let Some(scope) = self.form.as_ref().and_then(|form| form.item_scope.clone()) {
            scope.focus(window);
        }
        cx.notify();
    }

    pub(super) fn submit_space_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.form.as_ref().and_then(|form| form.space_form.as_ref()) else {
            return;
        };
        if state.input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        self.submit(window, cx);
    }

    pub(super) fn pick_space_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.device().is_local() {
            return;
        }
        let Some(input) = self
            .form
            .as_ref()
            .and_then(|form| form.space_form.as_ref())
            .map(|state| state.input.clone())
        else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("选择").into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = picker.await;
            let _ = view.update_in(cx, |this, window, cx| {
                let Some(state) = this
                    .form
                    .as_mut()
                    .and_then(|form| form.space_form.as_mut())
                    .filter(|state| state.input == input)
                else {
                    return;
                };
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            input.update(cx, |input, cx| {
                                input.set_value(path.display().to_string(), window, cx)
                            });
                            this.submit_space_dialog(window, cx);
                            return;
                        }
                    }
                    Ok(Err(error)) => state.error = Some(error.to_string()),
                    Err(error) => state.error = Some(error.to_string()),
                    Ok(Ok(None)) => {}
                }
                window.refresh();
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::space_picker_label;
    use crate::herdr::Workspace;

    #[test]
    fn space_picker_label_falls_back_to_path_basename() {
        let labeled = Workspace {
            workspace_id: "id".into(),
            label: "  Studio  ".into(),
            cwd: Some("/Users/me/other".into()),
            ..Workspace::default()
        };
        assert_eq!(space_picker_label(&labeled), "Studio");
        let path_only = Workspace {
            workspace_id: "id".into(),
            cwd: Some("/Users/me/project/".into()),
            ..Workspace::default()
        };
        assert_eq!(space_picker_label(&path_only), "project");
        let empty = Workspace {
            workspace_id: "fallback".into(),
            ..Workspace::default()
        };
        assert_eq!(space_picker_label(&empty), "fallback");
    }

    #[test]
    fn space_picker_puts_history_below_live_and_keeps_previous_folders() {
        use super::{SpacePickerEntry, space_picker_entries};
        use serde_json::json;
        let live = Workspace {
            workspace_id: "live".into(),
            label: "pi-jev-route".into(),
            cwd: Some("/Users/me/pi-jev-route".into()),
            ..Workspace::default()
        };
        let ghost = Workspace {
            workspace_id: "ghost".into(),
            label: "pi-jev-route".into(),
            cwd: Some("/Users/me/pi-jev-route".into()),
            extra: [("retained".into(), json!(true))].into_iter().collect(),
            ..Workspace::default()
        };
        let work = Workspace {
            workspace_id: "work".into(),
            label: "Work".into(),
            cwd: Some("/Users/me/Work".into()),
            extra: [("retained".into(), json!(true))].into_iter().collect(),
            ..Workspace::default()
        };
        let recents = vec![
            ("/Users/me/pi-jev-route".into(), "pi-jev-route".into()),
            ("/Users/me/old-notes".into(), "old-notes".into()),
        ];
        let entries = space_picker_entries(&[live, ghost, work], &recents, "");
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.label.as_str(), entry.history))
                .collect::<Vec<_>>(),
            [("pi-jev-route", false), ("Work", true), ("old-notes", true),]
        );
        assert!(matches!(
            &entries[2],
            SpacePickerEntry {
                folder: Some(path),
                ..
            } if path == "/Users/me/old-notes"
        ));
    }
}
