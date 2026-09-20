use super::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    dialog::{Dialog, DialogButtonProps},
    form::{field, v_form},
    input::{Input, InputEvent as ComponentInputEvent, InputState},
};

pub(super) struct DeviceForm {
    pub tailcat: bool,
    pub advanced: bool,
    pub inputs: [Entity<InputState>; 5],
    pub scope: FocusHandle,
    pub error: Option<String>,
}

// Keep the visible submit state and the save boundary on the same validation path.
fn device_draft(existing: Option<&Device>, tailcat: bool, values: &[String]) -> Result<Device> {
    let value = |index: usize| values.get(index).map(String::as_str).unwrap_or("");
    let local = existing.is_some_and(Device::is_local);
    let target = value(1).trim();
    let kind = if local {
        DeviceKind::Local
    } else if tailcat {
        let token = value(4).trim();
        if (token.is_empty() && !existing.is_some_and(|d| matches!(d.kind, DeviceKind::Tailcat)))
            || token.len() > 64 * 1024
            || token.contains('\0')
        {
            bail!("请输入有效的 Tailcat 令牌");
        }
        DeviceKind::Tailcat
    } else {
        transport::destination(target)?;
        DeviceKind::Ssh {
            target: target.to_owned(),
        }
    };
    let name = match value(0).trim() {
        "" if local => bail!("设备名称不能为空"),
        "" if tailcat => tr("Tailcat 设备"),
        "" => target.to_owned(),
        name => name.to_owned(),
    };
    herdr::validate(&name)?;
    let socket = value(2).trim();
    if !tailcat && !socket.is_empty() {
        herdr::validate(socket)?;
        if !local && !tailcat && socket.contains(':') {
            bail!("SSH socket 路径不能含冒号");
        }
    }
    if !local && !tailcat && value(3).contains('\0') {
        bail!("密码包含无效字符");
    }
    Ok(Device {
        id: existing.map(|d| d.id.clone()).unwrap_or_default(),
        name,
        kind,
        socket_path: if tailcat {
            existing.and_then(|d| d.socket_path.clone())
        } else {
            (!socket.is_empty()).then(|| socket.to_owned())
        },
        os_id: existing.and_then(|d| d.os_id.clone()),
    })
}

impl AppView {
    pub(crate) fn dialog_scope(&self) -> Option<FocusHandle> {
        let form = self.form.as_ref()?;
        // The space picker is not a modal Dialog, so it must not trap Tab.
        form.device_form.as_ref().map(|state| state.scope.clone())
    }

    pub(super) fn show_device_form(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = index.and_then(|i| self.devices.get(i)).cloned();
        if index.is_some() && existing.is_none() {
            return;
        }
        let tailcat = existing
            .as_ref()
            .is_some_and(|d| matches!(d.kind, DeviceKind::Tailcat));
        self.close_overlay(window, cx);
        let values = [
            existing
                .as_ref()
                .map(|d| d.name.clone())
                .unwrap_or_default(),
            existing
                .as_ref()
                .and_then(Device::ssh_target)
                .unwrap_or("")
                .to_owned(),
            existing
                .as_ref()
                .and_then(|d| d.socket_path.clone())
                .unwrap_or_default(),
            String::new(),
            String::new(),
        ];
        let placeholders = [
            "mac-studio",
            "user@host",
            "默认 Socket 路径",
            "可选；留空保留已存密码",
            "粘贴 Tailcat 令牌",
        ];
        let inputs = std::array::from_fn(|i| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(values[i].clone())
                    .placeholder(tr(placeholders[i]))
                    .masked(i >= 3)
            })
        });
        for input in &inputs {
            self.subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event, window, cx| {
                    if matches!(event, ComponentInputEvent::Change) {
                        if let Some(state) = this
                            .form
                            .as_mut()
                            .and_then(|form| form.device_form.as_mut())
                        {
                            state.error = None;
                        }
                        window.refresh();
                        cx.notify();
                    }
                },
            ));
        }
        let first_input = inputs[0].clone();
        self.form = Some(Form {
            kind: FormKind::Device(index),
            title: tr(if index.is_some() {
                "编辑设备"
            } else {
                "添加设备"
            }),
            message: None,
            submit: String::new(),
            fields: Vec::new(),
            workspace: None,
            agent_kind: None,
            space_form: None,
            item_scope: None,
            device_form: Some(DeviceForm {
                tailcat,
                advanced: false,
                inputs,
                scope: cx.focus_handle(),
                error: None,
            }),
        });
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.render_device_dialog(dialog, window, cx))
            } else {
                dialog
            }
        });
        first_input.focus_handle(cx).focus(window);
        cx.notify();
    }

    fn open_device_input_menu(
        &mut self,
        index: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self
            .form
            .as_ref()
            .and_then(|form| form.device_form.as_ref())
            .and_then(|form| form.inputs.get(index))
            .cloned()
        else {
            return;
        };
        self.close_menu(window, cx);
        input.focus_handle(cx).focus(window);
        let selected = input.update(cx, |input, cx| {
            input
                .selected_text_range(false, window, cx)
                .is_some_and(|selection| !selection.range.is_empty())
        });
        let rows = [
            ("剪切", DeviceInputCommand::Cut, selected),
            ("复制", DeviceInputCommand::Copy, selected),
            (
                "粘贴",
                DeviceInputCommand::Paste,
                cx.read_from_clipboard().is_some(),
            ),
            ("全选", DeviceInputCommand::SelectAll, true),
        ]
        .into_iter()
        .map(|(label, command, enabled)| {
            (
                tr(label),
                UiAction::EditDeviceInput(index, command, enabled && command.allowed(index)),
            )
        })
        .collect();
        self.open_menu(at, rows, window, cx);
    }

    pub(super) fn try_save_device_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.form.as_ref() else {
            return false;
        };
        let FormKind::Device(index) = form.kind else {
            return false;
        };
        let Some(state) = form.device_form.as_ref() else {
            return false;
        };
        // The component forwards Enter; composing text must never save the form.
        if state.inputs.iter().any(|input| {
            input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        }) {
            return false;
        }
        let values = state
            .inputs
            .iter()
            .map(|input| input.read(cx).value().to_string())
            .collect::<Vec<_>>();
        match self.save_device_form(index, &values) {
            Ok(()) => true,
            Err(error) => {
                if let Some(state) = self
                    .form
                    .as_mut()
                    .and_then(|form| form.device_form.as_mut())
                {
                    state.error = Some(tr(&error.to_string()));
                }
                window.refresh();
                cx.notify();
                false
            }
        }
    }

    pub(super) fn pick_device_socket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self
            .form
            .as_ref()
            .and_then(|form| form.device_form.as_ref())
            .map(|state| state.inputs[2].clone())
        else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("选择").into()),
        });
        cx.spawn_in(window, async move |_, cx| {
            if let Ok(Ok(Some(paths))) = picker.await {
                if let Some(path) = paths.first() {
                    let _ = cx.update(|window, cx| {
                        input.update(cx, |input, cx| {
                            input.set_value(path.display().to_string(), window, cx)
                        })
                    });
                }
            }
        })
        .detach();
    }

    pub(super) fn save_device_form(
        &mut self,
        index: Option<usize>,
        values: &[String],
    ) -> Result<()> {
        let form = self
            .form
            .as_ref()
            .and_then(|f| f.device_form.as_ref())
            .ok_or_else(|| anyhow::anyhow!("设备表单已关闭"))?;
        let existing = index.and_then(|i| self.devices.get(i));
        if index.is_some() && existing.is_none() {
            bail!("设备不存在");
        }
        let mut device = device_draft(existing, form.tailcat, values)?;
        if device.id.is_empty() {
            device.id = new_uuid();
        }
        let tailcat = matches!(device.kind, DeviceKind::Tailcat);
        let secret_kind = if tailcat {
            crate::credentials::SecretKind::TailcatToken
        } else {
            crate::credentials::SecretKind::SshPassword
        };
        // Passwords retain whitespace; tokens are pasted text, not passwords.
        let secret = if tailcat {
            values[4].trim()
        } else {
            values[3].as_str()
        };
        let old_secret = if !device.is_local() && (!secret.is_empty() || tailcat) {
            crate::credentials::get(secret_kind, &device.id)?
        } else {
            None
        };
        if tailcat && secret.is_empty() && old_secret.as_deref().is_none_or(|s| s.trim().is_empty())
        {
            bail!("请输入有效的 Tailcat 令牌");
        }
        let changed_secret = !device.is_local() && !secret.is_empty();
        if changed_secret {
            crate::credentials::set(secret_kind, &device.id, secret)?;
        }
        let mut devices = self.devices.clone();
        if let Some(index) = index {
            devices[index] = device.clone();
        } else {
            devices.push(device.clone());
        }
        if let Err(error) = settings::save_devices(&devices) {
            if changed_secret {
                let rollback = match old_secret {
                    Some(old) => crate::credentials::set(secret_kind, &device.id, &old),
                    None => crate::credentials::delete(secret_kind, &device.id),
                };
                if rollback.is_err() {
                    bail!("{}: {error}", tr("设备保存失败，凭据恢复失败；请重试保存"));
                }
            }
            return Err(error);
        }
        self.devices = devices;
        Ok(())
    }

    fn render_device_dialog(
        &self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let Some(form) = self.form.as_ref() else {
            return dialog;
        };
        let state = form.device_form.as_ref().unwrap();
        let index = match form.kind {
            FormKind::Device(index) => index,
            _ => None,
        };
        let existing = index.and_then(|i| self.devices.get(i));
        let local = existing.is_some_and(Device::is_local);
        let tailcat = state.tailcat && !local;
        let values = state
            .inputs
            .iter()
            .map(|input| input.read(cx).value().to_string())
            .collect::<Vec<_>>();
        let valid = device_draft(existing, tailcat, &values).is_ok();
        let input_field = |index: usize, label: &str| {
            field().label(tr(label)).child(
                div()
                    // ponytail: component 0.5.1 has no public context-menu builder; preserve selection/caret until upstream exposes one.
                    .capture_any_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, window, cx| {
                            if event.button == MouseButton::Right {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.open_device_input_menu(index, event.position, window, cx);
                            }
                        },
                    ))
                    .when(index >= 3, |d| {
                        // ponytail: GPUI 0.5.1 masking does not block clipboard actions; remove these guards when upstream does.
                        d.capture_action(|_: &gpui_component::input::Copy, _, cx| {
                            cx.stop_propagation()
                        })
                        .capture_action(
                            |_: &gpui_component::input::Cut, _, cx| cx.stop_propagation(),
                        )
                    })
                    .child(Input::new(&state.inputs[index])),
            )
        };
        let mut body = div().flex().flex_col().gap_4().child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(if local {
                    "本机 Herdr 连接"
                } else if tailcat {
                    "内置 WireGuard 隧道，无需额外客户端"
                } else {
                    "使用 OpenSSH 配置、agent、Tailscale SSH 或密码"
                })),
        );
        if !local {
            body = body.child(
                ButtonGroup::new("device-transport")
                    .compact()
                    .child(Button::new("device-ssh").label("SSH").selected(!tailcat))
                    .child(
                        Button::new("device-tailcat")
                            .label("Tailcat")
                            .selected(tailcat),
                    )
                    .on_click(cx.listener(|this, selected: &Vec<usize>, window, cx| {
                        if let Some(index) = selected.first() {
                            this.dispatch(UiAction::DeviceTransport(*index == 1), window, cx);
                        }
                    })),
            );
        }
        let mut fields = v_form().child(input_field(0, "名称"));
        if !local {
            let description = if tailcat {
                if existing.is_some_and(|d| matches!(d.kind, DeviceKind::Tailcat)) {
                    format!(
                        "{} {}",
                        tr("留空保留已存令牌"),
                        tr("令牌保存在钥匙串。独立终端和文件功能需要 SSH。")
                    )
                } else {
                    tr("令牌保存在钥匙串。独立终端和文件功能需要 SSH。")
                }
            } else {
                tr("支持 user@host、SSH 配置别名或 user@host:port。")
            };
            fields = fields.child(
                input_field(
                    if tailcat { 4 } else { 1 },
                    if tailcat {
                        "Tailcat 令牌"
                    } else {
                        "SSH 目标"
                    },
                )
                .description(description),
            );
        }
        body = body.child(fields);
        if !tailcat {
            body = body.child(
                Button::new("device-advanced")
                    .ghost()
                    .small()
                    .label(tr("高级选项"))
                    .icon(if state.advanced {
                        gpui_component::IconName::ChevronDown
                    } else {
                        gpui_component::IconName::ChevronRight
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dispatch(UiAction::DeviceAdvanced, window, cx)
                    })),
            );
            if state.advanced {
                let mut advanced = v_form();
                if !local {
                    advanced = advanced.child(input_field(3, "密码（留空不更改）"));
                }
                advanced = advanced.child(input_field(2, "Socket 路径（可留空）"));
                body = body.child(advanced);
                if local {
                    body = body.child(
                        Button::new("device-socket-browse")
                            .label(tr("选择…"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.pick_device_socket(window, cx)
                            })),
                    );
                }
            }
        }
        if let Some(error) = &state.error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        let on_ok = cx.entity().downgrade();
        let on_close = cx.entity().downgrade();
        let submit = cx.entity().downgrade();
        let submit_label = tr(if index.is_some() {
            "保存"
        } else {
            "添加设备"
        });
        dialog
            .title(form.title.clone())
            .width(px(400.))
            .max_h(window.viewport_size().height - px(80.))
            .confirm()
            .button_props(DialogButtonProps::default().cancel_text(tr("取消")))
            .child(body)
            .on_ok(move |_, window, cx| {
                on_ok
                    .update(cx, |this, cx| this.try_save_device_dialog(window, cx))
                    .unwrap_or(false)
            })
            .on_close(move |_, _, cx| {
                let _ = on_close.update(cx, |this, cx| {
                    this.menu = None;
                    this.menu_previous_focus = None;
                    this.form = None;
                    this.subscriptions.clear();
                    cx.notify();
                });
            })
            .footer(move |_, cancel, window, cx| {
                let submit = submit.clone();
                vec![
                    cancel(window, cx),
                    Button::new("device-submit")
                        .primary()
                        .label(submit_label.clone())
                        .disabled(!valid)
                        .on_click(move |_, window, cx| {
                            let _ = submit.update(cx, |this, cx| this.submit(window, cx));
                        })
                        .into_any_element(),
                ]
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Device, DeviceKind, device_draft};
    #[test]
    fn device_fields_preserve_transport_boundaries() {
        let mut values = vec![
            "".into(),
            "user@host:2222".into(),
            "/tmp/herdr.sock".into(),
            " password ".into(),
            "".into(),
        ];
        let ssh = device_draft(None, false, &values).unwrap();
        assert_eq!(ssh.name, "user@host:2222");
        assert_eq!(ssh.socket_path.as_deref(), Some("/tmp/herdr.sock"));
        assert!(device_draft(None, true, &values).is_err());
        values[4] = " token ".into();
        let mut tc = device_draft(None, true, &values).unwrap();
        assert!(matches!(tc.kind, DeviceKind::Tailcat));
        assert!(tc.socket_path.is_none());
        tc.id = "existing".into();
        values[4].clear();
        assert!(device_draft(Some(&tc), true, &values).is_ok());
        assert!(device_draft(Some(&ssh), true, &values).is_err());
        values[1] = "-oProxyCommand=x".into();
        assert!(device_draft(None, false, &values).is_err());
        values[0] = "Local".into();
        assert!(matches!(
            device_draft(Some(&Device::local()), false, &values)
                .unwrap()
                .kind,
            DeviceKind::Local
        ));
    }
}
