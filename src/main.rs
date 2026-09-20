mod app;
mod credentials;
mod fade_label;
mod files;
mod git;
mod herdr;
mod i18n;
mod icons;
mod inline_edit;
mod input;
mod layout;
mod notifications;
mod project_icons;
mod settings;
mod sidebar_status;
mod tailcat;
mod terminal;
mod transport;
mod usage;

use gpui::{
    App, AppContext, Application, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType,
    TitlebarOptions, WindowBounds, WindowOptions, actions, point, px, size,
};

actions!(goose_herdr, [Quit, Hide, HideOthers]);

// Keep library overlay builders outside AppView's render borrow.
struct AppWindow(gpui::Entity<app::AppView>);
impl gpui::Render for AppWindow {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        let scope = self.0.read(cx).dialog_scope();
        // The space picker is an anchored AppView overlay, not a library Dialog;
        // device and NewItem use gpui_component Dialog.
        let dialogs = gpui_component::Root::render_dialog_layer(window, cx)
            .map(|layer| layer.into_any_element());
        let view = self.0.clone();
        let dismiss = view.clone();
        gpui::div()
            .size_full()
            .relative()
            .child(self.0.clone())
            .when_some(dialogs, |d, dialogs| {
                d.child(
                    gpui::div()
                        .absolute()
                        .inset_0()
                        .key_context("DialogOverlay")
                        .on_action({
                            let view = view.clone();
                            move |_: &app::Escape, window, cx| {
                                view.update(cx, |this, cx| this.dismiss_popup(window, cx));
                                cx.stop_propagation();
                            }
                        })
                        .on_action({
                            let view = dismiss;
                            move |_: &gpui_component::input::Escape, window, cx| {
                                view.update(cx, |this, cx| this.dismiss_popup(window, cx));
                                cx.stop_propagation();
                            }
                        })
                        .when_some(scope, |d, scope| {
                            d.track_focus(&scope).capture_key_down({
                                let view = view.clone();
                                move |event, window, cx| {
                                    if event.keystroke.key == "escape" {
                                        view.update(cx, |this, cx| {
                                            if this.overlay_ime_composing(cx) {
                                                return;
                                            }
                                            this.dismiss_popup(window, cx);
                                        });
                                        cx.stop_propagation();
                                        window.prevent_default();
                                        return;
                                    }
                                    // ponytail: component 0.5.1 has no modal focus trap; remove when its Dialog supplies one.
                                    if event.keystroke.key != "tab" {
                                        return;
                                    }
                                    let Some(start) = window.focused(cx) else {
                                        return;
                                    };
                                    loop {
                                        if event.keystroke.modifiers.shift {
                                            window.focus_prev();
                                        } else {
                                            window.focus_next();
                                        }
                                        if scope.contains_focused(window, cx)
                                            || start.is_focused(window)
                                        {
                                            break;
                                        }
                                    }
                                    cx.stop_propagation();
                                    window.prevent_default();
                                }
                            })
                        })
                        .child(dialogs),
                )
            })
    }
}

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    match credentials::handle_askpass() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("{}：{error}", i18n::tr("SSH 授权失败"));
            std::process::exit(1);
        }
    }
    Application::new()
        .with_assets(icons::Icons)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            input::init(cx);
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
            cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
            cx.on_action(|_: &HideOthers, cx: &mut App| cx.hide_other_apps());
            bind_system_shortcuts(cx);
            notifications::init();
            let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(720.), px(480.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Goose herdr GPUI".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(7.))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| app::AppView::new(window, cx));
                    let shell = cx.new(|_| AppWindow(view));
                    cx.new(|cx| gpui_component::Root::new(shell, window, cx))
                },
            );
            if let Err(error) = result {
                eprintln!("{}：{error}", i18n::tr("无法打开 Goose herdr GPUI 窗口"));
                cx.quit();
                return;
            }
            cx.activate(true);
        });
}

fn bind_system_shortcuts(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
        ]);
    } else {
        cx.bind_keys([
            KeyBinding::new("ctrl-shift-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, Some("Workbench && !Terminal")),
        ]);
    }
}

pub fn set_menus(cx: &mut App) {
    let preference_list = |key: &str| -> Vec<String> {
        match settings::read_preference(key) {
            Ok(Some(value)) => serde_json::from_value(value).unwrap_or_default(),
            Ok(None) => Vec::new(),
            Err(error) => {
                log::warn!("Unable to read menu preference {key}: {error}");
                Vec::new()
            }
        }
    };
    let order = preference_list("agents.kindOrder");
    let disabled = preference_list("agents.disabledKinds");
    let installed = app::installed_agent_kinds();
    let mut seen = std::collections::HashSet::new();
    let agent_items = order
        .iter()
        .map(String::as_str)
        .chain(installed.iter().map(String::as_str))
        .filter(|kind| {
            let kind = *kind;
            kind != "terminal"
                && installed.iter().any(|available| available == kind)
                && !disabled.iter().any(|hidden| hidden == kind)
                && seen.insert(kind.to_owned())
        })
        .map(|kind| {
            MenuItem::action(
                kind.to_owned(),
                app::app_shortcuts::QuickAgent {
                    kind: kind.to_owned(),
                },
            )
        })
        .collect();
    let sessions = (1u8..=9)
        .map(|n| {
            MenuItem::action(
                i18n::format("前往会话 {}", &[&n.to_string()]),
                app::GoSession { n },
            )
        })
        .collect::<Vec<_>>();
    let mut spaces = (1u8..=9)
        .map(|n| {
            MenuItem::action(
                i18n::format("前往空间 {}", &[&n.to_string()]),
                app::GoSpace { n },
            )
        })
        .collect::<Vec<_>>();
    spaces.push(MenuItem::action(
        i18n::format("前往空间 {}", &["0"]),
        app::GoSpace { n: 0 },
    ));
    cx.set_menus(vec![
        Menu {
            name: i18n::tr("Goose herdr GPUI").into(),
            items: vec![
                MenuItem::action(i18n::tr("设置…"), app::OpenSettings),
                MenuItem::os_submenu(i18n::tr("服务"), SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(i18n::tr("退出 Goose herdr GPUI"), Quit),
            ],
        },
        Menu {
            name: i18n::tr("文件").into(),
            items: vec![
                MenuItem::action(i18n::tr("新建…"), app::NewItem),
                MenuItem::action(i18n::tr("新建终端"), app::NewTerminal),
                MenuItem::action(i18n::tr("新建空间"), app::NewSpace),
                MenuItem::action(i18n::tr("关闭"), app::Close),
            ],
        },
        Menu {
            name: i18n::tr("编辑").into(),
            items: vec![
                MenuItem::action(i18n::tr("撤销"), input::Undo),
                MenuItem::action(i18n::tr("重做"), input::Redo),
                MenuItem::separator(),
                MenuItem::action(i18n::tr("剪切"), input::Cut),
                MenuItem::action(i18n::tr("复制"), input::Copy),
                MenuItem::action(i18n::tr("粘贴"), input::Paste),
                MenuItem::action(i18n::tr("全选"), input::SelectAll),
            ],
        },
        Menu {
            name: i18n::tr("视图").into(),
            items: vec![
                MenuItem::action(i18n::tr("搜索"), app::Search),
                MenuItem::action(i18n::tr("侧栏"), app::ToggleSidebar),
                MenuItem::action(i18n::tr("刷新"), app::Refresh),
            ],
        },
        Menu {
            name: "Agent".into(),
            items: agent_items,
        },
        Menu {
            name: i18n::tr("终端").into(),
            items: vec![
                MenuItem::action(i18n::tr("左右分栏"), app::SplitRight),
                MenuItem::action(i18n::tr("上下分栏"), app::SplitDown),
                MenuItem::separator(),
                MenuItem::action(i18n::tr("焦点左移"), app::FocusLeft),
                MenuItem::action(i18n::tr("焦点右移"), app::FocusRight),
                MenuItem::action(i18n::tr("焦点上移"), app::FocusUp),
                MenuItem::action(i18n::tr("焦点下移"), app::FocusDown),
                MenuItem::separator(),
                MenuItem::action(i18n::tr("交换左侧"), app::SwapLeft),
                MenuItem::action(i18n::tr("交换右侧"), app::SwapRight),
                MenuItem::action(i18n::tr("交换上方"), app::SwapUp),
                MenuItem::action(i18n::tr("交换下方"), app::SwapDown),
                MenuItem::separator(),
                MenuItem::action(i18n::tr("均分"), app::Equalize),
            ],
        },
        Menu {
            name: i18n::tr("前往").into(),
            items: sessions
                .into_iter()
                .chain(std::iter::once(MenuItem::separator()))
                .chain(spaces)
                .collect(),
        },
    ]);
}
