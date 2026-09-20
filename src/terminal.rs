//! Herdr host adapter for the vendored Zed terminal core and grid renderer.
use anyhow::{Context as _, Result, bail};
use gpui::{prelude::*, *};
use std::{
    ffi::{OsStr, OsString},
    ops::Range,
    path::PathBuf,
    sync::Arc,
};
use zed_terminal::{
    Terminal, TerminalBounds, TerminalBuilder,
    alacritty_terminal::{term::TermMode, tty, vte::ansi::CursorShape},
    element::TerminalElement,
    palette::Palette,
};

#[derive(Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub authentication: Option<Arc<crate::credentials::SshAuthentication>>,
}

pub struct TerminalView {
    terminal: Entity<Terminal>,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    marked: String,
    marked_selection: Range<usize>,
    font_family: SharedString,
    font_size: f32,
    line_height: f32,
    font_weight: FontWeight,
    mouse_reporting: bool,
    padding_x: f32,
    padding_y: f32,
    balance_padding: bool,
    copy_on_select: bool,
    mouse_gesture: Option<bool>, // Some(true): local selection; Some(false): application mouse reporting.
    palette: Palette,
    ended: bool,
    error: Option<String>,
    connection_issue: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
    _authentication: Option<Arc<crate::credentials::SshAuthentication>>,
}

/// Key context of a terminal that can forward keystrokes to the PTY. An extra flag is
/// needed because `gpui_component::Root` binds tab/shift-tab to focus cycling in its
/// ancestor "Root" context, which otherwise wins before this view sees the keys.
pub(crate) const PTY_KEYS_CONTEXT: &str = "Terminal PtyKeys";

fn content_axis(
    length: f32,
    cell: f32,
    padding: f32,
    balance: bool,
    min_cells: usize,
) -> (f32, f32) {
    // ponytail: tiny panes retain the core minimum grid and clip; no minimum window size.
    let minimum = cell * min_cells as f32;
    let padding = padding.min(((length - minimum) / 2.).max(0.));
    let available = (length - padding * 2.).max(minimum);
    let extra = if balance {
        (available - (available / cell).floor() * cell) / 2.
    } else {
        0.
    };
    (padding + extra, available - extra)
}

fn content_bounds(
    bounds: Bounds<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    padding_x: f32,
    padding_y: f32,
    balance: bool,
) -> Bounds<Pixels> {
    let (x, width) = content_axis(
        bounds.size.width.into(),
        cell_width.into(),
        padding_x,
        balance,
        2, // The vendored core rejects grids narrower than two columns.
    );
    let (y, height) = content_axis(
        bounds.size.height.into(),
        line_height.into(),
        padding_y,
        balance,
        1,
    );
    Bounds::new(
        bounds.origin + point(px(x), px(y)),
        size(px(width), px(height)),
    )
}

fn attached_client_issue(attach: bool, output: &str) -> Option<&'static str> {
    let output = output.to_ascii_lowercase();
    (attach
        && (output.contains("already has an attached client")
            || output.contains("already attached")))
    .then_some("此会话已由另一客户端连接；如需切换控制端，请点击“接管会话”。")
}

fn should_default_kitty_image_protocol(
    spec_has_value: bool,
    process_value: Option<&OsStr>,
) -> bool {
    !spec_has_value && process_value.is_none()
}

fn terminal_builder(spec: &CommandSpec, light: bool, window: &Window) -> Result<TerminalBuilder> {
    if spec.program.as_os_str().is_empty() {
        bail!("终端程序不能为空");
    }
    if spec.cwd.as_ref().is_some_and(|cwd| !cwd.is_dir()) {
        bail!("终端工作目录不存在");
    }
    // Alacritty's PTY interface requires UTF-8; never silently corrupt command arguments.
    let utf8 = |value: &std::ffi::OsStr| {
        value
            .to_str()
            .map(str::to_owned)
            .context("终端命令或环境变量不是有效 UTF-8")
    };
    let mut env = spec
        .env
        .iter()
        .map(|(k, v)| Ok((utf8(k)?, utf8(v)?)))
        .collect::<Result<std::collections::HashMap<_, _>>>()?;
    env.entry("TERM".into())
        .or_insert_with(|| "xterm-256color".into());
    env.entry("COLORTERM".into())
        .or_insert_with(|| "truecolor".into());
    env.entry("TERM_PROGRAM".into())
        .or_insert_with(|| "Goose Herdr".into());
    if should_default_kitty_image_protocol(
        env.contains_key("PI_IMAGE_PROTOCOL"),
        std::env::var_os("PI_IMAGE_PROTOCOL").as_deref(),
    ) {
        env.insert("PI_IMAGE_PROTOCOL".into(), "kitty".into());
    }
    if std::env::var_os("LANG").is_none() {
        env.entry("LANG".into())
            .or_insert_with(|| "en_US.UTF-8".into());
    }
    TerminalBuilder::new(
        tty::Options {
            shell: Some(tty::Shell::new(
                utf8(spec.program.as_os_str())?,
                spec.args.iter().map(|a| utf8(a)).collect::<Result<_>>()?,
            )),
            working_directory: spec.cwd.clone(),
            drain_on_exit: true,
            env,
        },
        window.window_handle().window_id().as_u64(),
        light,
    )
}

impl TerminalView {
    pub fn create(
        spec: CommandSpec,
        light: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        let builder = terminal_builder(&spec, light, window)?;
        let terminal = cx.new(|cx| builder.subscribe(cx));
        Ok(cx.new(|cx| Self::from_terminal(spec, terminal, light, window, cx)))
    }
    fn from_terminal(
        spec: CommandSpec,
        terminal: Entity<Terminal>,
        light: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let attach = spec.args.iter().any(|a| {
            let a = a.to_string_lossy();
            a.contains("herdr") && a.contains(" attach ")
        });
        let events = cx.subscribe(&terminal, move |view, terminal, event, cx| {
            if matches!(event, zed_terminal::Event::CloseTerminal) {
                let terminal = terminal.read(cx);
                view.ended = true;
                if terminal.exit_code != Some(0) {
                    view.connection_issue = attached_client_issue(
                        attach,
                        &terminal.last_n_non_empty_lines(12).join("\n"),
                    );
                }
            }
            cx.notify();
        });
        let changes = cx.observe(&terminal, |_, _, cx| cx.notify());
        let focus = cx.focus_handle();
        let focus_in = cx.on_focus_in(&focus, window, |view, _, cx| {
            view.terminal.read(cx).focus_in();
        });
        let focus_out = cx.on_focus_out(&focus, window, |view, _, _, cx| {
            view.terminal.update(cx, |t, _| t.focus_out());
        });
        Self {
            terminal,
            focus,
            bounds: Bounds::default(),
            marked: String::new(),
            marked_selection: 0..0,
            font_family: "Menlo".into(),
            font_size: 14.,
            line_height: 1.4,
            font_weight: FontWeight::NORMAL,
            mouse_reporting: true,
            padding_x: 8.,
            padding_y: 8.,
            balance_padding: false,
            copy_on_select: false,
            mouse_gesture: None,
            palette: Palette {
                light,
                ..Palette::default()
            },
            ended: false,
            error: None,
            connection_issue: None,
            _subscriptions: vec![events, changes, focus_in, focus_out],
            _authentication: spec.authentication,
        }
    }
    pub fn set_font(&mut self, family: &str, size: f32, line_height: f32) -> Result<()> {
        if !size.is_finite()
            || !(8.0..=72.0).contains(&size)
            || !line_height.is_finite()
            || !(1.0..=3.0).contains(&line_height)
        {
            bail!("终端字体设置无效");
        }
        self.font_family = if family.trim().is_empty() {
            "Menlo"
        } else {
            family
        }
        .to_owned()
        .into();
        self.font_size = size;
        self.line_height = line_height;
        Ok(())
    }
    pub fn set_layout(
        &mut self,
        padding_x: f32,
        padding_y: f32,
        balance: bool,
        copy_on_select: bool,
    ) -> Result<()> {
        if !padding_x.is_finite()
            || !(0.0..=64.0).contains(&padding_x)
            || !padding_y.is_finite()
            || !(0.0..=64.0).contains(&padding_y)
        {
            bail!("终端内边距必须为 0–64 的有限数值");
        }
        self.padding_x = padding_x;
        self.padding_y = padding_y;
        self.balance_padding = balance;
        self.copy_on_select = copy_on_select;
        Ok(())
    }
    pub fn set_style(&mut self, weight: f64, mouse_reporting: bool) -> Result<()> {
        if !weight.is_finite() {
            bail!("终端字体粗细无效");
        }
        self.font_weight = FontWeight(if weight < -0.3 {
            300.
        } else if weight < 0.15 {
            400.
        } else if weight < 0.27 {
            500.
        } else if weight < 0.35 {
            600.
        } else if weight < 0.5 {
            700.
        } else {
            800.
        });
        self.mouse_reporting = mouse_reporting;
        Ok(())
    }
    pub fn sync_theme(&mut self, light: bool, force: bool, cx: &mut Context<Self>) {
        if !force && self.palette.light == light {
            return;
        }
        self.palette.light = light;
        self.terminal.update(cx, |t, _| t.sync_theme(light, force));
        self.error = self.terminal.read(cx).error();
        cx.notify();
    }
    pub fn is_ended(&self) -> bool {
        self.ended
    }
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
    pub fn connection_issue(&self) -> Option<&str> {
        self.connection_issue
    }
    pub fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) -> Result<()> {
        if self.ended {
            bail!("终端进程已退出");
        }
        self.terminal.update(cx, |t, _| t.input(bytes.to_vec()));
        self.error = self.terminal.read(cx).error();
        cx.notify();
        if let Some(error) = &self.error {
            bail!("{error}");
        }
        Ok(())
    }
    pub fn paste(&mut self, text: &str, cx: &mut Context<Self>) -> Result<()> {
        if self.ended {
            bail!("终端进程已退出");
        }
        self.terminal.update(cx, |t, _| t.paste(text));
        self.error = self.terminal.read(cx).error();
        cx.notify();
        if let Some(error) = &self.error {
            bail!("{error}");
        }
        Ok(())
    }
    pub fn search(&mut self, expression: &str, cx: &mut Context<Self>) -> Result<bool> {
        let result = self.terminal.update(cx, |t, _| t.search(expression));
        cx.notify();
        result
    }
    fn copy_action(&mut self, _: &crate::input::Copy, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal.update(cx, |t, _| t.copy(Some(true)));
        cx.notify();
    }
    fn paste_action(&mut self, _: &crate::input::Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let _ = self.paste(&text, cx);
        }
    }
    fn select_all_action(
        &mut self,
        _: &crate::input::SelectAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |t, _| {
            t.select_all();
            if self.copy_on_select {
                t.copy(Some(true));
            }
        });
        cx.notify();
    }
    fn mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selecting) = self.mouse_gesture.take() else {
            return;
        };
        self.terminal.update(cx, |t, cx| {
            t.mouse_up(event, cx);
            if selecting && self.copy_on_select && !t.mouse_mode(event.modifiers.shift) {
                t.sync(window, cx);
                if t.last_content
                    .selection_text
                    .as_ref()
                    .is_some_and(|text| !text.is_empty())
                {
                    t.copy(Some(true));
                }
            }
        });
        cx.notify();
    }
    /// Whether the PTY can consume keystrokes: no IME composition is pending and the
    /// child process is still running.
    fn accepts_keys(&self) -> bool {
        self.marked.is_empty() && !self.ended
    }
    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.accepts_keys() {
            return;
        }
        if self
            .terminal
            .update(cx, |t, _| t.try_keystroke(&event.keystroke, true))
        {
            self.error = self.terminal.read(cx).error();
            cx.stop_propagation();
            cx.notify();
        }
    }
}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let painter = entity.clone();
        div()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .key_context(if self.accepts_keys() {
                PTY_KEYS_CONTEXT
            } else {
                "Terminal"
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::copy_action))
            .on_action(cx.listener(Self::paste_action))
            .on_action(cx.listener(Self::select_all_action))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, e: &MouseDownEvent, window, cx| {
                    view.focus.focus(window);
                    view.mouse_gesture =
                        Some(!view.terminal.read(cx).mouse_mode(e.modifiers.shift));
                    view.terminal.update(cx, |t, cx| t.mouse_down(e, cx));
                    cx.notify();
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(|view, e: &MouseMoveEvent, _, cx| {
                view.terminal.update(cx, |t, cx| {
                    t.mouse_move(e, cx);
                    if e.dragging() && view.mouse_gesture.is_some() {
                        t.mouse_drag(e, view.bounds, cx);
                    }
                });
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(|view, e, _, cx| {
                view.terminal.update(cx, |t, _| t.scroll_wheel(e));
                cx.notify();
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        entity.update(cx, |view, cx| {
                            let text_style = TextStyle {
                                font_family: view.font_family.clone(),
                                font_size: px(view.font_size).into(),
                                font_weight: view.font_weight,
                                ..Default::default()
                            };
                            let font_id = window.text_system().resolve_font(&text_style.font());
                            let cell_width = window
                                .text_system()
                                .advance(font_id, px(view.font_size), 'm')
                                .map(|s| s.width)
                                .unwrap_or(px(view.font_size * 0.6));
                            let cell_width = cell_width.max(px(1.));
                            let line_height = px(view.font_size * view.line_height);
                            view.bounds = content_bounds(
                                bounds,
                                cell_width,
                                line_height,
                                view.padding_x,
                                view.padding_y,
                                view.balance_padding,
                            );
                            let dimensions =
                                TerminalBounds::new(line_height, cell_width, view.bounds);
                            let frame = view.terminal.update(cx, |terminal, cx| {
                                terminal.palette = view.palette;
                                terminal.mouse_reporting = view.mouse_reporting;
                                terminal.set_size(dimensions);
                                terminal.sync(window, cx);
                                let content = &terminal.last_content;
                                let (rects, runs) = TerminalElement::layout_grid(
                                    content.cells.iter().cloned(),
                                    0,
                                    &text_style,
                                    None,
                                    1.,
                                    &view.palette,
                                );
                                (
                                    dimensions,
                                    rects,
                                    runs,
                                    content.cursor,
                                    content.display_offset,
                                    content.selection,
                                    content.mode,
                                    terminal.image_placements(),
                                )
                            });
                            view.error = view.terminal.read(cx).error();
                            frame
                        })
                    },
                    move |bounds,
                          (dimensions, rects, runs, cursor, offset, selection, mode, images),
                          window,
                          cx| {
                        let view = painter.read(cx);
                        let focus = view.focus.clone();
                        let palette = view.palette;
                        let marked = view.marked.clone();
                        let family = view.font_family.clone();
                        let font_size = view.font_size;
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(dimensions.bounds, painter.clone()),
                            cx,
                        );
                        window.paint_quad(fill(bounds, palette.index(257)));
                        let bounds = dimensions.bounds;
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for rect in rects {
                                rect.paint(bounds.origin, &dimensions, window);
                            }
                            if let Some(selection) = selection {
                                for row in 0..dimensions.num_lines() {
                                    let line = row as i32 - offset as i32;
                                    if line < selection.start.line.0 || line > selection.end.line.0
                                    {
                                        continue;
                                    }
                                    let start = if line == selection.start.line.0 {
                                        selection.start.column.0
                                    } else {
                                        0
                                    };
                                    let end = if line == selection.end.line.0 {
                                        selection.end.column.0 + 1
                                    } else {
                                        dimensions.num_columns()
                                    };
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            bounds.origin
                                                + point(
                                                    dimensions.cell_width * start as f32,
                                                    dimensions.line_height * row as f32,
                                                ),
                                            size(
                                                dimensions.cell_width
                                                    * end.saturating_sub(start) as f32,
                                                dimensions.line_height,
                                            ),
                                        ),
                                        rgba(0x7088aa66),
                                    ));
                                }
                            }
                            for run in runs {
                                run.paint(bounds.origin, &dimensions, window, cx);
                            }
                            for image in images {
                                let image_bounds = Bounds::new(
                                    bounds.origin
                                        + point(
                                            dimensions.cell_width * image.column,
                                            dimensions.line_height * image.line,
                                        ),
                                    size(
                                        dimensions.cell_width * image.columns,
                                        dimensions.line_height * image.rows,
                                    ),
                                );
                                if let Err(error) = window.paint_image(
                                    image_bounds,
                                    Corners::default(),
                                    image.image,
                                    0,
                                    false,
                                ) {
                                    log::warn!("terminal image paint failed: {error:#}");
                                }
                            }
                            let row = cursor.point.line.0 + offset as i32;
                            let origin = bounds.origin
                                + point(
                                    dimensions.cell_width * cursor.point.column.0 as f32,
                                    dimensions.line_height * row as f32,
                                );
                            if row >= 0
                                && (row as usize) < dimensions.num_lines()
                                && mode.contains(TermMode::SHOW_CURSOR)
                                && focus.is_focused(window)
                            {
                                let cursor_bounds = match cursor.shape {
                                    CursorShape::Hidden => None,
                                    CursorShape::Beam => Some(Bounds::new(
                                        origin,
                                        size(px(2.), dimensions.line_height),
                                    )),
                                    CursorShape::Underline => Some(Bounds::new(
                                        origin + point(px(0.), dimensions.line_height - px(2.)),
                                        size(dimensions.cell_width, px(2.)),
                                    )),
                                    _ => Some(Bounds::new(
                                        origin,
                                        size(dimensions.cell_width, dimensions.line_height),
                                    )),
                                };
                                if let Some(bounds) = cursor_bounds {
                                    window.paint_quad(outline(
                                        bounds,
                                        palette.index(258),
                                        BorderStyle::Solid,
                                    ));
                                }
                            }
                            if !marked.is_empty() {
                                let run = TextRun {
                                    len: marked.len(),
                                    font: font(family),
                                    color: palette.index(256),
                                    background_color: Some(palette.index(257)),
                                    underline: Some(UnderlineStyle {
                                        thickness: px(1.),
                                        color: None,
                                        wavy: false,
                                    }),
                                    strikethrough: None,
                                };
                                let line = window.text_system().shape_line(
                                    marked.into(),
                                    px(font_size),
                                    &[run],
                                    Some(dimensions.cell_width),
                                );
                                let _ = line.paint(origin, dimensions.line_height, window, cx);
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

fn utf16_slice(text: &str, range: Range<usize>) -> String {
    String::from_utf16_lossy(
        &text
            .encode_utf16()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .collect::<Vec<_>>(),
    )
}
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let len = self.marked.encode_utf16().count();
        let range = range.start.min(len)..range.end.min(len);
        *actual = Some(range.clone());
        Some(utf16_slice(&self.marked, range))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.marked_selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        self.marked_selection = 0..0;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked.clear();
        self.marked_selection = 0..0;
        let _ = self.send(text.as_bytes(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let len = self.marked.encode_utf16().count();
        let range = range.unwrap_or(0..len);
        self.marked = format!(
            "{}{}{}",
            utf16_slice(&self.marked, 0..range.start.min(len)),
            text,
            utf16_slice(&self.marked, range.end.min(len)..len)
        );
        let len = self.marked.encode_utf16().count();
        self.marked_selection = selected.unwrap_or(len..len);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let content = &self.terminal.read(cx).last_content;
        let dimensions = content.terminal_bounds;
        Some(Bounds::new(
            self.bounds.origin
                + point(
                    dimensions.cell_width * content.cursor.point.column.0 as f32,
                    dimensions.line_height
                        * (content.cursor.point.line.0 + content.display_offset as i32).max(0)
                            as f32,
                ),
            size(dimensions.cell_width, dimensions.line_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.marked_selection.end)
    }
}
#[cfg(test)]
mod tests {
    use super::{
        Palette, TermMode, TerminalElement, attached_client_issue, content_axis,
        should_default_kitty_image_protocol, utf16_slice,
    };
    #[test]
    fn terminal_padding_keeps_cells_and_balances_remainder() {
        assert_eq!(content_axis(103., 10., 8., false, 2), (8., 87.));
        assert_eq!(content_axis(103., 10., 8., true, 2), (11.5, 83.5));
        for balance in [false, true] {
            for length in [0., 5., 10., 103.] {
                let (offset, extent) = content_axis(length, 10., 64., balance, 2);
                assert!(offset >= 0. && extent >= 20.);
                assert!((extent / 10.).floor() >= 2.);
                if length >= 20. {
                    assert!(offset + extent <= length);
                }
            }
        }
    }
    use gpui::{Keystroke, TextStyle};
    use std::ffi::OsStr;
    #[test]
    fn zed_grid_batches_text_and_preserves_input() {
        use zed_terminal::alacritty_terminal::{
            index::{Column, Line, Point},
            term::cell::{Cell, Flags},
        };
        let cells = (0..80).map(|column| zed_terminal::IndexedCell {
            point: Point::new(Line(0), Column(column)),
            cell: Cell {
                c: 'x',
                ..Cell::default()
            },
        });
        let (_, runs) = TerminalElement::layout_grid(
            cells,
            0,
            &TextStyle::default(),
            None,
            1.,
            &Palette::default(),
        );
        assert_eq!(runs.len(), 1, "80 cells must be one shaped run, not 80");
        assert_eq!(runs[0].text, "x".repeat(80));
        let hidden = zed_terminal::IndexedCell {
            point: Point::new(Line(0), Column(0)),
            cell: Cell {
                c: 'x',
                flags: Flags::HIDDEN,
                ..Cell::default()
            },
        };
        assert!(
            TerminalElement::layout_grid(
                std::iter::once(hidden),
                0,
                &TextStyle::default(),
                None,
                1.,
                &Palette::default()
            )
            .1
            .is_empty()
        );
        let key = Keystroke::parse("ctrl-c").unwrap();
        assert_eq!(
            zed_terminal::mappings::keys::to_esc_str(&key, &TermMode::empty(), true).as_deref(),
            Some("\x03")
        );
        let key = Keystroke::parse("up").unwrap();
        assert_eq!(
            zed_terminal::mappings::keys::to_esc_str(&key, &TermMode::APP_CURSOR, true).as_deref(),
            Some("\x1bOA")
        );
        #[cfg(target_os = "macos")]
        {
            let mode = TermMode::empty();
            let encode = |chord: &str| {
                zed_terminal::mappings::keys::to_esc_str(
                    &Keystroke::parse(chord).unwrap(),
                    &mode,
                    true,
                )
                .map(|s| s.into_owned())
            };
            assert_eq!(encode("cmd-backspace").as_deref(), Some("\x15"));
            assert_eq!(encode("cmd-delete").as_deref(), Some("\x0b"));
            assert_eq!(encode("cmd-left").as_deref(), Some("\x01"));
            assert_eq!(encode("cmd-right").as_deref(), Some("\x05"));
            assert_eq!(encode("alt-left").as_deref(), Some("\x1bb"));
            assert_eq!(encode("alt-right").as_deref(), Some("\x1bf"));
            assert_eq!(encode("alt-delete").as_deref(), Some("\x1bd"));
            assert_eq!(encode("cmd-up"), None);
        }
    }
    #[test]
    fn kitty_protocol_defaults_only_when_unspecified() {
        assert!(should_default_kitty_image_protocol(false, None));
        assert!(!should_default_kitty_image_protocol(true, None));
        assert!(!should_default_kitty_image_protocol(
            false,
            Some(OsStr::new("iterm2"))
        ));
        assert!(!should_default_kitty_image_protocol(
            false,
            Some(OsStr::new("NONE"))
        ));
    }
    #[test]
    fn attach_conflicts_and_ime() {
        assert!(attached_client_issue(true, "error: already has an attached client").is_some());
        assert!(attached_client_issue(false, "already attached").is_none());
        assert!(attached_client_issue(true, "unknown option --takeover").is_none());
        assert_eq!(utf16_slice("a😀中", 1..3), "😀");
    }
}
