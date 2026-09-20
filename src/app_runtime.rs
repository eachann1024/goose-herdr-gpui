use super::*;
use crate::i18n::tr;
use crate::{
    notifications::{self, AgentStatus},
    usage::{CursorSummary, UsageService, Window as UsageWindow},
};

impl AppView {
    pub(super) fn configure_usage(&mut self) {
        let local = self.devices.iter().find(|d| d.is_local());
        let catalog = local
            .and_then(|d| self.states.get(&d.id))
            .map(|s| s.catalog.clone())
            .unwrap_or_default();
        if let Err(error) = self.usage.configure(
            &catalog,
            &self.settings.disabled_agent_kinds.iter().cloned().collect(),
        ) {
            self.error = Some(error.to_string());
        }
    }
    pub(super) fn runtime_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Also reacts to disabling a provider while its request is in flight.
        self.configure_usage();
        let changed = self.usage.poll();
        for route in notifications::take_routes() {
            let Some(index) = self.devices.iter().position(|d| d.id == route.device_id) else {
                continue;
            };
            let workspace = self.states.get(&route.device_id).and_then(|s| {
                let snapshot = s.snapshot.as_ref()?;
                snapshot
                    .panes
                    .iter()
                    .find(|p| p.pane_id == route.pane_id)
                    .map(|p| p.workspace_id.clone())
                    .or_else(|| {
                        snapshot
                            .agents
                            .iter()
                            .find(|a| a.pane_id == route.pane_id)
                            .map(|a| a.workspace_id.clone())
                    })
            });
            self.switch_scope(index, workspace);
            self.selected_pane = Some(route.pane_id.clone());
            self.screen = Screen::Workspace;
            self.close_overlay(window, cx);
            self.attach(route.pane_id);
            self.focus_active(window, cx);
            cx.notify();
        }
        // Permission completion arrives on a native callback, independently of worker messages.
        if changed
            || (self.screen == Screen::Settings
                && self.settings_page == SettingsPage::Notifications)
            || (self.screen == Screen::Settings && self.settings_page == SettingsPage::Agents)
        {
            cx.notify();
        }
    }
    pub(super) fn notify_snapshot(
        &mut self,
        device_id: &str,
        old: Option<&Snapshot>,
        new: &Snapshot,
    ) {
        let Some(old) = old else {
            return;
        };
        let device = self.devices.iter().find(|d| d.id == device_id);
        for agent in &new.agents {
            let Some(previous) = old.agents.iter().find(|a| a.pane_id == agent.pane_id) else {
                continue;
            };
            if previous.agent_status == agent.agent_status {
                continue;
            }
            let status = match agent.agent_status.as_deref() {
                Some("done") => AgentStatus::Done,
                Some("blocked") => AgentStatus::Blocked,
                _ => continue,
            };
            if status == AgentStatus::Done {
                let focused = self.active_terminal.as_deref()
                    == Some(&format!("{device_id}:{}", agent.pane_id));
                if focused
                    && self.screen == Screen::Workspace
                    && !self.show_usage
                    && self.form.is_none()
                    && self.search.is_none()
                {
                    self.unread
                        .remove(&(device_id.to_owned(), agent.pane_id.clone()));
                    continue;
                }
                self.unread
                    .insert((device_id.to_owned(), agent.pane_id.clone()));
            }
            let space = new
                .workspaces
                .iter()
                .find(|w| w.workspace_id == agent.workspace_id)
                .map(|w| w.label.as_str())
                .unwrap_or("");
            let tab_label = new
                .tabs
                .iter()
                .find(|t| t.tab_id == agent.tab_id)
                .and_then(|t| {
                    t.custom_label
                        .as_deref()
                        .or(t.extra.get("customLabel").and_then(|v| v.as_str()))
                        .or(t.extra.get("custom_label").and_then(|v| v.as_str()))
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                });
            let _ = notifications::post(
                tab_label
                    .or(agent.title.as_deref())
                    .or(agent.name.as_deref())
                    .unwrap_or("Agent"),
                agent.agent.as_deref().unwrap_or("Agent"),
                status,
                device_id,
                device.map(|d| d.name.as_str()).unwrap_or(&tr("设备")),
                &agent.pane_id,
                space,
            );
        }
    }
    pub(super) fn toggle_cursor_usage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if UsageService::cursor_authorized() {
            if let Err(error) = self.usage.set_cursor_authorized(false) {
                self.error = Some(error.to_string());
            }
            self.configure_usage();
            cx.notify();
            return;
        }
        if self
            .settings
            .disabled_agent_kinds
            .iter()
            .any(|s| s == "cursor")
        {
            self.error = Some(tr("请先在 Agent 设置启用 Cursor"));
            cx.notify();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Info,
            &tr("允许只读获取 Cursor 用量？"),
            Some(&tr("仅读取 Cursor 已有登录凭证并向 Cursor 请求账户用量。不会刷新、修改或导出登录凭证；你可随时断开。")),
            &[tr("取消").as_str(), tr("允许只读获取").as_str()],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if answer.await.ok() == Some(1) {
                let _ = cx.update(|_, cx| {
                    view.update(cx, |view, cx| {
                        if let Err(error) = view.usage.set_cursor_authorized(true) {
                            view.error = Some(error.to_string());
                        }
                        view.configure_usage();
                        cx.notify();
                    })
                });
            }
        })
        .detach();
    }
    pub(super) fn render_usage(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let local = self.device().is_local();
        let providers: Vec<&str> = crate::usage::PROVIDERS
            .into_iter()
            .filter(|provider| self.usage.shows(provider))
            .collect();
        let mut body = div()
            .id("usage-list")
            .overflow_y_scroll()
            .max_h(px(420.))
            .flex()
            .flex_col()
            .gap_2();
        if !local {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(rgb(p.muted))
                    .child(tr("请选择本机以查看这台 Mac 的用量。")),
            );
        } else if providers.is_empty() {
            body = body
                .items_center()
                .gap_2()
                .child(crate::icons::illustration("usage", 96.))
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("请在现有 Agent 设置中启用一个 Agent。")),
                )
                .child(self.btn(
                    "usage-open-agents",
                    "Agent 设置",
                    UiAction::AgentSettings,
                    p,
                    cx,
                ));
        } else {
            for provider in providers {
                body = body.child(self.render_usage_card(provider, p, cx));
            }
        }
        let panel = div()
            .id("usage-panel")
            .key_context("OverlayPopup")
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.close_overlay(window, cx);
            }))
            .w(px(320.))
            .p_2()
            .bg(rgb(p.raised))
            .text_color(rgb(p.text))
            .border_1()
            .border_color(rgb(p.line))
            .rounded(px(8.))
            .shadow_md()
            .flex()
            .flex_col()
            .gap_2()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.close_overlay(window, cx);
            }))
            .child(
                div()
                    .h(px(28.))
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("Agent 用量")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(p.muted))
                            .child(tr("本机 Mac")),
                    )
                    .child(div().flex_1())
                    .child(self.btn(
                        "usage-details",
                        if self.show_usage_details {
                            "收起"
                        } else {
                            "详情"
                        },
                        UiAction::ToggleUsageDetails,
                        p,
                        cx,
                    ))
                    .child(
                        self.icon_btn(
                            "usage-refresh",
                            "refresh",
                            "刷新用量",
                            UiAction::UsageRefresh,
                            p,
                            cx,
                        )
                        .when(!local, |d| d.opacity(0.4)),
                    ),
            )
            .child(body);
        let position = self
            .usage_trigger_bounds
            .map(|bounds| bounds.origin)
            .unwrap_or_else(|| point(px(12.), px(420.)));
        deferred(
            anchored()
                .anchor(Corner::BottomLeft)
                .position(position)
                .snap_to_window_with_margin(px(8.))
                .child(panel),
        )
        .with_priority(1)
        .into_any_element()
    }
    fn render_usage_card(&self, provider: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let fetching = self.usage.fetching.contains(provider);
        let mut card = div()
            .p_2()
            .rounded(px(8.))
            .bg(rgb(p.active))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(agent_display_name(provider)),
                    )
                    .child(div().flex_1())
                    .when(fetching, |d| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(rgb(p.muted))
                                .child(tr("正在读取…")),
                        )
                    })
                    .children(
                        self.usage
                            .snapshots
                            .get(provider)
                            .filter(|_| self.show_usage_details)
                            .and_then(|snapshot| snapshot.plan_type.clone())
                            .map(|plan| div().text_sm().text_color(rgb(p.muted)).child(plan)),
                    ),
            );
        if provider == "cursor" {
            card = card.child(self.render_cursor_account(true, p, cx));
        } else if let Some(snapshot) = self.usage.snapshots.get(provider) {
            if snapshot.status == "ok" {
                for (label, value) in [
                    ("当前窗口用量", &snapshot.session),
                    ("周期用量", &snapshot.weekly),
                    ("每月用量", &snapshot.monthly),
                    ("Fable 每周用量", &snapshot.fable_weekly),
                ] {
                    if let Some(window) = value {
                        card = card.child(usage_metric(label, window, p));
                    }
                }
                if let Some(credits) = &snapshot.rate_limit_reset_credits {
                    let mut line = format!("{} {}", tr("重置额度"), credits.available_count);
                    if let Some(earned) = credits
                        .total_earned_count
                        .filter(|_| self.show_usage_details)
                    {
                        line.push_str(&format!(" · {} {earned}", tr("已获得")));
                    }
                    card = card.child(div().text_sm().text_color(rgb(p.muted)).child(line));
                    if let Some(expires) =
                        credits.next_expires_at.filter(|_| self.show_usage_details)
                    {
                        card = card.child(div().text_sm().text_color(rgb(p.muted)).child(format!(
                            "{} {}",
                            tr("额度到期"),
                            timestamp(expires)
                        )));
                    }
                }
            } else if snapshot
                .usage_metadata
                .as_ref()
                .and_then(|meta| meta.failure_kind.as_deref())
                == Some("delegated-refresh-required")
            {
                card = card.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("登录已过期，请打开 Agent 命令行刷新登录。")),
                );
            } else if provider == "opencode" {
                card = card.child(div().text_sm().text_color(rgb(p.muted)).child(tr(
                    "OpenCode Go 需要网页会话 cookie；本机没有现成的凭证设置。",
                )));
            } else if snapshot.status == "unavailable" {
                card = card.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("用量暂不可用。请检查 Agent 登录和方案。")),
                );
            } else {
                card = card.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("无法读取用量。检查 Agent 后重试。")),
                );
            }
            if let Some(history) = snapshot
                .history
                .as_ref()
                .filter(|_| self.show_usage_details)
            {
                card = card.child(
                    div()
                        .flex()
                        .items_center()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(format!(
                            "{} {}",
                            tr("会话"),
                            compact_count(history.sessions)
                        ))
                        .child(div().flex_1())
                        .children(
                            history
                                .estimated_cost_usd
                                .map(|cost| div().child(format!("${cost:.2}"))),
                        ),
                );
                card = card.child(
                    div()
                        .flex()
                        .gap_3()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(format!(
                            "{} {}",
                            tr("输入"),
                            compact_count(history.input_tokens)
                        ))
                        .child(format!(
                            "{} {}",
                            tr("输出"),
                            compact_count(history.output_tokens)
                        )),
                );
            } else if snapshot.history_error == Some(true) {
                card = card.child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("本地历史暂不可读。")),
                );
            }
        } else if self.usage.failures.contains(provider) {
            card = card.child(
                div()
                    .text_sm()
                    .text_color(rgb(p.muted))
                    .child(tr("用量助手不可用，请重试或检查安装")),
            );
        }
        card.into_any_element()
    }
    pub(super) fn render_cursor_account(
        &self,
        compact: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = !self
            .settings
            .disabled_agent_kinds
            .iter()
            .any(|kind| kind == "cursor");
        let authorized = UsageService::cursor_authorized();
        let fetching = self.usage.fetching.contains("cursor");
        let details = !compact || self.show_usage_details;
        let status = cursor_status(
            authorized,
            fetching,
            self.usage.cursor_failure.as_deref(),
            self.usage.snapshots.contains_key("cursor"),
        );
        let mut view = div().flex().flex_col().gap_2();
        if show_cursor_status(
            details,
            authorized,
            fetching,
            self.usage.cursor_failure.is_some(),
            self.usage.snapshots.contains_key("cursor"),
        ) {
            view = view.child(div().text_sm().child(status));
        }
        if authorized {
            if let Some(summary) = self
                .usage
                .snapshots
                .get("cursor")
                .and_then(|snapshot| snapshot.cursor.as_ref())
            {
                if self.usage.cursor_failure.is_some() {
                    view = view.child(
                        div()
                            .text_sm()
                            .text_color(rgb(p.muted))
                            .child(tr("缓存用量 · 已过期")),
                    );
                }
                view = view.child(cursor_summary_view(summary, details, p));
                if let Some(updated) = self.usage.cursor_updated_at.filter(|_| details) {
                    view = view.child(div().text_sm().text_color(rgb(p.muted)).child(format!(
                        "{} {}",
                        tr("上次验证"),
                        relative_ago(updated)
                    )));
                }
            }
        }
        if compact && !authorized {
            view = view.child(
                div()
                    .text_sm()
                    .text_color(rgb(p.muted))
                    .child(tr("请在 Agent 设置中连接 Cursor 用量。")),
            );
        } else if authorized {
            let mut actions = div().flex().gap_2();
            if details {
                actions = actions.child(self.btn(
                    "usage-cursor-disconnect",
                    if fetching {
                        "取消连接"
                    } else {
                        "断开用量连接"
                    },
                    UiAction::ToggleCursorUsage,
                    p,
                    cx,
                ));
            }
            if !fetching && (details || self.usage.cursor_failure.is_some()) {
                actions = actions.child(
                    self.btn(
                        "usage-cursor-retry",
                        "重新查询",
                        UiAction::RetryCursor,
                        p,
                        cx,
                    )
                    .when(!enabled, |d| d.opacity(0.4)),
                );
            }
            if details || self.usage.cursor_failure.is_some() {
                view = view.child(actions);
            }
        } else {
            view = view
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(p.muted))
                        .child(tr("Cursor 应用和 CLI 是否可用，不会自动连接账户用量。")),
                )
                .child(
                    self.btn(
                        "usage-cursor-connect",
                        "连接用量…",
                        UiAction::ToggleCursorUsage,
                        p,
                        cx,
                    )
                    .when(!enabled, |d| d.opacity(0.4)),
                );
        }
        view.into_any_element()
    }
}

fn usage_metric(label: &str, window: &UsageWindow, p: Palette) -> Div {
    let percent = window.used_percent.clamp(0., 100.);
    let mut block = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_sm()
                .text_color(rgb(p.muted))
                .child(tr(label))
                .child(div().flex_1())
                .child(format!("{percent:.0}%")),
        )
        .child(
            div()
                .w_full()
                .h(px(4.))
                .rounded_full()
                .bg(rgb(p.line))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(rgb(p.accent))
                        .w(relative(percent as f32 / 100.)),
                ),
        );
    if let Some(reset) = window.resets_at {
        block = block.child(div().text_sm().text_color(rgb(p.muted)).child(format!(
            "{} {}",
            tr("重置时间"),
            timestamp(reset)
        )));
    }
    block
}

fn cursor_summary_view(summary: &CursorSummary, details: bool, p: Palette) -> Div {
    let mut rows = div().flex().flex_col().gap_1().text_sm();
    for (label, value) in [
        ("自动用量", summary.auto_percent),
        ("API 用量", summary.api_percent),
        ("总用量", summary.total_percent),
    ] {
        if details || label == "总用量" {
            rows = rows.child(kv_row(label, percent_or_unknown(value), p));
        }
    }
    for (label, value) in [
        ("套餐支出", summary.plan_spent_usd),
        ("包含额度支出", summary.included_spent_usd),
        ("套餐上限", summary.plan_limit_usd),
    ] {
        if details || label != "包含额度支出" {
            rows = rows.child(kv_row(label, money_or_unknown(value), p));
        }
    }
    let cycle = match (summary.cycle_start, summary.cycle_end) {
        (Some(start), Some(end)) => format!("{} – {}", date_label(start), date_label(end)),
        _ => tr("未知"),
    };
    rows.child(kv_row("账期", cycle, p))
}

fn kv_row(label: &str, value: String, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .child(div().text_color(rgb(p.muted)).child(tr(label)))
        .child(div().flex_1())
        .child(value)
}

fn show_cursor_status(
    details: bool,
    authorized: bool,
    fetching: bool,
    failed: bool,
    has_snapshot: bool,
) -> bool {
    details || !authorized || fetching || failed || !has_snapshot
}

#[cfg(test)]
mod usage_visibility_tests {
    use super::show_cursor_status;

    #[test]
    fn folds_only_healthy_compact_status() {
        assert!(!show_cursor_status(false, true, false, false, true));
        assert!(show_cursor_status(true, true, false, false, true)); // Details/settings.
        assert!(show_cursor_status(false, false, false, false, true)); // Disconnected.
        assert!(show_cursor_status(false, true, true, false, true)); // Loading.
        assert!(show_cursor_status(false, true, false, true, true)); // Stale/error.
        assert!(show_cursor_status(false, true, false, false, false)); // No data.
    }
}

fn cursor_status(
    authorized: bool,
    fetching: bool,
    failure: Option<&str>,
    has_snapshot: bool,
) -> String {
    if !authorized {
        return tr("尚未连接");
    }
    if fetching {
        return tr("正在验证 Cursor 用量…");
    }
    if let Some(kind) = failure {
        return cursor_failure(kind);
    }
    if has_snapshot {
        tr("用量已连接")
    } else {
        tr("尚未连接")
    }
}

fn cursor_failure(kind: &str) -> String {
    tr(match kind {
        "no-session" => "请先登录 Cursor 应用",
        "session-expired" => "Cursor 登录已过期，请打开 Cursor 重新登录后重试。",
        "authentication-failed" => "Cursor 拒绝此登录，请检查账户权限",
        "storage-unavailable" => "Cursor 登录存储暂不可读，请打开 Cursor 后重试",
        "network-failed" => "无法连接 Cursor，请检查网络",
        "unsupported-response" => "Cursor 未返回支持的用量字段，数值保持未知",
        "helper-unavailable" => "用量助手不可用，请检查安装",
        _ => "Cursor 用量请求失败，请重试",
    })
}

fn timestamp(milliseconds: f64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.)
        .unwrap_or(0.);
    let total_minutes = ((milliseconds - now) / 60000.).ceil().max(0.) as i64;
    let days = total_minutes / 1440;
    let hours = (total_minutes % 1440) / 60;
    let minutes = total_minutes % 60;
    let relative = if days > 0 && hours > 0 {
        format!(
            "{} {days} {} {hours} {}{}",
            tr("约"),
            tr("天"),
            tr("小时"),
            tr("后")
        )
    } else if days > 0 {
        format!("{} {days} {}", tr("约"), tr("天后"))
    } else if hours > 0 {
        format!("{} {hours} {}", tr("约"), tr("小时后"))
    } else {
        format!("{} {minutes} {}", tr("约"), tr("分钟后"))
    };
    match datetime_label(milliseconds) {
        clock if clock != tr("未知") => format!("{relative} {clock}"),
        _ => relative,
    }
}

fn relative_ago(updated: std::time::SystemTime) -> String {
    let minutes = updated.elapsed().unwrap_or_default().as_secs() / 60;
    if minutes >= 1440 {
        format!("{} {} {}", tr("约"), minutes / 1440, tr("天前"))
    } else if minutes >= 60 {
        format!("{} {} {}", tr("约"), minutes / 60, tr("小时前"))
    } else {
        format!("{minutes} {}", tr("分钟前"))
    }
}

fn date_label(milliseconds: f64) -> String {
    local_tm(milliseconds)
        .map(|tm| format!("{}/{}/{}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday))
        .unwrap_or_else(|| tr("未知"))
}

fn datetime_label(milliseconds: f64) -> String {
    let now_year = local_tm(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.)
            .unwrap_or(0.),
    )
    .map(|tm| tm.tm_year);
    local_tm(milliseconds)
        .map(|tm| {
            let date = if now_year == Some(tm.tm_year) {
                format!("{}/{}", tm.tm_mon + 1, tm.tm_mday)
            } else {
                format!("{}/{}/{}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
            };
            format!("{date} {:02}:{:02}", tm.tm_hour, tm.tm_min)
        })
        .unwrap_or_else(|| tr("未知"))
}

fn local_tm(milliseconds: f64) -> Option<libc::tm> {
    let secs = (milliseconds / 1000.).floor() as i64;
    if secs < 0 {
        return None;
    }
    let t = secs as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let ptr = unsafe { libc::localtime_r(&t, tm.as_mut_ptr()) };
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { tm.assume_init() })
    }
}

fn percent_or_unknown(value: Option<f64>) -> String {
    value
        .map(|n| format!("{n:.1}%"))
        .unwrap_or_else(|| tr("未知"))
}

fn money_or_unknown(value: Option<f64>) -> String {
    value
        .map(|n| format!("${n:.2}"))
        .unwrap_or_else(|| tr("未知"))
}

fn compact_count(value: u64) -> String {
    if crate::i18n::language() == "zh-Hans" {
        if value >= 100_000_000 {
            format!("{:.1}亿", value as f64 / 100_000_000.)
        } else if value >= 10_000 {
            format!("{:.1}万", value as f64 / 10_000.)
        } else {
            value.to_string()
        }
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.)
    } else {
        value.to_string()
    }
}

fn cursor_app_detected() -> bool {
    std::path::Path::new("/Applications/Cursor.app").exists()
}

impl AppView {
    pub(super) fn cursor_app_status(&self) -> String {
        tr(if cursor_app_detected() {
            "已检测到 Cursor 应用"
        } else {
            "未检测到 Cursor 应用"
        })
    }
}
