use anyhow::{Result, bail};
use gpui::{prelude::*, *};

use crate::{
    app::Escape,
    i18n::tr,
    input::{InputEvent, TextInput},
};

/// Shared inline rename editor. Persistence and target identity belong to its caller.
pub struct InlineEdit {
    input: Entity<TextInput>,
    original: String,
    dismissed: bool,
    _subscription: Subscription,
}

pub enum InlineEditEvent {
    Submit,
    Cancel { restore_focus: bool },
}
impl EventEmitter<InlineEditEvent> for InlineEdit {}

impl Focusable for InlineEdit {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

pub fn checked_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!(tr("名称不能为空"));
    }
    if value.chars().any(char::is_control) {
        bail!(tr("名称不能包含控制字符"));
    }
    Ok(value.to_owned())
}

fn exit_after_save(dismissed: bool, draft: &str, saved: &str) -> bool {
    dismissed || draft.trim() == saved
}

impl InlineEdit {
    pub fn new(
        initial: &str,
        placeholder: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new(initial, placeholder, window, cx));
        input.update(cx, |input, _| input.set_title());
        let subscription = cx.subscribe_in(&input, window, |_, _, event, _, cx| {
            if matches!(event, InputEvent::Submit) {
                cx.emit(InlineEditEvent::Submit);
            }
        });
        Self {
            input,
            original: initial.into(),
            dismissed: false,
            _subscription: subscription,
        }
    }

    pub fn set_theme(&mut self, dark: bool, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_theme(dark, cx));
    }

    pub fn focus(&self, window: &mut Window, cx: &App) {
        if !self.dismissed {
            self.input.focus_handle(cx).focus(window);
        }
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.input.focus_handle(cx).is_focused(window)
    }

    pub fn text<'a>(&self, cx: &'a App) -> &'a str {
        self.input.read(cx).text()
    }
    pub fn original(&self) -> &str {
        &self.original
    }
    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    pub fn cancel(&mut self, restore_focus: bool, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        cx.emit(InlineEditEvent::Cancel { restore_focus });
        cx.notify();
    }

    /// True means the caller can remove the editor; preserve newer in-flight edits otherwise.
    pub fn saved(&mut self, value: String, cx: &mut Context<Self>) -> bool {
        let exit = exit_after_save(self.dismissed, self.text(cx), &value);
        self.original = value;
        exit
    }

    pub fn failed(&mut self, cx: &mut Context<Self>) {
        self.dismissed = false;
        cx.notify();
    }
}

impl Render for InlineEdit {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("inline-edit")
            .w_full()
            .min_w_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                // TextInput consumes Escape first when cancelling an IME composition.
                this.cancel(true, cx);
                cx.stop_propagation();
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // Do not swallow the click or steal focus from its destination.
                this.cancel(false, cx);
            }))
            .child(self.input.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_name, exit_after_save};

    #[test]
    fn rename_validation_and_dismissed_save() {
        assert_eq!(checked_name("  我的空间 🪿  ").unwrap(), "我的空间 🪿");
        for invalid in ["", " \t", "bad\0name", "bad\nname"] {
            assert!(checked_name(invalid).is_err());
        }
        assert!(exit_after_save(false, " saved ", "saved"));
        assert!(!exit_after_save(false, "new draft", "saved"));
        assert!(exit_after_save(true, "new draft", "saved"));
    }
}
