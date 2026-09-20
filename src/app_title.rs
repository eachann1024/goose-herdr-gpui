use super::*;

#[derive(Clone, PartialEq, Eq)]
struct TitleTarget {
    device: String,
    pane: String,
    tab: String,
    generation: u64,
}

pub(super) struct TitleEdit {
    target: TitleTarget,
    editor: Entity<InlineEdit>,
    pending: Option<String>,
    _subscription: Subscription,
}

use crate::inline_edit::checked_name as checked_title;

pub(super) fn snapshot_session_title(snapshot: &Snapshot, pane_id: &str) -> String {
    let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id);
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id);
    let tab_id = pane
        .and_then(|p| p.tab_id.as_deref())
        .or_else(|| agent.map(|a| a.tab_id.as_str()));
    let tab = tab_id.and_then(|id| snapshot.tabs.iter().find(|tab| tab.tab_id == id));
    // An explicit label is authoritative, even when it equals the agent kind.
    if let Some(label) = tab
        .and_then(|t| t.custom_label.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return label.to_owned();
    }
    let kind = pane
        .and_then(|p| crate::icons::pane_kind(&p.extra, p.agent.as_deref()))
        .or_else(|| agent.and_then(|a| a.agent.clone()))
        .unwrap_or_else(|| tr("终端"));
    resolved_session_title(
        agent.and_then(|a| a.title.as_deref()),
        tab.map(|t| t.label.as_str()),
        agent
            .and_then(|a| a.terminal_title.as_deref())
            .or_else(|| pane.and_then(|p| p.terminal_title.as_deref())),
        agent.and_then(|a| a.name.as_deref()),
        &kind,
        agent
            .and_then(|a| a.cwd.as_deref())
            .or_else(|| pane.and_then(|p| p.cwd.as_deref())),
        pane_id,
    )
}

impl AppView {
    fn title_target(&self) -> Option<TitleTarget> {
        if self.screen != Screen::Workspace {
            return None;
        }
        let device = self.device().id;
        let state = self.states.get(&device)?;
        state.connection.as_ref()?;
        let snapshot = state.snapshot.as_ref()?;
        let pane = self.selected_pane.as_ref()?;
        let tab = snapshot
            .panes
            .iter()
            .find(|p| &p.pane_id == pane)
            .and_then(|p| p.tab_id.clone())
            .or_else(|| {
                snapshot
                    .agents
                    .iter()
                    .find(|a| &a.pane_id == pane)
                    .map(|a| a.tab_id.clone())
            })?;
        if tab.is_empty() || !snapshot.tabs.iter().any(|t| t.tab_id == tab) {
            return None;
        }
        Some(TitleTarget {
            device,
            pane: pane.clone(),
            tab,
            generation: state.generation,
        })
    }

    fn begin_title_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.title_target() else {
            return;
        };
        if let Some(edit) = &self.title_edit {
            if edit.target.device == target.device && edit.target.pane == target.pane {
                edit.editor.read(cx).focus(window, cx);
            } else {
                // ponytail: retain one draft; add per-session drafts only for simultaneous editing.
                self.title_focus.focus(window);
                self.error = Some(tr(
                    "另一会话有名称草稿；返回继续编辑，或关闭提示后按 Escape 取消草稿",
                ));
            }
            cx.notify();
            return;
        }
        let original = self.session_title();
        let editor = cx.new(|cx| InlineEdit::new(&original, tr("显示名称"), window, cx));
        editor.update(cx, |editor, cx| editor.set_theme(self.is_dark(window), cx));
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                InlineEditEvent::Submit => this.submit_title_edit(window, cx),
                InlineEditEvent::Cancel { restore_focus } => {
                    this.cancel_title_edit(*restore_focus, window, cx)
                }
            });
        editor.read(cx).focus(window, cx);
        self.title_edit = Some(TitleEdit {
            target,
            editor,
            pending: None,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn cancel_title_edit(
        &mut self,
        restore_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &self.title_edit else { return };
        let focused = edit.editor.read(cx).is_focused(window, cx);
        // The editor can close while the already-submitted request finishes.
        if edit.pending.is_none() {
            self.title_edit = None;
        }
        if restore_focus && focused {
            self.title_focus.focus(window);
        }
        cx.notify();
    }

    fn submit_title_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let Some(edit) = &self.title_edit else {
                return Ok(());
            };
            if edit.pending.is_some() {
                return Ok(());
            }
            if self.title_target().as_ref() != Some(&edit.target) {
                bail!(tr("会话或连接已变化，请取消后重新编辑名称"));
            }
            let title = checked_title(edit.editor.read(cx).text(cx))?;
            if title == edit.editor.read(cx).original() {
                self.cancel_title_edit(true, window, cx);
                return Ok(());
            }
            let connection = self.connection()?;
            let edit = self.title_edit.as_mut().unwrap();
            let tab = edit.target.tab.clone();
            let id = edit.editor.entity_id();
            edit.pending = Some(title.clone());
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = connection
                    .client
                    .rename_tab(&tab, &title)
                    .map_err(Into::into);
                let _ = tx.send(Worker::TitleRenamed(id, result));
            });
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
        cx.notify();
    }

    pub(super) fn finish_title_edit(
        &mut self,
        id: EntityId,
        result: Result<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self
            .title_edit
            .as_mut()
            .filter(|edit| edit.editor.entity_id() == id)
        else {
            return;
        };
        let Some(sent) = edit.pending.take() else {
            return;
        };
        let device = edit.target.device.clone();
        let current = self
            .states
            .get(&device)
            .is_some_and(|state| state.generation == edit.target.generation);
        match result {
            Ok(_) if current => {
                let focused = edit.editor.read(cx).is_focused(window, cx);
                if edit.editor.update(cx, |editor, cx| editor.saved(sent, cx)) {
                    self.title_edit = None;
                    if focused {
                        self.title_focus.focus(window);
                    }
                }
            }
            Ok(_) => {
                edit.editor.update(cx, |editor, cx| editor.failed(cx));
                self.error = Some(tr("会话或连接已变化，请取消后重新编辑名称"));
            }
            Err(error) => {
                edit.editor.update(cx, |editor, cx| editor.failed(cx));
                self.error = Some(error.to_string());
            }
        }
        // Refresh even after an unknown result; never automatically repeat the mutation.
        self.refresh_device(&device);
        cx.notify();
    }

    pub(super) fn render_session_heading(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.title_target();
        let editing = self.title_edit.as_ref().is_some_and(|e| {
            !e.editor.read(cx).is_dismissed()
                && self.screen == Screen::Workspace
                && e.target.device == self.device().id
                && self.selected_pane.as_ref() == Some(&e.target.pane)
        });
        let mut slot = div().flex_1().min_w_0().flex().items_center();
        if editing {
            let dark = self.is_dark(window);
            let edit = self.title_edit.as_ref().unwrap();
            edit.editor
                .update(cx, |editor, cx| editor.set_theme(dark, cx));
            slot = slot.child(
                div()
                    .id("session-title-editor")
                    .w_full()
                    .max_w(px(256.))
                    .opacity(if edit.pending.is_some() { 0.7 } else { 1. })
                    .min_w_0()
                    .child(edit.editor.clone()),
            );
        } else {
            let title = self.session_title();
            let mut heading = div()
                .id("titlebar-heading")
                .min_w_0()
                .max_w_full()
                .h(px(22.))
                .px(px(4.))
                .rounded(px(4.))
                .border_1()
                .border_color(gpui::transparent_black())
                .text_size(px(12.))
                .line_height(px(20.))
                .text_color(rgb(p.title_text))
                .overflow_hidden()
                .child(fade_label(title));
            if current.is_some() {
                heading = heading
                    .key_context("SessionTitle")
                    .track_focus(&self.title_focus)
                    .tab_index(0)
                    .cursor_pointer()
                    .hover(move |s| s.bg(rgb(p.title_hover)))
                    .focus(move |s| s.bg(rgb(p.title_hover)).border_color(rgb(p.accent)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| this.begin_title_edit(window, cx)))
                    .on_action(cx.listener(|this, _: &RenameTitle, window, cx| {
                        this.begin_title_edit(window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &Escape, window, cx| {
                        this.cancel_title_edit(true, window, cx);
                        cx.stop_propagation();
                    }))
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new(tr("重命名会话"))
                            .action(&RenameTitle, Some("SessionTitle"))
                            .build(window, cx)
                    });
            }
            slot = slot.child(heading);
        }
        slot.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{TitleTarget, checked_title, snapshot_session_title};
    use crate::herdr::Snapshot;
    use serde_json::json;
    #[test]
    fn title_edit_validation_and_authoritative_label() {
        assert_eq!(checked_title("  修复登录 🪿  ").unwrap(), "修复登录 🪿");
        for invalid in ["", " \t", "bad\0name", "bad\nname"] {
            assert!(checked_title(invalid).is_err());
        }
        let mut snapshot: Snapshot = serde_json::from_value(json!({
            "panes":[{"pane_id":"p","workspace_id":"w","tab_id":"t","agent":"codex","terminal_title":"OSC"}],
            "agents":[{"pane_id":"p","workspace_id":"w","tab_id":"t","agent":"codex","title":"old agent title"}],
            "tabs":[{"tab_id":"t","workspace_id":"w","label":"codex","custom_label":"中文会话"}]
        })).unwrap();
        assert_eq!(snapshot_session_title(&snapshot, "p"), "中文会话");
        snapshot.tabs[0].custom_label = Some("codex".into());
        assert_eq!(snapshot_session_title(&snapshot, "p"), "codex");
        snapshot.tabs[0].custom_label = None;
        assert_eq!(snapshot_session_title(&snapshot, "p"), "old agent title");
        let target = TitleTarget {
            device: "d".into(),
            pane: "p".into(),
            tab: "t".into(),
            generation: 1,
        };
        assert!(
            target
                != TitleTarget {
                    generation: 2,
                    ..target.clone()
                }
        );
        assert!(
            target
                != TitleTarget {
                    device: "other".into(),
                    ..target.clone()
                }
        );
    }
}
