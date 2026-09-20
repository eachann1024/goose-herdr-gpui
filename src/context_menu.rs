//! 工作台所有右键菜单的唯一入口。
//!
//! 用途：统一浮层、分组、键盘与右侧快捷键列。以后给右键项加快捷键时，
//! 只在 `shortcut_action` 登记映射并加入快捷键表，不要在各处手写菜单行。

use super::*;

pub(super) fn shortcut_action(action: &UiAction) -> Option<Box<dyn Action>> {
    match action {
        UiAction::CopySpacePath(_) => Some(Box::new(CopySpacePath)),
        UiAction::EditDeviceInput(_, command, _) => Some(command.action()),
        UiAction::NewTerminalIn(_) => Some(Box::new(NewTerminal)),
        UiAction::QuickAgent(kind) => {
            Some(Box::new(app_shortcuts::QuickAgent { kind: kind.clone() }))
        }
        _ => None,
    }
}

pub(super) fn menu_separator(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::CloseSpace(_)
            | UiAction::CloseRemote(_)
            | UiAction::DeleteDevice(_)
            | UiAction::EditDeviceInput(_, DeviceInputCommand::SelectAll, _)
    )
}

pub(super) fn menu_enabled(action: &UiAction) -> bool {
    match action {
        UiAction::EditDeviceInput(index, command, enabled) => *enabled && command.allowed(*index),
        _ => true,
    }
}

pub(super) fn menu_selection(
    index: Option<usize>,
    len: usize,
    next: bool,
    enabled: impl Fn(usize) -> bool,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let start = index.map_or(if next { 0 } else { len - 1 }, |index| {
        menu_step(index, len, next)
    });
    (0..len)
        .map(|offset| {
            if next {
                (start + offset) % len
            } else {
                (start + len - offset) % len
            }
        })
        .find(|&i| enabled(i))
}

pub(super) fn menu_step(index: usize, len: usize, next: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if next {
        (index + 1) % len
    } else {
        (index + len - 1) % len
    }
}

impl AppView {
    pub(super) fn open_menu(
        &mut self,
        at: Point<Pixels>,
        rows: Vec<(String, UiAction)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.is_none() {
            self.menu_previous_focus = window.focused(cx);
        }
        self.menu = Some((at, rows, None));
        self.menu_scroll = ScrollHandle::new();
        self.menu_focus.focus(window);
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            if let Some(focus) = self.menu_previous_focus.take() {
                focus.focus(window);
            }
            cx.notify();
        }
    }

    pub(super) fn menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, rows, index)) = &mut self.menu else {
            return;
        };
        match key {
            "up" => {
                *index = menu_selection(*index, rows.len(), false, |i| menu_enabled(&rows[i].1))
            }
            "down" => {
                *index = menu_selection(*index, rows.len(), true, |i| menu_enabled(&rows[i].1))
            }
            "enter" => {
                if let Some(action) = index
                    .and_then(|i| rows.get(i))
                    .map(|(_, action)| action.clone())
                    .filter(menu_enabled)
                {
                    self.dispatch(action, window, cx);
                }
            }
            "escape" | "tab" => self.close_menu(window, cx),
            _ => return,
        }
        if let Some((_, _, Some(index))) = &self.menu {
            self.menu_scroll.scroll_to_item(*index);
        }
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn menu_input_command(
        &mut self,
        command: DeviceInputCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = self.menu.as_ref().and_then(|(_, rows, _)| {
            rows.iter().find_map(|(_, action)| {
                matches!(action, UiAction::EditDeviceInput(_, candidate, _) if *candidate == command)
                    .then(|| action.clone())
            })
        });
        if let Some(action) = action.filter(menu_enabled) {
            self.dispatch(action, window, cx);
        }
        cx.stop_propagation();
    }

    pub(super) fn copy_space_path_from_shortcut(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self
            .menu
            .as_ref()
            .and_then(|(_, rows, _)| {
                rows.iter().find_map(|(_, action)| match action {
                    UiAction::CopySpacePath(id) => Some(id.clone()),
                    _ => None,
                })
            })
            .or_else(|| self.workspace.clone());
        let Some(id) = id else {
            return;
        };
        self.dispatch(UiAction::CopySpacePath(id), window, cx);
    }

    pub(super) fn render_context_menu(
        &self,
        at: Point<Pixels>,
        rows: &[(String, UiAction)],
        selected: &Option<usize>,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input_menu = rows
            .iter()
            .any(|(_, action)| matches!(action, UiAction::EditDeviceInput(..)));
        let mut menu = div()
            .id("shell-menu")
            .track_focus(&self.menu_focus)
            .key_context(if input_menu { "Input" } else { "ContextMenu" })
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.menu_key(&event.keystroke.key, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::MoveUp, window, cx| {
                    this.menu_key("up", window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::MoveDown, window, cx| {
                    this.menu_key("down", window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Enter, window, cx| {
                    this.menu_key("enter", window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.menu_key("escape", window, cx)
                }),
            )
            .on_action(cx.listener(
                |this, _: &gpui_component::input::IndentInline, window, cx| {
                    this.menu_key("tab", window, cx)
                },
            ))
            .on_action(cx.listener(
                |this, _: &gpui_component::input::OutdentInline, window, cx| {
                    this.menu_key("tab", window, cx)
                },
            ))
            .on_action(
                cx.listener(|this, _: &Escape, window, cx| this.menu_key("escape", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Close, window, cx| this.menu_key("escape", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusNext, window, cx| this.menu_key("tab", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusPrevious, window, cx| this.menu_key("tab", window, cx)),
            )
            .on_action(cx.listener(|this, _: &CopySpacePath, window, cx| {
                this.copy_space_path_from_shortcut(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Cut, window, cx| {
                    this.menu_input_command(DeviceInputCommand::Cut, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Copy, window, cx| {
                    this.menu_input_command(DeviceInputCommand::Copy, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Paste, window, cx| {
                    this.menu_input_command(DeviceInputCommand::Paste, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &gpui_component::input::SelectAll, window, cx| {
                    this.menu_input_command(DeviceInputCommand::SelectAll, window, cx)
                }),
            )
            .w(px(240.))
            .max_w((window.bounds().size.width - px(16.)).max(px(0.)))
            .max_h((window.bounds().size.height - px(16.)).max(px(0.)))
            .p_1()
            .bg(rgb(p.raised))
            .border_1()
            .border_color(Hsla::from(rgb(p.line)).opacity(0.5))
            .rounded(px(8.))
            .shadow_md()
            .overflow_y_scroll()
            .track_scroll(&self.menu_scroll)
            .flex()
            .flex_col();
        for (i, (label, action)) in rows.iter().enumerate() {
            let shortcut = shortcut_action(action).and_then(|bound| {
                gpui_component::kbd::Kbd::binding_for_action(
                    bound.as_ref(),
                    if input_menu {
                        Some("Input")
                    } else {
                        Some("ContextMenu")
                    },
                    window,
                )
                .or_else(|| {
                    gpui_component::kbd::Kbd::binding_for_action(
                        bound.as_ref(),
                        Some("Workbench"),
                        window,
                    )
                })
                .map(|kbd| div().text_color(rgb(p.muted)).child(kbd.appearance(false)))
            });
            let enabled = menu_enabled(action);
            let action = action.clone();
            let row = div()
                .id(SharedString::from(format!("menu-{i}")))
                .min_h(px(28.))
                .px(px(8.))
                .rounded(px(4.))
                .text_size(px(14.))
                .text_color(rgb(if enabled { p.text } else { p.muted }))
                .cursor_default()
                .flex()
                .items_center()
                .when(enabled && Some(i) == *selected, |d| d.bg(rgb(p.active)))
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if let Some((_, _, selected)) = &mut this.menu {
                        if *selected != enabled.then_some(i) {
                            *selected = enabled.then_some(i);
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if enabled {
                        this.dispatch(action.clone(), window, cx);
                    }
                }))
                .justify_between()
                .gap_3()
                .child(label.clone())
                .children(shortcut);
            menu = menu.child(
                div()
                    .flex_shrink_0()
                    .when(i > 0 && menu_separator(&rows[i].1), |d| {
                        d.child(
                            div()
                                .h(px(1.))
                                .my(px(4.))
                                .bg(Hsla::from(rgb(p.line)).opacity(0.5)),
                        )
                    })
                    .child(row),
            );
        }
        let menu = menu
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.close_menu(window, cx);
            }));
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(at)
                .snap_to_window_with_margin(px(8.))
                .child(menu),
        )
        .with_priority(1)
        .into_any_element()
    }
}
