//! Native workbench. Blocking RPC and filesystem operations never run in Render.
use crate::fade_label::fade_label;
use crate::i18n::tr;
use crate::inline_edit::{InlineEdit, InlineEditEvent, checked_name as space_name};
use crate::{
    files, git,
    herdr::{self, Device, DeviceKind, Snapshot},
    input::{InputEvent, TextInput},
    layout::{Axis, Direction, SplitTree},
    settings::{self, Settings},
    terminal::{CommandSpec, TerminalView},
    transport::{self, AttachTarget, Connection},
};
use anyhow::{Result, bail};
use gpui::{prelude::*, *};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

actions!(
    workbench,
    [
        Search,
        NewItem,
        NewTerminal,
        NewSpace,
        SubmitSpace,
        OpenFolder,
        CopySpacePath,
        SubmitNewItem,
        OpenSettings,
        ToggleSidebar,
        Close,
        SplitRight,
        SplitDown,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        SwapLeft,
        SwapRight,
        SwapUp,
        SwapDown,
        Widen,
        Narrow,
        Grow,
        Shrink,
        Equalize,
        Escape,
        Refresh,
        FindTerminal,
        FocusNext,
        FocusPrevious,
        NextSession,
        PreviousSession,
        Minimize,
        Fullscreen,
        RenameTitle
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = workbench, no_json)]
pub struct GoSession {
    pub n: u8,
}

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = workbench, no_json)]
pub struct GoSpace {
    pub n: u8,
}
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Workspace,
    Files,
    Settings,
}
#[derive(Clone, Copy, PartialEq)]
enum SettingsPage {
    Appearance,
    Terminal,
    Agents,
    Shortcuts,
    Notifications,
}
#[derive(Clone)]
enum FormKind {
    NewSpace,
    NewItem,
    Device(Option<usize>),
    RenamePane(String, Option<String>, bool),
    Confirm(String, Value),
    AgentPath(String),
    Transfer(files::Direction),
    Attachment,
    TerminalSearch,
    StartServer,
    DeleteDevice(usize),
}
struct Form {
    kind: FormKind,
    title: String,
    message: Option<String>,
    submit: String,
    fields: Vec<(String, Entity<TextInput>)>,
    workspace: Option<String>,
    agent_kind: Option<String>,
    device_form: Option<device_ui::DeviceForm>,
    space_form: Option<space_ui::SpaceForm>,
    item_scope: Option<FocusHandle>,
}
struct SpaceEdit {
    device: String,
    workspace: String,
    generation: u64,
    editor: Entity<InlineEdit>,
    pending: Option<String>,
    _subscription: Subscription,
}

#[cfg(test)]
fn new_space_params(cwd: &str) -> Result<Value> {
    let cwd = cwd.trim();
    let mut params = json!({"focus": false});
    if !cwd.is_empty() {
        herdr::validate(cwd)?;
        params["cwd"] = json!(cwd);
    }
    Ok(params)
}

fn empty_space_label(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(cwd)
        .to_owned()
}

/// Directories dropped from Finder that can each become a space; files and missing paths are ignored.
fn dropped_space_directories(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| path.is_dir())
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum DeviceInputCommand {
    Cut,
    Copy,
    Paste,
    SelectAll,
}
impl DeviceInputCommand {
    fn action(self) -> Box<dyn Action> {
        use gpui_component::input;
        match self {
            Self::Cut => Box::new(input::Cut),
            Self::Copy => Box::new(input::Copy),
            Self::Paste => Box::new(input::Paste),
            Self::SelectAll => Box::new(input::SelectAll),
        }
    }
    fn allowed(self, index: usize) -> bool {
        index < 3 || !matches!(self, Self::Cut | Self::Copy)
    }
}
#[derive(Clone)]
enum UiAction {
    Refresh,
    Search,
    NewItem,
    NewTerminal,
    QuickAgent(String),
    Standalone,
    NewSpace,
    Settings,
    Files,
    Workspace,
    ToggleSidebar,
    ToggleDetails,
    Usage,
    AddDevice,
    EditDevice(usize),
    EditDeviceInput(usize, DeviceInputCommand, bool),
    SelectDevice(usize),
    SelectWorkspace(String),
    AllSpaces,
    ToggleSpaces,
    NewTerminalIn(String),
    DeviceMenu,
    ReconnectDevice,
    ReconnectDeviceAt(usize),
    CloseSpace(String),
    SelectPane(String),
    Attach(String),
    ReconnectView(String),
    Split(Axis),
    Focus(String),
    Detach(String),
    CloseRemote(String),
    RenameWorkspace(String),
    RenamePane(String),
    Up,
    Home,
    Hidden,
    Browse(String),
    BrowseInput,
    SelectFile(String),
    Transfer(files::Direction),
    Attachment,
    TerminalSearch,
    Inspector,
    Submit,
    DeviceTransport(bool),
    DeviceAdvanced,
    Dismiss,
    FormSelectWorkspace(String),
    FormSelectKind(String),
    SettingsPage(SettingsPage),
    Toggle(&'static str),
    Theme(&'static str),
    SaveSetting(&'static str),
    AgentToggle(String),
    AgentCatalog,
    AgentPath(String),
    AgentApplyPath(String),
    AgentExpand(String),
    AgentMove(String, isize),
    AgentSettings,
    UsageRefresh,
    ToggleUsageDetails,
    RetryCursor,
    ToggleCursorUsage,
    MoveWorkspace(String, isize),
    MoveTab(String, isize),
    Confirm(String, Value),
    ConfirmTransfer(files::Direction, files::ConflictPolicy),
    Language(&'static str),
    PickPath(usize, bool),
    BrowseLocal(String),
    SelectLocalFile(String),
    LocalUp,
    LocalHome,
    LocalBrowseInput,
    CheckPi(String),
    DeleteDevice(usize),
    RenameAgent(String),
    Revive(String),
    ForgetRetained(String),
    StartServer,
    CopyCommand,
    CopySpacePath(String),
    CopyLocation(String),
    Equalize,
    StepPane(Axis, bool),
    Neighbor(Direction, bool),
}
struct DeviceState {
    connection: Option<Connection>,
    snapshot: Option<Snapshot>,
    status: String,
    generation: u64,
    loading: bool,
    event_cancel: Option<std::os::unix::net::UnixStream>,
    event_generation: u64,
    refresh_pending: bool,
    manifests: Vec<Value>,
    event_panes: Vec<String>,
    catalog: Vec<String>,
    catalog_paths: HashMap<String, String>,
}
impl DeviceState {
    fn accepts_event(&self, connection: u64, subscription: u64) -> bool {
        self.generation == connection && self.event_generation == subscription
    }
}
fn close_terminal_action(device: &str, id: &str) -> UiAction {
    match id.strip_prefix(&format!("{device}:")) {
        Some(pane) => UiAction::CloseRemote(pane.into()),
        None => UiAction::Detach(id.into()),
    }
}
#[derive(Default)]
struct ScopeView {
    split: Option<SplitTree>,
    active: Option<String>,
    selected: Option<String>,
}
fn exchange_scope(
    views: &mut HashMap<(String, Option<String>), ScopeView>,
    from: (String, Option<String>),
    to: (String, Option<String>),
    current: ScopeView,
) -> ScopeView {
    views.insert(from, current);
    views.remove(&to).unwrap_or_default()
}
struct PaneView {
    title: String,
    terminal: Entity<TerminalView>,
    command: CommandSpec,
    _connection: Option<Connection>,
}
enum Worker {
    LocalCatalog(HashMap<String, String>, Vec<app_catalog::InstalledAgent>),
    ProjectIcons(Vec<(String, crate::project_icons::ProjectIcon)>),
    Connected(String, u64, Result<(Connection, Snapshot)>),
    Snapshot(String, u64, Result<Snapshot>),
    Mutation(String, u64, Result<Value>),
    TitleRenamed(EntityId, Result<Value>),
    SpaceRenamed(EntityId, Result<()>),
    Command(String, String, u64, Option<Connection>, Result<CommandSpec>),
    Files(u64, bool, Result<files::DirectoryListing>),
    PiReady(String, String, Result<bool>),
    Inspector(u64, Result<Value>),
    Started(Result<()>),
    Progress(u64, u64),
    Transfer(Result<String>),
    Attachment(String, Result<String>),
    ImageAttachment(String, Result<(ImageFormat, Vec<u8>)>),
    Event(String, u64, u64, Result<Value>),
    EventReady(String, u64, u64, std::os::unix::net::UnixStream),
    Catalog(
        String,
        u64,
        Result<(Value, Vec<String>, HashMap<String, String>)>,
    ),
    Loaded(Result<(Settings, Vec<Device>)>),
    Git(u64, String, Option<git::GitRepositoryInfo>),
}
#[derive(Clone, Copy)]
struct Palette {
    bg: u32,
    side: u32,
    raised: u32,
    active: u32,
    text: u32,
    muted: u32,
    line: u32,
    accent: u32,
    titlebar: u32,
    title_hover: u32,
    title_text: u32,
    tooltip_border: u32,
    tooltip_text: u32,
}
impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                bg: 0x1a1c1f,
                side: 0x212327,
                raised: 0x23252a,
                active: 0x2d2f34,
                text: 0xdde0e4,
                muted: 0xaaafbb,
                line: 0x383b42,
                accent: 0x91b4e8,
                titlebar: 0x1e2023,
                title_hover: 0x282a2f,
                title_text: 0xaaafbb,
                tooltip_border: 0x2a2c31,
                tooltip_text: 0xdde0e4,
            }
        } else {
            Self {
                bg: 0xfafafa,
                side: 0xebebec,
                raised: 0xebebec,
                active: 0xdadbdd,
                text: 0x202226,
                muted: 0x686e78,
                line: 0xd5d6d8,
                accent: 0x5279b7,
                titlebar: 0xf4f4f4,
                title_hover: 0xdfdfe0,
                title_text: 0x58585a,
                tooltip_border: 0xdfdfe0,
                tooltip_text: 0x242529,
            }
        }
    }

    fn apply_component_theme(&self, cx: &mut App) {
        let theme = gpui_component::Theme::global_mut(cx);
        let bg = Hsla::from(rgb(self.bg));
        let side = Hsla::from(rgb(self.side));
        let raised = Hsla::from(rgb(self.raised));
        let active = Hsla::from(rgb(self.active));
        let text = Hsla::from(rgb(self.text));
        let muted = Hsla::from(rgb(self.muted));
        let line = Hsla::from(rgb(self.line));
        let accent = Hsla::from(rgb(self.accent));
        let titlebar = Hsla::from(rgb(self.titlebar));
        theme.background = bg;
        theme.accordion = bg;
        theme.list = bg;
        theme.table = bg;
        theme.tiles = bg;
        theme.sidebar = side;
        theme.title_bar = titlebar;
        theme.popover = raised;
        theme.group_box = raised;
        theme.tab_bar = side;
        theme.tab_bar_segmented = active;
        theme.tab = raised;
        theme.tab_active = bg;
        theme.foreground = text;
        theme.popover_foreground = text;
        theme.sidebar_foreground = text;
        theme.tab_foreground = muted;
        theme.tab_active_foreground = text;
        theme.secondary_foreground = text;
        theme.muted_foreground = muted;
        theme.muted = active;
        theme.secondary = raised;
        theme.secondary_hover = active;
        theme.secondary_active = line;
        theme.accent = active;
        theme.accent_foreground = text;
        theme.list_hover = active;
        theme.list_active = active;
        theme.list_active_border = line;
        theme.list_even = raised;
        theme.list_head = raised;
        theme.table_hover = active;
        theme.table_active = active;
        theme.table_active_border = line;
        theme.table_even = raised;
        theme.table_head = raised;
        theme.sidebar_accent = active;
        theme.sidebar_accent_foreground = text;
        theme.skeleton = raised;
        theme.border = line;
        theme.input = line;
        theme.title_bar_border = line;
        theme.sidebar_border = line;
        theme.table_row_border = line;
        theme.window_border = line;
        theme.caret = accent;
        theme.ring = accent;
        theme.link = accent;
        theme.link_hover = accent;
        theme.link_active = muted;
        theme.drag_border = accent;
        theme.selection = accent.opacity(0.35);
        theme.primary = accent;
        theme.primary_foreground = bg;
        theme.primary_hover = accent.opacity(0.85);
        theme.primary_active = accent;
        theme.sidebar_primary = accent;
        theme.sidebar_primary_foreground = bg;
        theme.progress_bar = accent;
        theme.slider_bar = accent;
        theme.slider_thumb = text;
        theme.switch = accent;
        theme.blue = accent;
        theme.blue_light = accent.opacity(0.2);
    }
}

pub struct AppView {
    focus: FocusHandle,
    feedback_focus: FocusHandle,
    title_focus: FocusHandle,
    title_edit: Option<title_ui::TitleEdit>,
    space_edit: Option<SpaceEdit>,
    feedback_previous_focus: Option<FocusHandle>,
    screen: Screen,
    settings_page: SettingsPage,
    settings: Settings,
    devices: Vec<Device>,
    states: HashMap<String, DeviceState>,
    project_icons: HashMap<String, (Instant, crate::project_icons::ProjectIcon)>,
    project_icons_loading: bool,
    selected_device: usize,
    workspace: Option<String>,
    selected_pane: Option<String>,
    terminals: HashMap<String, PaneView>,
    scope_views: HashMap<(String, Option<String>), ScopeView>,
    split: Option<SplitTree>,
    active_terminal: Option<String>,
    last_terminal_theme: Option<(bool, Option<EntityId>)>,
    next_terminal: u64,
    next_command: u64,
    pending_commands: HashMap<String, u64>,
    dragging: Option<(Vec<bool>, Axis, Point<Pixels>, f32)>,
    sidebar: bool,
    sidebar_width: f32,
    sidebar_resizing: bool,
    spaces_expanded: bool,
    object_drop: Option<(String, bool)>,
    object_drag_in_tree: bool,
    folder_drop: bool,
    sidebar_actions_hidden: [bool; 4],
    menu: Option<(Point<Pixels>, Vec<(String, UiAction)>, Option<usize>)>,
    menu_previous_focus: Option<FocusHandle>,
    menu_focus: FocusHandle,
    menu_scroll: ScrollHandle,
    device_picker: bool,
    device_picker_index: usize,
    device_trigger_bounds: Option<Bounds<Pixels>>,
    device_picker_scroll: ScrollHandle,
    usage_trigger_bounds: Option<Bounds<Pixels>>,
    details: bool,
    error: Option<String>,
    notice: Option<String>,
    form: Option<Form>,
    search: Option<Entity<TextInput>>,
    search_index: usize,
    search_previous_focus: Option<FocusHandle>,
    fields: HashMap<String, Entity<TextInput>>,
    field_subscriptions: HashMap<String, Subscription>,
    subscriptions: Vec<Subscription>,
    tx: Sender<Worker>,
    rx: Receiver<Worker>,
    file_path: String,
    file_entries: Vec<files::FileEntry>,
    file_selected: Option<String>,
    file_hidden: bool,
    file_loading: bool,
    file_read: Option<Result<(), String>>,
    file_generation: u64,
    local_path: String,
    local_entries: Vec<files::FileEntry>,
    local_selected: Option<String>,
    local_loading: bool,
    local_read: Option<Result<(), String>>,
    local_generation: u64,
    unread: HashSet<(String, String)>,
    priority: priority::PriorityState,
    pending_transfer: Option<(files::Direction, PathBuf, String)>,
    transfer: Option<files::CancelToken>,
    transfer_progress: Option<(u64, u64)>,
    pending_attach: Option<(String, String)>,
    pi_waiting: Option<(String, String)>,
    inspector: Option<Value>,
    inspector_input: Option<Entity<TextInput>>,
    inspector_generation: u64,
    usage: crate::usage::UsageService,
    show_usage: bool,
    // ponytail: shared in-memory details; persist per provider only if requested.
    show_usage_details: bool,
    expanded_agent: Option<String>,
    git_metadata: Option<git::GitRepositoryInfo>,
    git_key: Option<String>,
    git_inflight: Option<String>,
    git_generation: u64,
    git_next_at: Instant,
    _tick: Task<()>,
}
impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::notifications::init();
        Self::bind(cx);
        let focus = cx.focus_handle();
        focus.focus(window);
        let (tx, rx) = mpsc::channel();
        let init_tx = tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<_> {
                let settings = Settings::load()?;
                let mut devices = settings::load_devices()?;
                if let Some(home) = std::env::var_os("HOME") {
                    for device in herdr::discover_named_sessions(&PathBuf::from(home))? {
                        if !devices.iter().any(|d| d.id == device.id) {
                            devices.push(device);
                        }
                    }
                }
                Ok((settings, devices))
            })();
            let _ = init_tx.send(Worker::Loaded(result));
        });
        let tick = cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if cx
                    .update(|window, cx| view.update(cx, |view, cx| view.drain(window, cx)))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            focus,
            feedback_focus: cx.focus_handle(),
            title_focus: cx.focus_handle(),
            title_edit: None,
            space_edit: None,
            feedback_previous_focus: None,
            screen: Screen::Workspace,
            settings_page: SettingsPage::Appearance,
            settings: Settings::default(),
            devices: vec![Device::local()],
            states: HashMap::new(),
            project_icons: HashMap::new(),
            project_icons_loading: false,
            selected_device: 0,
            workspace: None,
            selected_pane: None,
            terminals: HashMap::new(),
            scope_views: HashMap::new(),
            split: None,
            active_terminal: None,
            last_terminal_theme: None,
            next_terminal: 0,
            next_command: 0,
            pending_commands: HashMap::new(),
            dragging: None,
            sidebar: true,
            sidebar_width: 260.,
            sidebar_resizing: false,
            spaces_expanded: settings::read_preference("sidebar.spacesExpanded")
                .ok()
                .flatten()
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            object_drop: None,
            object_drag_in_tree: false,
            folder_drop: false,
            sidebar_actions_hidden: ["newTerminal", "newSpace", "files", "search"].map(|key| {
                settings::read_preference(&format!("sidebar.action.{key}Hidden"))
                    .ok()
                    .flatten()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }),
            menu: None,
            menu_previous_focus: None,
            menu_focus: cx.focus_handle(),
            menu_scroll: ScrollHandle::new(),
            device_picker: false,
            device_picker_index: 0,
            device_trigger_bounds: None,
            device_picker_scroll: ScrollHandle::new(),
            usage_trigger_bounds: None,
            details: false,
            error: None,
            notice: Some("正在读取本地配置…".into()),
            form: None,
            search: None,
            search_index: 0,
            search_previous_focus: None,
            fields: HashMap::new(),
            field_subscriptions: HashMap::new(),
            subscriptions: vec![
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.object_drop = None;
                        this.object_drag_in_tree = false;
                        cx.stop_active_drag(window);
                        cx.notify();
                    }
                }),
                {
                    let view = cx.weak_entity();
                    cx.intercept_keystrokes(move |event, window, cx| {
                        if let Some(view) = view.upgrade() {
                            let _ = view.update(cx, |this, cx| {
                                this.capture_shortcut_if_focused(&event.keystroke, window, cx);
                            });
                        }
                    })
                },
            ],
            tx,
            rx,
            file_path: "~".into(),
            file_entries: vec![],
            file_selected: None,
            file_hidden: false,
            file_loading: false,
            file_read: None,
            file_generation: 0,
            local_path: "~".into(),
            local_entries: vec![],
            local_selected: None,
            local_loading: false,
            local_read: None,
            local_generation: 0,
            unread: HashSet::new(),
            priority: priority::PriorityState::default(),
            pending_transfer: None,
            transfer: None,
            transfer_progress: None,
            pending_attach: None,
            pi_waiting: None,
            inspector: None,
            inspector_input: None,
            inspector_generation: 0,
            usage: crate::usage::UsageService::new(),
            show_usage: false,
            show_usage_details: false,
            expanded_agent: None,
            git_metadata: None,
            git_key: None,
            git_inflight: None,
            git_generation: 0,
            git_next_at: Instant::now(),
            _tick: tick,
        }
    }
    fn bind(cx: &mut Context<Self>) {
        cx.bind_keys([
            KeyBinding::new("enter", SubmitNewItem, Some("NewItemModal")),
            KeyBinding::new("escape", Escape, Some("NewItemModal")),
            KeyBinding::new("escape", Escape, Some("DialogOverlay")),
            KeyBinding::new("escape", Escape, Some("OverlayPopup")),
        ]);
        cx.bind_keys(app_shortcuts::default_bindings());
        crate::set_menus(cx);
    }
    fn device(&self) -> Device {
        self.devices
            .get(self.selected_device)
            .cloned()
            .unwrap_or_else(Device::local)
    }
    fn snapshot(&self) -> Option<&Snapshot> {
        self.states
            .get(&self.device().id)
            .and_then(|s| s.snapshot.as_ref())
    }
    fn begin_space_edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let device = self.device().id;
        if let Some(edit) = &self.space_edit {
            if edit.device == device && edit.workspace == id {
                edit.editor.read(cx).focus(window, cx);
            } else {
                // ponytail: keep one unsaved draft; add per-space drafts only if needed.
                self.error = Some(tr("请先完成或取消当前空间的名称编辑"));
            }
            return;
        }
        let Some(state) = self.states.get(&device) else {
            return;
        };
        let Some(workspace) = state
            .snapshot
            .as_ref()
            .and_then(|s| s.workspaces.iter().find(|w| w.workspace_id == id))
        else {
            return;
        };
        let original = workspace.label.clone();
        let generation = state.generation;
        let editor = cx.new(|cx| InlineEdit::new(&original, tr("名称"), window, cx));
        editor.update(cx, |editor, cx| {
            editor.set_theme(self.is_dark(window), cx);
        });
        let subscription =
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| match event {
                InlineEditEvent::Submit => this.submit_space_edit(window, cx),
                InlineEditEvent::Cancel { restore_focus } => {
                    this.cancel_space_edit(*restore_focus, window, cx)
                }
            });
        editor.read(cx).focus(window, cx);
        self.space_edit = Some(SpaceEdit {
            device,
            workspace: id,
            generation,
            editor,
            pending: None,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn cancel_space_edit(
        &mut self,
        restore_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &self.space_edit else { return };
        let focused = edit.editor.read(cx).is_focused(window, cx);
        // Hide immediately, but retain a submitted request until its result arrives.
        if edit.pending.is_none() {
            self.space_edit = None;
        }
        if restore_focus && focused {
            self.focus.focus(window);
        }
        cx.notify();
    }

    fn submit_space_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> Result<()> {
            let Some(edit) = &self.space_edit else {
                return Ok(());
            };
            if edit.pending.is_some() {
                return Ok(());
            }
            let state = self.states.get(&edit.device);
            if self.device().id != edit.device
                || state.is_none_or(|state| state.generation != edit.generation)
            {
                bail!(tr("空间或连接已变化，请取消后重新编辑名称"));
            }
            let label = space_name(edit.editor.read(cx).text(cx))?;
            if label == edit.editor.read(cx).original() {
                self.cancel_space_edit(true, window, cx);
                return Ok(());
            }
            let retained = self.is_retained_workspace(&edit.workspace);
            let connection = if retained {
                None
            } else {
                Some(self.connection()?)
            };
            let edit = self.space_edit.as_mut().unwrap();
            let device = edit.device.clone();
            let workspace = edit.workspace.clone();
            let id = edit.editor.entity_id();
            edit.pending = Some(label.clone());
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = if let Some(connection) = connection {
                    connection
                        .client
                        .rename_workspace(&workspace, &label)
                        .map(|_| ())
                        .map_err(Into::into)
                } else {
                    app_persistence::rename_retained(&device, &workspace, &label)
                };
                let _ = tx.send(Worker::SpaceRenamed(id, result));
            });
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
        cx.notify();
    }

    fn finish_space_edit(
        &mut self,
        id: EntityId,
        result: Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = self
            .space_edit
            .as_mut()
            .filter(|edit| edit.editor.entity_id() == id)
        else {
            return;
        };
        let Some(sent) = edit.pending.take() else {
            return;
        };
        let device = edit.device.clone();
        let current = self
            .states
            .get(&device)
            .is_some_and(|state| state.generation == edit.generation);
        match result {
            Ok(()) if current => {
                // Retained spaces are local; remote spaces refresh from the server below.
                if let Some(workspace) = self
                    .states
                    .get_mut(&device)
                    .and_then(|state| state.snapshot.as_mut())
                    .and_then(|s| {
                        s.workspaces
                            .iter_mut()
                            .find(|w| w.workspace_id == edit.workspace)
                    })
                    .filter(|w| w.extra.get("retained") == Some(&Value::Bool(true)))
                {
                    workspace.label = sent.clone();
                }
                if edit.editor.update(cx, |editor, cx| editor.saved(sent, cx)) {
                    let focused = edit.editor.read(cx).is_focused(window, cx);
                    self.space_edit = None;
                    if focused {
                        self.focus.focus(window);
                    }
                }
            }
            Ok(()) => {
                edit.editor.update(cx, |editor, cx| editor.failed(cx));
                self.error = Some(tr("空间或连接已变化，请取消后重新编辑名称"));
            }
            Err(error) => {
                edit.editor.update(cx, |editor, cx| editor.failed(cx));
                self.error = Some(error.to_string());
            }
        }
        // An uncertain result is refreshed, never automatically retried.
        self.refresh_device(&device);
        cx.notify();
    }

    fn is_retained_workspace(&self, id: &str) -> bool {
        self.snapshot().is_some_and(|snapshot| {
            snapshot.workspaces.iter().any(|workspace| {
                workspace.workspace_id == id
                    && workspace.extra.get("retained") == Some(&Value::Bool(true))
            })
        })
    }
    fn forget_retained_workspace(&mut self, id: &str) -> Result<()> {
        let device = self.device().id;
        app_persistence::dismiss_space(&device, id)?;
        if let Some(snapshot) = self
            .states
            .get_mut(&device)
            .and_then(|state| state.snapshot.as_mut())
        {
            snapshot.workspaces.retain(|workspace| {
                workspace.workspace_id != id
                    || workspace.extra.get("retained") != Some(&Value::Bool(true))
            });
        }
        Ok(())
    }
    fn create_empty_space(&mut self, cwd: &str) -> Result<String> {
        let cwd = cwd.trim();
        if !cwd.is_empty() {
            herdr::validate(cwd)?;
        }
        let label = if cwd.is_empty() {
            tr("空间")
        } else {
            empty_space_label(cwd)
        };
        let device = self.device().id.clone();
        let sort_index = self
            .snapshot()
            .map(|s| s.workspaces.len() as u64)
            .unwrap_or(0);
        let id = app_persistence::retain_empty_space(
            &device,
            (!cwd.is_empty()).then_some(cwd),
            &label,
            sort_index,
        )?;
        if let Some(snapshot) = self
            .states
            .get_mut(&device)
            .and_then(|state| state.snapshot.as_mut())
        {
            app_persistence::merge_retained_workspaces(&device, snapshot)?;
        }
        self.screen = Screen::Workspace;
        self.switch_scope(self.selected_device, Some(id.clone()));
        if !cwd.is_empty() {
            if let Err(error) = app_persistence::remember_recent_folder(&device, cwd, &label) {
                log::warn!("未能记住最近文件夹：{error}");
            }
        }
        Ok(id)
    }
    fn request_project_icons(&mut self) {
        if self.project_icons_loading {
            return;
        }
        let paths: HashSet<String> = self
            .devices
            .iter()
            .filter(|d| d.is_local())
            .filter_map(|d| self.states.get(&d.id)?.snapshot.as_ref())
            .flat_map(|snapshot| {
                snapshot.workspaces.iter().filter_map(|workspace| {
                    workspace_icon_path(snapshot, &workspace.workspace_id).map(str::to_owned)
                })
            })
            .collect();
        self.project_icons.retain(|path, _| paths.contains(path));
        // ponytail: refresh visible project assets every 60s; add watching only for live logo editing.
        let pending: Vec<_> = paths
            .into_iter()
            .filter(|path| {
                self.project_icons
                    .get(path)
                    .is_none_or(|(loaded, _)| loaded.elapsed() >= Duration::from_secs(60))
            })
            .collect();
        if pending.is_empty() {
            return;
        }
        self.project_icons_loading = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let icons = pending
                .into_iter()
                .map(|path| {
                    let icon = crate::project_icons::resolve(&path);
                    (path, icon)
                })
                .collect();
            let _ = tx.send(Worker::ProjectIcons(icons));
        });
    }
    fn project_icon(&self, device: &Device, workspace: &str, size: f32, color: u32) -> AnyElement {
        let icon = device
            .is_local()
            .then(|| self.states.get(&device.id)?.snapshot.as_ref())
            .flatten()
            .and_then(|snapshot| workspace_icon_path(snapshot, workspace))
            .and_then(|path| self.project_icons.get(path));
        match icon {
            Some((_, icon)) => icon.mark(size, color),
            None => crate::icons::project_type_icon("folder", color)
                .size(px(size))
                .into_any_element(),
        }
    }
    fn selected_git_cwd(&self) -> Option<(String, String)> {
        let pane_id = self.selected_pane.clone()?;
        let snapshot = self.snapshot()?;
        let cwd = snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .and_then(|pane| pane.cwd.clone())
            .or_else(|| {
                snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == pane_id)
                    .and_then(|agent| agent.cwd.clone())
            })?;
        let cwd = cwd.trim();
        if cwd.is_empty() {
            None
        } else {
            Some((pane_id, cwd.to_string()))
        }
    }
    fn git_task_key(&self) -> Option<String> {
        let device = self.device();
        if !device.is_local() {
            return None;
        }
        let (pane_id, cwd) = self.selected_git_cwd()?;
        Some(format!("{}:{}:{}", device.id, pane_id, cwd))
    }
    fn space_label(&self) -> String {
        self.workspace
            .as_ref()
            .and_then(|id| {
                self.snapshot()?
                    .workspaces
                    .iter()
                    .find(|workspace| &workspace.workspace_id == id)
            })
            .map(|workspace| workspace.label.clone())
            .unwrap_or_else(|| {
                self.workspace
                    .clone()
                    .unwrap_or_else(|| tr("工作台").to_string())
            })
    }
    fn session_title(&self) -> String {
        let Some(pane_id) = self.selected_pane.as_deref() else {
            return self
                .active_terminal
                .as_ref()
                .and_then(|id| self.terminals.get(id))
                .map(|pane| pane.title.clone())
                .unwrap_or_else(|| self.space_label());
        };
        let Some(snapshot) = self.snapshot() else {
            return self.space_label();
        };
        snapshot_session_title(snapshot, pane_id)
    }
    fn selected_cwd(&self) -> Option<String> {
        let pane_id = self.selected_pane.as_deref()?;
        let snapshot = self.snapshot()?;
        snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)
            .and_then(|agent| agent.cwd.clone())
            .or_else(|| {
                snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == pane_id)
                    .and_then(|pane| pane.cwd.clone())
            })
            .and_then(|cwd| {
                let cwd = cwd.trim();
                (!cwd.is_empty()).then(|| cwd.to_owned())
            })
    }
    fn request_git_metadata(&mut self, force: bool) {
        let key = self.git_task_key();
        if key.as_ref() != self.git_key.as_ref() {
            self.git_metadata = None;
            self.git_key = None;
        }
        let Some(key) = key else {
            return;
        };
        let due = force || Instant::now() >= self.git_next_at;
        let key_changed = self.git_key.as_ref() != Some(&key);
        if !key_changed && !due {
            return;
        }
        if self.git_inflight.as_ref() == Some(&key) {
            return;
        }
        self.git_generation = self.git_generation.wrapping_add(1);
        let generation = self.git_generation;
        self.git_inflight = Some(key.clone());
        let cwd = self
            .selected_git_cwd()
            .map(|(_, cwd)| cwd)
            .unwrap_or_default();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let info = git::probe_local(&cwd);
            let _ = tx.send(Worker::Git(generation, key, info));
        });
    }
    fn connection(&self) -> Result<Connection> {
        self.states
            .get(&self.device().id)
            .and_then(|s| s.connection.clone())
            .ok_or_else(|| anyhow::anyhow!("设备未连接，请先重新连接"))
    }
    fn is_dark(&self, window: &Window) -> bool {
        self.settings.theme == "dark"
            || (self.settings.theme == "system"
                && matches!(
                    window.appearance(),
                    WindowAppearance::Dark | WindowAppearance::VibrantDark
                ))
    }
    fn palette(&self, window: &Window) -> Palette {
        Palette::new(self.is_dark(window))
    }
    fn sync_terminal_themes(&mut self, window: &Window, cx: &mut Context<Self>) {
        let light = !self.is_dark(window);
        let active = (self.screen == Screen::Workspace)
            .then(|| {
                self.active_terminal
                    .as_ref()
                    .and_then(|id| self.terminals.get(id))
                    .map(|pane| pane.terminal.entity_id())
            })
            .flatten();
        let theme_changed = self
            .last_terminal_theme
            .is_none_or(|(previous, _)| previous != light);
        let active_changed = active.is_some()
            && self.last_terminal_theme.map(|(_, previous)| previous) != Some(active);

        if theme_changed {
            for pane in self.terminals.values() {
                pane.terminal
                    .update(cx, |terminal, cx| terminal.sync_theme(light, true, cx));
            }
        } else if active_changed {
            if let Some(pane) = self
                .active_terminal
                .as_ref()
                .and_then(|id| self.terminals.get(id))
            {
                pane.terminal
                    .update(cx, |terminal, cx| terminal.sync_theme(light, true, cx));
            }
        }
        self.last_terminal_theme = Some((light, active));
    }
    fn sync_selected_pane(&mut self) {
        let prefix = format!("{}:", self.device().id);
        let previous = self.selected_pane.clone();
        self.selected_pane = self
            .active_terminal
            .as_deref()
            .and_then(|id| id.strip_prefix(&prefix))
            .map(str::to_owned);
        self.note_priority_selection(previous.as_deref());
    }
    fn rehome_after_close(&mut self, id: &str) {
        let next = self
            .split
            .as_ref()
            .and_then(|tree| tree.focus_after_close(id));
        if let Some(tree) = self.split.as_mut() {
            if !tree.remove(id) {
                self.split = None;
            }
        }
        if self.active_terminal.as_deref() == Some(id) {
            self.active_terminal = next.or_else(|| {
                self.split
                    .as_ref()
                    .and_then(|tree| tree.leaves().last().cloned())
            });
        }
        self.sync_selected_pane();
    }
    fn discard_remote_pane(
        &mut self,
        device: &str,
        pane: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = format!("{device}:{pane}");
        self.pending_commands.remove(&id);
        self.terminals.remove(&id);
        if self
            .split
            .as_ref()
            .is_some_and(|tree| tree.leaves().contains(&id))
        {
            self.rehome_after_close(&id);
            self.focus_active(window, cx);
        }
        for ((owner, _), view) in &mut self.scope_views {
            if owner != device {
                continue;
            }
            let next = view
                .split
                .as_ref()
                .and_then(|tree| tree.focus_after_close(&id));
            if let Some(tree) = &mut view.split
                && tree.leaves().contains(&id)
            {
                if !tree.remove(&id) {
                    view.split = None;
                }
            }
            if view.active.as_deref() == Some(&id) {
                view.active = next;
            }
            if view.selected.as_deref() == Some(pane) {
                view.selected = None;
            }
        }
        for pending in [&mut self.pending_attach, &mut self.pi_waiting] {
            if pending
                .as_ref()
                .is_some_and(|(d, p)| d == device && p == pane)
            {
                *pending = None;
            }
        }
        if self.device().id == device && self.selected_pane.as_deref() == Some(pane) {
            self.selected_pane = None;
        }
        self.unread.remove(&(device.into(), pane.into()));
    }
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self
            .active_terminal
            .as_ref()
            .and_then(|id| self.terminals.get(id))
        {
            pane.terminal.focus_handle(cx).focus(window);
        } else {
            self.focus.focus(window);
        }
    }
    fn switch_scope(&mut self, device: usize, workspace: Option<String>) {
        if device >= self.devices.len()
            || (device == self.selected_device && workspace == self.workspace)
        {
            return;
        }
        let previous = (self.device().id, self.workspace.clone());
        let target = (self.devices[device].id.clone(), workspace.clone());
        let saved = exchange_scope(
            &mut self.scope_views,
            previous,
            target,
            ScopeView {
                split: self.split.take(),
                active: self.active_terminal.take(),
                selected: self.selected_pane.take(),
            },
        );
        self.selected_device = device;
        self.workspace = workspace;
        self.split = saved.split;
        self.active_terminal = saved.active;
        self.selected_pane = saved.selected;
        self.last_terminal_theme = self.last_terminal_theme.map(|(light, _)| (light, None));
        self.dragging = None;
    }
    fn connect(&mut self) {
        self.connect_device(self.device());
    }
    fn connect_device(&mut self, device: Device) {
        let state = self.states.entry(device.id.clone()).or_insert(DeviceState {
            connection: None,
            snapshot: None,
            status: "未连接".into(),
            generation: 0,
            loading: false,
            event_cancel: None,
            event_generation: 0,
            refresh_pending: false,
            manifests: vec![],
            event_panes: vec![],
            catalog: vec![],
            catalog_paths: HashMap::new(),
        });
        if state.loading {
            return;
        }
        if let Some(cancel) = state.event_cancel.take() {
            let _ = cancel.shutdown(std::net::Shutdown::Both);
        }
        state.generation += 1;
        state.event_generation += 1;
        state.refresh_pending = false;
        let generation = state.generation;
        state.loading = true;
        state.status = "连接中…".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let c = transport::connect(&device)?;
                let s = c.client.snapshot()?;
                Ok((c, s))
            })();
            let _ = tx.send(Worker::Connected(device.id, generation, result));
        });
    }
    fn refresh_named_sessions(&mut self) {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let discovered = match herdr::discover_named_sessions(&PathBuf::from(home)) {
            Ok(devices) => devices,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let live: HashSet<String> = discovered.iter().map(|d| d.id.clone()).collect();
        for device in discovered {
            if !self.devices.iter().any(|d| d.id == device.id) {
                self.connect_device(device.clone());
                self.devices.push(device);
            }
        }
        let current = self.device().id.clone();
        self.devices
            .retain(|d| !d.is_named_session() || live.contains(&d.id));
        if let Some(index) = self.devices.iter().position(|d| d.id == current) {
            self.selected_device = index;
        } else {
            self.selected_device = self
                .devices
                .iter()
                .position(|d| d.is_local() && !d.is_named_session())
                .unwrap_or(0);
            self.workspace = None;
            self.selected_pane = None;
        }
    }
    fn reconnect_failed_devices(&mut self) {
        let devices = self.devices.clone();
        for device in devices {
            let failed = self
                .states
                .get(&device.id)
                .map(|s| s.connection.is_none() && !s.loading)
                .unwrap_or(true);
            if failed {
                self.connect_device(device);
            }
        }
    }
    fn refresh(&mut self) {
        if self.screen == Screen::Files {
            self.load_files_side(self.local_path.clone(), true);
            self.load_files_side(self.file_path.clone(), false);
            return;
        }
        self.refresh_named_sessions();
        self.reconnect_failed_devices();
        let device = self.device();
        if let Some(state) = self.states.get_mut(&device.id) {
            if state.loading {
                return;
            }
            if let Some(connection) = state.connection.clone() {
                let generation = state.generation;
                state.loading = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Worker::Snapshot(
                        device.id,
                        generation,
                        connection.client.snapshot().map_err(Into::into),
                    ));
                });
                return;
            }
        }
        self.connect();
    }
    fn mutate(&mut self, method: String, params: Value) {
        match self.connection() {
            Ok(connection) => {
                let device = self.device();
                let generation = self.states.get(&device.id).map_or(0, |s| s.generation);
                let tx = self.tx.clone();
                if method != "pane.close" && method != "workspace.close" {
                    self.notice = Some("正在执行…".into());
                }
                std::thread::spawn(move || {
                    let creates_tab = method == "tab.create";
                    let closed = if method == "workspace.close" {
                        params.get("workspace_id").cloned()
                    } else {
                        None
                    };
                    let closed_pane = (method == "pane.close")
                        .then(|| params.get("pane_id").cloned())
                        .flatten();
                    let result = connection
                        .client
                        .mutation(&method, params)
                        .map(|mut value| {
                            if creates_tab {
                                value["created_pane"] = value
                                    .pointer("/root_pane/pane_id")
                                    .cloned()
                                    .unwrap_or(Value::Null);
                            }
                            if let Some(pane) = closed_pane {
                                json!({"closed_pane":pane,"response":value})
                            } else if let Some(closed) = closed {
                                json!({"closed_workspace":closed,"response":value})
                            } else {
                                value
                            }
                        })
                        .map_err(Into::into);
                    let _ = tx.send(Worker::Mutation(device.id, generation, result));
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn load_files(&mut self, path: String) {
        self.load_files_side(path, false);
    }
    fn load_files_side(&mut self, path: String, local: bool) {
        if local {
            self.local_generation += 1;
            self.local_loading = true;
        } else {
            self.file_generation += 1;
            self.file_loading = true;
        }
        let generation = if local {
            self.local_generation
        } else {
            self.file_generation
        };
        let device = if local {
            Device::local()
        } else {
            self.device()
        };
        let hidden = self.file_hidden;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Worker::Files(
                generation,
                local,
                files::list_directory(&device, &path, hidden),
            ));
        });
    }
    fn drain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.has_active_drag() {
            let object = self.object_drop.take().is_some();
            if object || self.folder_drop {
                self.object_drag_in_tree = false;
                self.folder_drop = false;
                cx.notify();
            }
        }
        self.runtime_tick(window, cx);
        let mut changed = false;
        while let Ok(message) = self.rx.try_recv() {
            changed = true;
            match message {
                Worker::TitleRenamed(id, result) => self.finish_title_edit(id, result, window, cx),
                Worker::SpaceRenamed(id, result) => self.finish_space_edit(id, result, window, cx),
                Worker::ProjectIcons(icons) => {
                    self.project_icons_loading = false;
                    for (path, icon) in icons {
                        self.project_icons.insert(path, (Instant::now(), icon));
                    }
                }
                Worker::Loaded(result) => match result {
                    Ok((settings, mut devices)) => {
                        self.settings = settings;
                        crate::i18n::set_language(&self.settings.language);
                        crate::set_menus(cx);
                        if let Err(e) = self.apply_shortcuts(cx) {
                            self.error = Some(e.to_string());
                        }
                        if !devices.iter().any(Device::is_local) {
                            devices.insert(0, Device::local());
                        }
                        self.devices = devices;
                        if let Err(e) = self.restore_selection() {
                            self.error = Some(e.to_string());
                        }
                        self.notice = None;
                        self.connect();
                        self.refresh_local_catalog();
                    }
                    Err(e) => {
                        self.error = Some(format!("配置读取失败，原数据未覆盖：{e}"));
                        self.notice = None;
                        self.connect();
                        self.refresh_local_catalog();
                    }
                },
                Worker::Connected(id, generation, result) => {
                    if let Some(state) = self.states.get_mut(&id) {
                        if state.generation != generation {
                            continue;
                        }
                        state.loading = false;
                        match result {
                            Ok((c, mut s)) => {
                                let old = state.snapshot.clone();
                                if let Err(error) =
                                    app_persistence::retain_disappeared(&id, old.as_ref(), &s)
                                {
                                    self.error = Some(error.to_string());
                                }
                                if let Err(error) =
                                    app_persistence::merge_retained_workspaces(&id, &mut s)
                                {
                                    self.error = Some(error.to_string());
                                }
                                app_persistence::preserve_emptied_workspaces(old.as_ref(), &mut s);
                                state.status = format!("已连接 · {}", c.version);
                                state.connection = Some(c);
                                state.snapshot = Some(s);
                            }
                            Err(e) => {
                                state.status = format!("连接失败：{e}");
                                self.error = Some(e.to_string());
                            }
                        }
                    }
                    if self.device().id == id {
                        let workspace = self.snapshot().and_then(|s| {
                            self.workspace
                                .as_ref()
                                .filter(|id| s.workspaces.iter().any(|w| &w.workspace_id == *id))
                                .cloned()
                                .or_else(|| s.workspaces.first().map(|w| w.workspace_id.clone()))
                        });
                        self.switch_scope(self.selected_device, workspace);
                    }
                    if self.device().id == id && self.active_terminal.is_none() {
                        let pane = self.snapshot().and_then(|snapshot| {
                            snapshot
                                .panes
                                .iter()
                                .find(|p| {
                                    Some(&p.pane_id) == self.selected_pane.as_ref()
                                        && Some(&p.workspace_id) == self.workspace.as_ref()
                                })
                                .or_else(|| {
                                    snapshot
                                        .panes
                                        .iter()
                                        .find(|p| Some(&p.workspace_id) == self.workspace.as_ref())
                                })
                                .map(|p| p.pane_id.clone())
                        });
                        if let Some(pane) = pane {
                            self.attach(pane);
                        } else if self.shows_priority_sessions()
                            && self.workspace.as_ref().is_some_and(|workspace| {
                                self.snapshot()
                                    .is_some_and(|s| workspace_has_no_sessions(workspace, s))
                            })
                        {
                            self.dispatch(UiAction::NewTerminal, window, cx);
                        }
                        self.focus_active(window, cx);
                    }
                    self.start_events(&id);
                    self.configure_usage();
                }
                Worker::Snapshot(id, generation, mut result) => {
                    let mut restore_session = None;
                    if self
                        .states
                        .get(&id)
                        .is_some_and(|s| s.generation == generation)
                    {
                        if let Ok(new) = &mut result {
                            let old = self.states.get(&id).and_then(|s| s.snapshot.clone());
                            // Preserve from the unmodified server snapshot, then add local ghosts.
                            if let Err(e) =
                                app_persistence::retain_disappeared(&id, old.as_ref(), new)
                            {
                                self.error = Some(e.to_string());
                            }
                            if let Err(e) = app_persistence::merge_retained_workspaces(&id, new) {
                                self.error = Some(e.to_string());
                            }
                            app_persistence::preserve_emptied_workspaces(old.as_ref(), new);
                            self.notify_snapshot(&id, old.as_ref(), new);
                            let restore_order =
                                if self.device().id == id && self.shows_priority_sessions() {
                                    old.as_ref().map(|old| self.ordered_pane_ids(old))
                                } else {
                                    None
                                };
                            self.observe_priority_snapshot(&id, new);
                            restore_session = if self.device().id == id {
                                old.as_ref().and_then(|old| {
                                    restore_session_after_snapshot(
                                        self.selected_pane.as_deref(),
                                        self.workspace.as_deref(),
                                        old,
                                        new,
                                        restore_order.as_deref(),
                                    )
                                })
                            } else {
                                None
                            };
                            if let Some(old) = &old {
                                for pane in &old.panes {
                                    if !new.panes.iter().any(|p| p.pane_id == pane.pane_id)
                                        && !new.agents.iter().any(|p| p.pane_id == pane.pane_id)
                                    {
                                        self.discard_remote_pane(&id, &pane.pane_id, window, cx);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(state) = self.states.get_mut(&id) {
                        if state.generation != generation {
                            continue;
                        }
                        state.loading = false;
                        match result {
                            Ok(s) => state.snapshot = Some(s),
                            Err(e) => {
                                state.status = format!("控制连接中断：{e}");
                                self.error = Some(e.to_string());
                            }
                        }
                    }
                    if self.device().id == id {
                        if let Some(pane) = restore_session {
                            self.attach(pane);
                            self.focus_active(window, cx);
                        } else if let Some(pane) = self.selected_pane.clone() {
                            let present = self
                                .snapshot()
                                .is_some_and(|s| s.panes.iter().any(|p| p.pane_id == pane));
                            if !present {
                                self.selected_pane = None;
                            }
                        }
                    }
                    let restart = self.states.get(&id).is_some_and(|state| {
                        state
                            .snapshot
                            .as_ref()
                            .map(|s| {
                                s.agents
                                    .iter()
                                    .map(|a| a.pane_id.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                            != state.event_panes
                    });
                    if restart {
                        self.start_events(&id);
                    }
                    if self
                        .states
                        .get_mut(&id)
                        .is_some_and(|s| std::mem::take(&mut s.refresh_pending))
                    {
                        self.refresh_device(&id);
                    }
                }
                Worker::Mutation(id, generation, result) => {
                    if self
                        .states
                        .get(&id)
                        .is_some_and(|s| s.generation == generation)
                    {
                        self.notice = None;
                        match result {
                            Ok(value) => {
                                if let Some(error) =
                                    value.get("partial_error").and_then(Value::as_str)
                                {
                                    self.error = Some(error.into());
                                }
                                if let Some(pane) = value.get("closed_pane").and_then(Value::as_str)
                                {
                                    let full = format!("{id}:{pane}");
                                    let restore = self.device().id == id
                                        && (self.selected_pane.as_deref() == Some(pane)
                                            || self.active_terminal.as_deref()
                                                == Some(full.as_str()));
                                    let next = self
                                        .states
                                        .get(&id)
                                        .and_then(|state| state.snapshot.as_ref())
                                        .and_then(|snapshot| {
                                            let ordered = self
                                                .shows_priority_sessions()
                                                .then(|| self.ordered_pane_ids(snapshot));
                                            next_session_after_close(
                                                pane,
                                                snapshot,
                                                ordered.as_deref(),
                                            )
                                        });
                                    if let Some(snapshot) = self
                                        .states
                                        .get_mut(&id)
                                        .and_then(|state| state.snapshot.as_mut())
                                    {
                                        drop_closed_pane_from_snapshot(snapshot, pane);
                                    }
                                    self.discard_remote_pane(&id, pane, window, cx);
                                    if restore {
                                        if let Some(next) = next {
                                            self.attach(next);
                                            self.focus_active(window, cx);
                                        }
                                    }
                                }
                                if let Some(closed) =
                                    value.get("closed_workspace").and_then(Value::as_str)
                                {
                                    if self.device().id == id
                                        && self.workspace.as_deref() == Some(closed)
                                    {
                                        self.switch_scope(self.selected_device, None);
                                    }
                                    if let Err(e) = app_persistence::dismiss_space(&id, closed) {
                                        self.error = Some(e.to_string());
                                    }
                                }
                                if let Some(workspace) = value
                                    .pointer("/workspace/workspace_id")
                                    .or_else(|| value.get("workspace_id"))
                                    .and_then(Value::as_str)
                                {
                                    if self.device().id == id {
                                        self.switch_scope(
                                            self.selected_device,
                                            Some(workspace.into()),
                                        );
                                    }
                                }
                                if let Some(pane) =
                                    value.get("created_pane").and_then(Value::as_str)
                                {
                                    self.priority.note_starting(&id, pane);
                                    self.pending_attach = Some((id.clone(), pane.into()));
                                    self.pi_waiting = None;
                                    if value.get("pi_ready") == Some(&Value::Bool(false)) {
                                        self.pi_waiting = Some((id.clone(), pane.into()));
                                        self.notice =
                                            Some("Pi 启动尚未就绪，可检查状态或显示终端".into());
                                    }
                                } else if let Some(pane) =
                                    value.pointer("/root_pane/pane_id").and_then(Value::as_str)
                                {
                                    self.priority.note_starting(&id, pane);
                                }
                                self.refresh_device(&id);
                            }
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                }
                Worker::Command(id, title, generation, connection, result) => {
                    if self.pending_commands.get(&id) != Some(&generation) {
                        continue;
                    }
                    self.pending_commands.remove(&id);
                    match result {
                        Ok(command) => {
                            self.open_terminal(id, title, command, connection, window, cx)
                        }
                        Err(e) => {
                            self.rehome_after_close(&id);
                            self.error = Some(e.to_string());
                        }
                    }
                }
                Worker::Files(generation, local, result) => {
                    let expected = if local {
                        self.local_generation
                    } else {
                        self.file_generation
                    };
                    if generation != expected {
                        continue;
                    }
                    if local {
                        self.local_loading = false;
                    } else {
                        self.file_loading = false;
                    }
                    match result {
                        Ok(list) => {
                            if local {
                                self.local_selected = files::keep_selection(
                                    &list.entries,
                                    self.local_selected.as_deref(),
                                );
                                self.local_path = list.path;
                                self.local_entries = list.entries;
                                self.local_read = Some(Ok(()));
                                self.fields.remove("local-path");
                            } else {
                                self.file_selected = files::keep_selection(
                                    &list.entries,
                                    self.file_selected.as_deref(),
                                );
                                self.file_path = list.path;
                                self.file_entries = list.entries;
                                self.file_read = Some(Ok(()));
                                self.fields.remove("file-path");
                            }
                        }
                        Err(e) => {
                            let error = e.to_string();
                            if local {
                                self.local_read = Some(Err(error.clone()));
                            } else {
                                self.file_read = Some(Err(error.clone()));
                            }
                            self.error = Some(error);
                        }
                    }
                }
                Worker::EventReady(id, generation, subscription, cancel) => {
                    if let Some(state) = self.states.get_mut(&id)
                        && state.accepts_event(generation, subscription)
                    {
                        if let Some(connection) = &state.connection {
                            state.status = format!("已连接 · {}", connection.version);
                        }
                        state.event_cancel = Some(cancel);
                    } else {
                        let _ = cancel.shutdown(std::net::Shutdown::Both);
                    }
                }
                Worker::Event(id, generation, subscription, result) => {
                    if self
                        .states
                        .get(&id)
                        .is_some_and(|s| s.accepts_event(generation, subscription))
                    {
                        match result {
                            Ok(_) => self.refresh_device(&id),
                            Err(e) => {
                                if let Some(state) = self.states.get_mut(&id) {
                                    state.status = format!("事件流断开：{e}");
                                }
                            }
                        }
                    }
                }
                Worker::LocalCatalog(overrides, installed) => {
                    if self.apply_local_catalog(overrides, installed) {
                        if let Err(e) = self.apply_shortcuts(cx) {
                            self.error = Some(e.to_string());
                        }
                    }
                }
                Worker::Catalog(id, generation, result) => {
                    let mut local_kinds = None;
                    if let Some(state) = self.states.get_mut(&id) {
                        if state.generation == generation {
                            if let Ok((value, catalog, paths)) = result {
                                state.catalog = catalog;
                                state.catalog_paths = paths;
                                state.manifests = value
                                    .get("manifests")
                                    .or_else(|| value.get("agents"))
                                    .and_then(Value::as_array)
                                    .cloned()
                                    .unwrap_or_default();
                                local_kinds = Some(state.catalog.clone());
                            }
                        }
                    }
                    if id == Device::local().id
                        && local_kinds.is_some_and(publish_installed_agent_kinds)
                    {
                        if let Err(e) = self.apply_shortcuts(cx) {
                            self.error = Some(e.to_string());
                        }
                    }
                    self.configure_usage();
                }
                Worker::ImageAttachment(id, result) => {
                    self.transfer = None;
                    self.transfer_progress = None;
                    self.notice = None;
                    match result {
                        Ok((format, bytes)) => {
                            if let Some(pane) = self.terminals.get(&id) {
                                cx.write_to_clipboard(ClipboardItem::new_image(
                                    &Image::from_bytes(format, bytes),
                                ));
                                if let Err(e) = pane
                                    .terminal
                                    .update(cx, |terminal, cx| terminal.send(&[0x16], cx))
                                {
                                    self.error = Some(e.to_string());
                                }
                            }
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Worker::Attachment(id, result) => {
                    self.transfer = None;
                    self.notice = None;
                    match result {
                        Ok(text) => {
                            if let Some(pane) = self.terminals.get(&id) {
                                if let Err(e) = pane.terminal.update(cx, |t, cx| t.paste(&text, cx))
                                {
                                    self.error = Some(e.to_string());
                                }
                            } else {
                                self.error =
                                    Some("上传已完成，但目标终端已关闭；未向其他终端发送".into());
                            }
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Worker::PiReady(id, pane, result) => {
                    if self.pi_waiting.as_ref() != Some(&(id.clone(), pane.clone())) {
                        continue;
                    }
                    self.notice = None;
                    match result {
                        Ok(true) => {
                            self.pi_waiting = None;
                            self.pending_attach = Some((id.clone(), pane));
                            self.refresh_device(&id);
                        }
                        Ok(false) => {
                            self.notice = Some("Pi 仍未就绪，可显示终端查看真实输出".into())
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Worker::Inspector(generation, result) => {
                    self.inspector_result(generation, result, cx)
                }
                Worker::Started(result) => {
                    self.notice = None;
                    match result {
                        Ok(()) => self.connect(),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Worker::Progress(completed, total) => {
                    self.transfer_progress = Some((completed, total));
                    self.notice = Some(format!(
                        "文件传输：{} / {}",
                        format_bytes(completed),
                        format_bytes(total)
                    ));
                }
                Worker::Transfer(result) => {
                    self.transfer = None;
                    self.transfer_progress = None;
                    self.notice = None;
                    match result {
                        Ok(path) => {
                            self.notice = Some(format!("传输完成：{path}"));
                            self.load_files_side(self.local_path.clone(), true);
                            self.load_files_side(self.file_path.clone(), false);
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Worker::Git(generation, key, info) => {
                    if self.git_inflight.as_ref() == Some(&key) {
                        self.git_inflight = None;
                    }
                    self.git_next_at = Instant::now() + Duration::from_secs(5);
                    if generation == self.git_generation
                        && Some(&key) == self.git_task_key().as_ref()
                    {
                        self.git_metadata = info;
                        self.git_key = Some(key);
                    }
                }
            }
        }
        if let Some(pane) = created_pane_to_attach(
            &self.device().id,
            self.pending_attach.as_ref(),
            self.snapshot(),
        ) {
            if self.pi_waiting.take().is_some() {
                self.notice = Some("Pi 就绪检测超时，请在终端查看启动状态".into());
            }
            self.pending_attach = None;
            self.screen = Screen::Workspace;
            self.attach(pane);
            self.focus_active(window, cx);
            changed = true;
        }
        self.request_project_icons();
        let git_before = (self.git_metadata.clone(), self.git_key.clone());
        self.request_git_metadata(false);
        if git_before != (self.git_metadata.clone(), self.git_key.clone()) {
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
    fn open_terminal(
        &mut self,
        id: String,
        title: String,
        command: CommandSpec,
        connection: Option<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match TerminalView::create(command.clone(), !self.is_dark(window), window, cx) {
            Ok(terminal) => {
                let _ = terminal.update(cx, |t, cx| {
                    cx.notify();
                    let _ = t.set_font(
                        &self.settings.font_name,
                        self.settings.font_size as f32,
                        self.settings.line_spacing as f32,
                    );
                    let _ = t.set_style(self.settings.font_weight, self.settings.mouse_reporting);
                    let _ = t.set_layout(
                        self.settings.terminal_padding_x as f32,
                        self.settings.terminal_padding_y as f32,
                        self.settings.terminal_padding_balance,
                        self.settings.copy_on_select,
                    );
                    t.sync_theme(!self.is_dark(window), false, cx);
                });
                // A worker from a hidden device/space must never steal the current view.
                let activate = self.active_terminal.as_deref() == Some(&id);
                if activate {
                    terminal.focus_handle(cx).focus(window);
                }
                self.terminals.insert(
                    id.clone(),
                    PaneView {
                        title,
                        terminal,
                        command,
                        _connection: connection,
                    },
                );
                if activate {
                    self.active_terminal = Some(id.clone());
                    if self.split.is_none() {
                        self.split = Some(SplitTree::new(id));
                    }
                    self.screen = Screen::Workspace;
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn attach(&mut self, pane_id: String) {
        self.attach_with_takeover(pane_id, false);
    }
    fn attach_with_takeover(&mut self, pane_id: String, takeover: bool) {
        let workspace = self
            .snapshot()
            .and_then(|s| session_workspace_id(s, &pane_id).map(str::to_owned));
        let previous = self.selected_pane.clone();
        if let Some(workspace) = workspace.filter(|_| self.workspace.is_some()) {
            self.switch_scope(self.selected_device, Some(workspace));
        }
        self.selected_pane = Some(pane_id.clone());
        self.note_priority_selection(previous.as_deref());
        self.unread.remove(&(self.device().id, pane_id.clone()));
        if let Err(e) = self.persist_selection() {
            self.error = Some(e.to_string());
        }
        let device = self.device();
        let id = format!("{}:{pane_id}", device.id);
        if self.terminals.contains_key(&id) {
            self.active_terminal = Some(id.clone());
            if !self
                .split
                .as_ref()
                .is_some_and(|tree| tree.leaves().contains(&id))
            {
                self.split = Some(SplitTree::new(id));
            }
            return;
        }
        let Some(pane) = self
            .snapshot()
            .and_then(|s| s.panes.iter().find(|p| p.pane_id == pane_id))
            .cloned()
        else {
            self.error = Some("会话已不在当前快照中，请刷新".into());
            return;
        };
        let title = self
            .snapshot()
            .map(|snapshot| snapshot_session_title(snapshot, &pane.pane_id))
            .unwrap_or_else(|| pane.pane_id.clone());
        match self.connection() {
            Ok(connection) => {
                let tx = self.tx.clone();
                self.next_command += 1;
                let generation = self.next_command;
                self.pending_commands.insert(id.clone(), generation);
                self.active_terminal = Some(id.clone());
                self.split = Some(SplitTree::new(id.clone()));
                let target = if pane.agent.is_some() {
                    AttachTarget::Agent(pane_id)
                } else {
                    AttachTarget::Terminal(pane.terminal_id.unwrap_or(pane_id))
                };
                std::thread::spawn(move || {
                    let _ = tx.send(Worker::Command(
                        id,
                        title,
                        generation,
                        Some(connection.clone()),
                        transport::attach_command(&device, &connection, target, takeover)
                            .map_err(Into::into),
                    ));
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn shell(&mut self, axis: Option<Axis>) {
        let device = self.device();
        let source = self.active_terminal.clone();
        self.next_terminal += 1;
        let id = format!("shell-{}", self.next_terminal);
        self.next_command += 1;
        let generation = self.next_command;
        self.pending_commands.insert(id.clone(), generation);
        if let (Some(axis), Some(tree), Some(active)) = (axis, self.split.as_mut(), source.as_ref())
        {
            tree.split(active, id.clone(), axis);
        } else {
            self.split = Some(SplitTree::new(id.clone()));
        }
        self.active_terminal = Some(id.clone());
        self.selected_pane = None;
        let cwd = split_shell_cwd(
            &device,
            source.as_deref(),
            self.snapshot(),
            self.workspace.as_deref(),
        );
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Worker::Command(
                id,
                "独立终端".into(),
                generation,
                None,
                transport::shell_command(&device, cwd.as_deref()).map_err(Into::into),
            ));
        });
    }
    fn input(
        &mut self,
        key: &str,
        value: String,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        if let Some(input) = self.fields.get(key) {
            return input.clone();
        }
        let input = cx.new(|cx| TextInput::new(value, tr(placeholder), window, cx));
        let field = key.to_owned();
        let subscription = cx.subscribe_in(&input, window, move |this, input, event, _, cx| {
            if matches!(event, InputEvent::Submit) {
                if field == "file-path" {
                    this.load_files_side(input.read(cx).text().to_owned(), false);
                } else if field == "local-path" {
                    this.load_files_side(input.read(cx).text().to_owned(), true);
                }
            }
            cx.notify();
        });
        self.field_subscriptions.insert(key.into(), subscription);
        self.fields.insert(key.into(), input.clone());
        input
    }
    fn show_form(
        &mut self,
        kind: FormKind,
        title: &str,
        fields: Vec<(&str, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = None;
        let fields = fields
            .into_iter()
            .map(|(label, value)| {
                let input = cx.new(|cx| TextInput::new(value, tr(label), window, cx));
                if label.contains("密码") {
                    input.update(cx, |input, cx| input.set_password(true, cx));
                }
                let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                    if matches!(event, InputEvent::Submit) {
                        this.submit(window, cx);
                    }
                    cx.notify();
                });
                self.subscriptions.push(subscription);
                (label.to_owned(), input)
            })
            .collect::<Vec<_>>();
        if let Some((_, input)) = fields.first() {
            input.focus_handle(cx).focus(window);
        } else {
            self.focus.focus(window);
        }
        self.form = Some(Form {
            kind,
            title: title.into(),
            message: None,
            submit: String::new(),
            fields,
            workspace: None,
            agent_kind: None,
            device_form: None,
            space_form: None,
            item_scope: None,
        });
    }
    fn show_confirm(
        &mut self,
        kind: FormKind,
        title: &str,
        message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_form(kind, title, vec![], window, cx);
        if let Some(form) = &mut self.form {
            form.message = Some(message.into());
            form.submit = tr("关闭");
        }
    }
    fn request_close_pane(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self
            .snapshot()
            .and_then(|s| s.panes.iter().find(|p| p.pane_id == id));
        let name = self
            .snapshot()
            .map(|snapshot| snapshot_session_title(snapshot, id))
            .unwrap_or_else(|| tr("终端"));
        let needs_confirm = pane.is_some_and(|p| {
            p.agent.is_some() && matches!(p.agent_status.as_deref(), Some("working" | "blocked"))
        });
        if needs_confirm {
            self.show_confirm(
                FormKind::Confirm("pane.close".into(), json!({"pane_id": id})),
                &crate::i18n::format("关闭 \"{}\"？", &[&name]),
                "此 Agent 仍在运行，将被终止。",
                window,
                cx,
            );
        } else {
            self.mutate("pane.close".into(), json!({"pane_id": id}));
        }
    }
    fn request_close_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_retained_workspace(id) {
            self.dispatch(UiAction::ForgetRetained(id.to_owned()), window, cx);
            return;
        }
        let label = self
            .snapshot()
            .and_then(|s| s.workspaces.iter().find(|w| w.workspace_id == id))
            .map(|w| w.label.clone())
            .unwrap_or_else(|| tr("空间"));
        let empty = self
            .snapshot()
            .map(|s| !s.panes.iter().any(|p| p.workspace_id == id))
            .unwrap_or(true);
        let title = crate::i18n::format("关闭空间 \"{}\" on {}?", &[&label, &self.device().name]);
        self.show_confirm(
            FormKind::Confirm("workspace.close".into(), json!({"workspace_id": id})),
            &title,
            if empty {
                "此空空间将被移除。"
            } else {
                "该空间中的所有终端和 Agent 将被关闭。"
            },
            window,
            cx,
        );
    }
    fn close_command_space(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let retained = self.snapshot().is_some_and(|s| {
            s.workspaces.iter().any(|w| {
                w.workspace_id == id && w.extra.get("retained") == Some(&Value::Bool(true))
            })
        });
        let empty = self
            .snapshot()
            .map(|s| !s.panes.iter().any(|p| p.workspace_id == id))
            .unwrap_or(true);
        if retained {
            self.dispatch(UiAction::ForgetRetained(id.to_owned()), window, cx);
            return;
        }
        if empty {
            self.mutate("workspace.close".into(), json!({"workspace_id": id}));
            return;
        }
        self.request_close_space(id, window, cx);
    }
    fn go_session(&mut self, n: u8, window: &mut Window, cx: &mut Context<Self>) {
        let panes = self
            .sidebar_panes()
            .iter()
            .map(|pane| pane.pane_id.clone())
            .collect::<Vec<_>>();
        if panes.is_empty() {
            return;
        }
        let index = if n == 9 {
            panes.len() - 1
        } else {
            (n as usize).saturating_sub(1)
        };
        if let Some(id) = panes.get(index).cloned() {
            self.dispatch(UiAction::SelectPane(id), window, cx);
        }
    }
    fn cycle_session(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Workspace || self.form.is_some() || self.search.is_some() {
            return;
        }
        let panes = self.sidebar_panes();
        if panes.is_empty() {
            return;
        }
        let current = panes
            .iter()
            .position(|pane| Some(&pane.pane_id) == self.selected_pane.as_ref());
        let index = current.map_or(if next { 0 } else { panes.len() - 1 }, |index| {
            menu_step(index, panes.len(), next)
        });
        let id = panes[index].pane_id.clone();
        self.dispatch(UiAction::SelectPane(id), window, cx);
    }
    fn go_space(&mut self, n: u8, window: &mut Window, cx: &mut Context<Self>) {
        if n == 1 {
            self.dispatch(UiAction::AllSpaces, window, cx);
            return;
        }
        let spaces = self
            .snapshot()
            .map(|s| {
                s.workspaces
                    .iter()
                    .map(|w| w.workspace_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let index = if n == 0 { 8 } else { n as usize - 2 };
        if let Some(id) = spaces.get(index).cloned() {
            self.dispatch(UiAction::SelectWorkspace(id), window, cx);
        }
    }
    fn begin_transfer(
        &mut self,
        direction: files::Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let local = self
            .local_selected
            .clone()
            .unwrap_or_else(|| self.local_path.clone());
        let remote = self
            .file_selected
            .clone()
            .unwrap_or_else(|| self.file_path.clone());
        self.pending_transfer = Some((direction, PathBuf::from(local), remote));
        self.show_form(
            FormKind::Transfer(direction),
            "文件传输",
            vec![],
            window,
            cx,
        );
        if let Some(form) = &mut self.form {
            form.message = Some(tr("冲突策略"));
            form.submit = tr("取消");
        }
    }
    fn run_pending_transfer(&mut self, policy: files::ConflictPolicy) {
        let Some((direction, local, remote)) = self.pending_transfer.take() else {
            return;
        };
        let local = {
            let text = local.display().to_string();
            if let Some(rest) = text.strip_prefix("~/") {
                if let Some(home) = std::env::var_os("HOME") {
                    PathBuf::from(home).join(rest)
                } else {
                    local
                }
            } else if text == "~" {
                std::env::var_os("HOME").map(PathBuf::from).unwrap_or(local)
            } else {
                local
            }
        };
        if !local.is_absolute() {
            self.error = Some(tr("本地路径须为绝对路径"));
            return;
        }
        let token = files::CancelToken::default();
        self.transfer = Some(token.clone());
        let device = self.device();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let last = std::sync::atomic::AtomicU64::new(u64::MAX);
            let result = files::transfer(
                &device,
                &local,
                &remote,
                direction,
                policy,
                &token,
                move |progress| {
                    let percent = if progress.total_bytes == 0 {
                        0
                    } else {
                        progress.completed_bytes.saturating_mul(100) / progress.total_bytes
                    };
                    if last.swap(percent, std::sync::atomic::Ordering::Relaxed) != percent {
                        let _ = progress_tx.send(Worker::Progress(
                            progress.completed_bytes,
                            progress.total_bytes,
                        ));
                    }
                },
            )
            .map(|p| p.display().to_string());
            let _ = tx.send(Worker::Transfer(result));
        });
        self.notice = Some("文件传输中…".into());
    }
    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let space_previous_focus = self
            .form
            .as_ref()
            .and_then(|form| form.space_form.as_ref())
            .and_then(|state| state.previous_focus.clone());
        let component_dialog = self
            .form
            .as_ref()
            .is_some_and(|form| form.device_form.is_some());
        self.form = None;
        self.menu = None;
        self.menu_previous_focus = None;
        self.device_picker = false;
        self.search = None;
        self.show_usage = false;
        self.inspector = None;
        self.inspector_input = None;
        self.inspector_generation += 1;
        self.subscriptions.clear();
        if let Some(focus) = space_previous_focus.or_else(|| self.search_previous_focus.take()) {
            focus.focus(window);
        } else if self.screen != Screen::Workspace {
            self.focus.focus(window);
        } else if let Some(pane) = self
            .active_terminal
            .as_ref()
            .and_then(|id| self.terminals.get(id))
        {
            pane.terminal.focus_handle(cx).focus(window);
        } else {
            self.focus.focus(window);
        }
        self.pi_waiting = None;
        if component_dialog {
            use gpui_component::WindowExt as _;
            if window.has_active_dialog(cx) {
                window.close_dialog(cx);
            }
        }
        cx.notify();
    }
    fn overlay_open(&self) -> bool {
        self.error.is_some()
            || self.menu.is_some()
            || self.device_picker
            || self.form.is_some()
            || self.search.is_some()
            || self.show_usage
            || self.inspector.is_some()
            || self.pi_waiting.is_some()
    }
    pub(crate) fn overlay_ime_composing(&self, cx: &App) -> bool {
        if let Some(form) = &self.form {
            if form
                .fields
                .iter()
                .any(|(_, input)| input.read(cx).is_composing())
            {
                return true;
            }
        }
        self.search
            .as_ref()
            .is_some_and(|input| input.read(cx).is_composing())
    }
    pub(crate) fn dismiss_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch(UiAction::Dismiss, window, cx);
    }
    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.error = Some(format!("设置保存失败：{e}"));
        }
    }
    fn dispatch(&mut self, action: UiAction, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(action, UiAction::Dismiss) && cx.has_active_drag() {
            self.object_drop = None;
            self.object_drag_in_tree = false;
            cx.stop_active_drag(window);
            cx.notify();
            return;
        }
        if self.error.is_some() && matches!(action, UiAction::Dismiss) {
            self.dismiss_feedback(window, cx);
            return;
        }
        if matches!(action, UiAction::Dismiss) {
            if self.menu.is_some() {
                self.close_menu(window, cx);
            } else if self.device_picker {
                self.device_picker = false;
                if self.screen == Screen::Workspace && self.form.is_none() && self.search.is_none()
                {
                    self.focus_active(window, cx);
                }
            } else if self.form.is_some()
                || self.search.is_some()
                || self.show_usage
                || self.inspector.is_some()
                || self.pi_waiting.is_some()
            {
                self.close_overlay(window, cx);
            } else if self.screen == Screen::Settings {
                self.screen = Screen::Workspace;
                self.focus_active(window, cx);
            } else if self.shows_priority_sessions() {
                self.settings.priority_sessions = false;
                self.object_drop = None;
                self.object_drag_in_tree = false;
                self.save_settings();
            } else {
                cx.propagate();
            }
            cx.notify();
            return;
        }
        self.error = None;
        self.close_menu(window, cx);
        if !matches!(&action, UiAction::DeviceMenu) {
            self.device_picker = false;
        }
        match action {
            UiAction::ToggleSpaces => {
                let expanded = !self.spaces_expanded;
                match settings::write_preference("sidebar.spacesExpanded", Some(&json!(expanded))) {
                    Ok(()) => self.spaces_expanded = expanded,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            UiAction::NewTerminalIn(id) => {
                self.switch_scope(self.selected_device, Some(id));
                self.screen = Screen::Workspace;
                match self.persist_selection() {
                    Ok(()) => self.dispatch(UiAction::NewTerminal, window, cx),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            UiAction::AllSpaces => {
                let previous = ScopeView {
                    split: self.split.clone(),
                    active: self.active_terminal.clone(),
                    selected: self.selected_pane.clone(),
                };
                self.switch_scope(self.selected_device, None);
                if self.active_terminal.is_none() {
                    self.split = previous.split;
                    self.active_terminal = previous.active;
                    self.selected_pane = previous.selected;
                }
                self.screen = Screen::Workspace;
                self.focus_active(window, cx);
                if let Err(error) = self.persist_selection() {
                    self.error = Some(error.to_string());
                }
            }
            UiAction::CloseSpace(id) => self.request_close_space(&id, window, cx),
            UiAction::DeviceMenu => {
                self.menu = None;
                self.device_picker = !self.device_picker;
                self.device_picker_index = self.selected_device;
                self.device_picker_scroll
                    .scroll_to_item(self.device_picker_index + 1);
                if self.device_picker {
                    self.focus.focus(window);
                }
                cx.notify();
            }
            UiAction::Refresh => self.refresh(),
            UiAction::ReconnectDevice => self.connect_device(self.device()),
            UiAction::ReconnectDeviceAt(index) => {
                if let Some(device) = self.devices.get(index).cloned() {
                    self.connect_device(device);
                }
            }
            UiAction::ToggleSidebar => self.sidebar = !self.sidebar,
            UiAction::ToggleDetails => self.details = !self.details,
            UiAction::Workspace => {
                self.screen = Screen::Workspace;
                self.focus_active(window, cx);
            }
            UiAction::Files => {
                self.screen = Screen::Files;
                self.load_files_side(self.local_path.clone(), true);
                self.load_files_side(self.file_path.clone(), false);
            }
            UiAction::Settings => {
                if self.screen != Screen::Settings {
                    self.close_overlay(window, cx);
                    self.screen = Screen::Settings;
                    self.fields.clear();
                    self.field_subscriptions.clear();
                    self.focus.focus(window);
                }
                if self.settings_page == SettingsPage::Agents
                    || self.settings_page == SettingsPage::Shortcuts
                {
                    self.refresh_local_catalog();
                }
            }
            UiAction::SettingsPage(page) => {
                self.settings_page = page;
                self.fields.retain(|key, _| key == "settings-search");
                self.field_subscriptions
                    .retain(|key, _| key == "settings-search");
                if page == SettingsPage::Agents || page == SettingsPage::Shortcuts {
                    self.refresh_local_catalog();
                }
            }
            UiAction::SelectDevice(index) => {
                self.switch_scope(index, None);
                if let Err(e) = self.persist_selection() {
                    self.error = Some(e.to_string());
                }
                self.connect();
                self.focus_active(window, cx);
                if self.screen == Screen::Files {
                    self.load_files_side(self.local_path.clone(), true);
                    self.load_files_side("~".into(), false);
                }
            }
            UiAction::SelectWorkspace(id) => {
                self.switch_scope(self.selected_device, Some(id.clone()));
                self.screen = Screen::Workspace;
                if self.active_terminal.is_none() {
                    if let Some(pane) = self
                        .snapshot()
                        .and_then(|s| s.panes.iter().find(|p| p.workspace_id == id))
                        .map(|p| p.pane_id.clone())
                    {
                        self.attach(pane);
                    } else if self.shows_priority_sessions()
                        && self
                            .snapshot()
                            .is_some_and(|s| workspace_has_no_sessions(&id, s))
                    {
                        self.dispatch(UiAction::NewTerminal, window, cx);
                    }
                }
                self.focus_active(window, cx);
                if let Err(e) = self.persist_selection() {
                    self.error = Some(e.to_string());
                }
            }
            UiAction::SelectPane(id) => {
                self.pi_waiting = None;
                self.pending_attach = None;
                self.screen = Screen::Workspace;
                self.attach(id);
                self.focus_active(window, cx);
            }
            UiAction::ReconnectView(id) => {
                if let Some(pane) = self.terminals.remove(&id) {
                    if pane._connection.is_some() {
                        let prefix = format!("{}:", self.device().id);
                        if let Some(pane_id) = id.strip_prefix(&prefix) {
                            let layout = self.split.clone();
                            self.attach(pane_id.to_owned());
                            self.split = layout;
                        }
                    } else {
                        self.active_terminal = Some(id.clone());
                        self.sync_selected_pane();
                        self.open_terminal(id, pane.title, pane.command, None, window, cx);
                    }
                }
            }
            UiAction::Attach(id) => {
                self.pi_waiting = None;
                self.pending_attach = None;
                let full = format!("{}:{id}", self.device().id);
                self.terminals.remove(&full);
                self.attach_with_takeover(id, true);
            }
            UiAction::NewTerminal => {
                if self.prompts_for_new_session_space() {
                    self.dispatch(UiAction::NewItem, window, cx);
                    if let Some(form) = &mut self.form {
                        form.agent_kind = Some("terminal".into());
                    }
                } else if let Some((workspace, revive)) = self.session_space_target() {
                    let space = self.snapshot().and_then(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|item| item.workspace_id == workspace)
                    });
                    let label = space
                        .map(|item| item.label.clone())
                        .unwrap_or_else(|| workspace.clone());
                    let cwd = self
                        .snapshot()
                        .and_then(|snapshot| space_path(snapshot, &workspace));
                    match self.connection() {
                        Ok(connection) => {
                            if revive && self.notice.as_deref() == Some("正在恢复空间…") {
                                return;
                            }
                            let device = self.device();
                            let generation =
                                self.states.get(&device.id).map_or(0, |s| s.generation);
                            let tx = self.tx.clone();
                            std::thread::spawn(move || {
                                let result = app_persistence::open_tab_in_workspace(
                                    &connection.client,
                                    &device.id,
                                    &workspace,
                                    revive,
                                    Some(label.as_str()),
                                    cwd.as_deref(),
                                );
                                let _ = tx.send(Worker::Mutation(device.id, generation, result));
                            });
                            self.notice = Some(
                                if revive {
                                    "正在恢复空间…"
                                } else {
                                    "正在创建…"
                                }
                                .into(),
                            );
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                } else {
                    self.dispatch(UiAction::NewItem, window, cx);
                    if let Some(form) = &mut self.form {
                        form.agent_kind = Some("terminal".into());
                    }
                }
            }
            UiAction::QuickAgent(kind) => self.quick_agent(&kind, window, cx),
            UiAction::Standalone => self.shell(None),
            UiAction::Split(axis) => self.shell(Some(axis)),
            UiAction::Focus(id) => {
                self.active_terminal = Some(id.clone());
                self.sync_selected_pane();
                if let Some(p) = self.terminals.get(&id) {
                    p.terminal.focus_handle(cx).focus(window);
                }
            }
            UiAction::Detach(id) => {
                self.pending_commands.remove(&id);
                self.rehome_after_close(&id);
                self.terminals.remove(&id);
                self.focus_active(window, cx);
            }
            UiAction::CloseRemote(id) => self.request_close_pane(&id, window, cx),
            UiAction::NewSpace => self.show_space_form(window, cx),
            UiAction::NewItem => self.show_new_item_form(window, cx),
            UiAction::RenameWorkspace(id) => self.begin_space_edit(id, window, cx),
            UiAction::RenamePane(id) => {
                if let Some(pane) = self
                    .snapshot()
                    .and_then(|s| s.panes.iter().find(|p| p.pane_id == id))
                {
                    let tab = pane.tab_id.clone();
                    let agent = pane.agent.is_some();
                    let title = self
                        .snapshot()
                        .map(|s| snapshot_session_title(s, &id))
                        .unwrap_or_default();
                    self.show_form(
                        FormKind::RenamePane(id, tab, agent),
                        "重命名会话",
                        vec![("显示名称", title)],
                        window,
                        cx,
                    );
                }
            }
            UiAction::AddDevice => self.show_device_form(None, window, cx),
            UiAction::EditDevice(index) => self.show_device_form(Some(index), window, cx),
            UiAction::EditDeviceInput(index, command, enabled) => {
                if enabled && command.allowed(index) {
                    if let Some(input) = self
                        .form
                        .as_ref()
                        .and_then(|form| form.device_form.as_ref())
                        .and_then(|form| form.inputs.get(index))
                        .cloned()
                    {
                        input.focus_handle(cx).focus(window);
                        window.dispatch_action(command.action(), cx);
                    }
                }
            }
            UiAction::DeviceTransport(tailcat) => {
                if let Some(form) = self.form.as_mut() {
                    if let Some(device) = form.device_form.as_mut() {
                        device.tailcat = tailcat;
                        device.error = None;
                        device.inputs[if tailcat { 4 } else { 1 }]
                            .focus_handle(cx)
                            .focus(window);
                        window.refresh();
                    }
                }
            }
            UiAction::DeviceAdvanced => {
                if let Some(device) = self
                    .form
                    .as_mut()
                    .and_then(|form| form.device_form.as_mut())
                {
                    device.advanced = !device.advanced;
                    window.refresh();
                }
            }
            UiAction::Search => self.open_search(window, cx),
            UiAction::TerminalSearch => {
                if self.screen == Screen::Settings {
                    if let Some(input) = self.fields.get("settings-search") {
                        input.focus_handle(cx).focus(window);
                    }
                } else {
                    self.open_terminal_search(window, cx);
                }
            }
            UiAction::Inspector => self.open_inspector(window, cx),
            UiAction::Submit => self.submit(window, cx),
            UiAction::Dismiss => self.close_overlay(window, cx),
            UiAction::Up => {
                if let Some(path) = files::parent(&self.file_path) {
                    self.load_files_side(path, false);
                }
            }
            UiAction::Home => self.load_files_side("~".into(), false),
            UiAction::Hidden => {
                self.file_hidden = !self.file_hidden;
                self.load_files_side(self.local_path.clone(), true);
                self.load_files_side(self.file_path.clone(), false);
            }
            UiAction::Browse(path) => self.load_files_side(path, false),
            UiAction::BrowseInput => {
                if let Some(input) = self.fields.get("file-path") {
                    self.load_files_side(input.read(cx).text().to_owned(), false);
                }
            }
            UiAction::SelectFile(path) => self.file_selected = Some(path),
            UiAction::BrowseLocal(path) => self.load_files_side(path, true),
            UiAction::SelectLocalFile(path) => self.local_selected = Some(path),
            UiAction::LocalUp => {
                if let Some(path) = files::parent(&self.local_path) {
                    self.load_files_side(path, true);
                }
            }
            UiAction::LocalHome => self.load_files_side("~".into(), true),
            UiAction::LocalBrowseInput => {
                if let Some(input) = self.fields.get("local-path") {
                    self.load_files_side(input.read(cx).text().to_owned(), true);
                }
            }
            UiAction::Transfer(direction) => self.begin_transfer(direction, window, cx),
            UiAction::ConfirmTransfer(direction, policy) => {
                if self.pending_transfer.is_none() {
                    self.begin_transfer(direction, window, cx);
                }
                self.close_overlay(window, cx);
                self.run_pending_transfer(policy);
            }
            UiAction::Attachment => self.show_form(
                FormKind::Attachment,
                "发送附件",
                vec![("本地文件路径", String::new())],
                window,
                cx,
            ),
            UiAction::Theme(theme) => {
                self.settings.theme = theme.into();
                self.save_settings();
                self.sync_terminal_themes(window, cx);
            }
            UiAction::Language(language) => {
                self.settings.language = language.into();
                crate::i18n::set_language(&self.settings.language);
                crate::set_menus(cx);
                self.save_settings();
                self.fields.clear();
            }
            UiAction::Toggle(key) => {
                let previous = self.settings.clone();
                match key {
                    "padding-balance" => {
                        self.settings.terminal_padding_balance =
                            !self.settings.terminal_padding_balance
                    }
                    "copy-on-select" => {
                        self.settings.copy_on_select = !self.settings.copy_on_select
                    }
                    "thin" => self.settings.thin_strokes = !self.settings.thin_strokes,
                    "mouse" => self.settings.mouse_reporting = !self.settings.mouse_reporting,
                    "notifications" => {
                        self.settings.notifications_enabled = !self.settings.notifications_enabled
                    }
                    "sound" => {
                        self.settings.notifications_sound = !self.settings.notifications_sound
                    }
                    "spaces" => self.settings.spaces_hidden = !self.settings.spaces_hidden,
                    "priority" => {
                        self.settings.priority_sessions = !self.settings.priority_sessions;
                        self.object_drop = None;
                        self.object_drag_in_tree = false;
                    }
                    "bypass" => {
                        self.settings.agent_bypass_default = !self.settings.agent_bypass_default
                    }
                    _ => {}
                }
                if let Err(e) = self.settings.save() {
                    self.settings = previous;
                    self.error = Some(format!("设置保存失败：{e}"));
                } else {
                    for pane in self.terminals.values() {
                        let _ = pane.terminal.update(cx, |t, cx| {
                            cx.notify();
                            let _ = t.set_style(
                                self.settings.font_weight,
                                self.settings.mouse_reporting,
                            );
                            let _ = t.set_layout(
                                self.settings.terminal_padding_x as f32,
                                self.settings.terminal_padding_y as f32,
                                self.settings.terminal_padding_balance,
                                self.settings.copy_on_select,
                            );
                            t.sync_theme(!self.is_dark(window), false, cx);
                        });
                    }
                }
            }
            UiAction::SaveSetting(key) => {
                let previous = self.settings.clone();
                let value = self
                    .fields
                    .get(key)
                    .map(|v| v.read(cx).text().to_owned())
                    .unwrap_or_default();
                let result = (|| -> Result<()> {
                    match key {
                        "padding-x" | "padding-y" | "terminal-margin" => {
                            let n: f64 = value.parse()?;
                            if !n.is_finite() || !(0.0..=64.0).contains(&n) {
                                bail!("留白应在 0–64 之间");
                            }
                            match key {
                                "padding-x" => self.settings.terminal_padding_x = n,
                                "padding-y" => self.settings.terminal_padding_y = n,
                                _ => self.settings.terminal_margin = n,
                            }
                        }
                        "unfocused-split-opacity" => {
                            let n: f64 = value.parse()?;
                            if !n.is_finite() || !(0.15..=1.0).contains(&n) {
                                bail!("非焦点分屏透明度应在 0.15–1 之间");
                            }
                            self.settings.unfocused_split_opacity = n;
                        }
                        "font-name" => self.settings.font_name = value,
                        "font-size" => {
                            let n: f64 = value.parse()?;
                            if !(9.0..=22.0).contains(&n) {
                                bail!("字号应在 9–22 之间");
                            }
                            self.settings.font_size = n;
                        }
                        "line-spacing" => {
                            let n: f64 = value.parse()?;
                            if !(1.0..=1.4).contains(&n) {
                                bail!("行距应在 1–1.4 之间");
                            }
                            self.settings.line_spacing = n;
                        }
                        "font-weight" => {
                            let n: f64 = value.parse()?;
                            if !(-1.0..=1.0).contains(&n) {
                                bail!("无效字重");
                            }
                            self.settings.font_weight = n;
                        }
                        "language" => {
                            if !["", "system", "zh-Hans", "en"].contains(&value.as_str()) {
                                bail!("语言应为 system、zh-Hans 或 en");
                            }
                            self.settings.language = value;
                        }
                        "shortcuts" => {
                            let parsed: Value = serde_json::from_str(&value)?;
                            if !parsed.is_object() {
                                bail!("快捷键必须是 JSON 对象");
                            }
                            self.settings.shortcuts = parsed;
                        }
                        _ => {}
                    }
                    self.settings.save()
                })();
                if let Err(e) = result {
                    self.settings = previous;
                    self.error = Some(e.to_string());
                } else {
                    if key == "language" {
                        crate::i18n::set_language(&self.settings.language);
                        crate::set_menus(cx);
                        self.fields.clear();
                        self.notice = None;
                        self.error = None;
                    }
                    for pane in self.terminals.values() {
                        let _ = pane.terminal.update(cx, |t, cx| {
                            cx.notify();
                            let _ = t.set_font(
                                &self.settings.font_name,
                                self.settings.font_size as f32,
                                self.settings.line_spacing as f32,
                            );
                            let _ = t.set_style(
                                self.settings.font_weight,
                                self.settings.mouse_reporting,
                            );
                            let _ = t.set_layout(
                                self.settings.terminal_padding_x as f32,
                                self.settings.terminal_padding_y as f32,
                                self.settings.terminal_padding_balance,
                                self.settings.copy_on_select,
                            );
                            t.sync_theme(!self.is_dark(window), false, cx);
                        });
                    }
                }
            }
            UiAction::AgentCatalog => self.refresh_local_catalog(),
            UiAction::AgentToggle(kind) => {
                if self.settings.disabled_agent_kinds.contains(&kind) {
                    self.settings.disabled_agent_kinds.retain(|v| v != &kind);
                } else {
                    self.settings.disabled_agent_kinds.push(kind.clone());
                }
                self.save_settings();
                bump_disabled_kinds_revision();
                if kind == "cursor"
                    && self
                        .settings
                        .disabled_agent_kinds
                        .iter()
                        .any(|k| k == "cursor")
                {
                    self.expanded_agent = None;
                }
                if self.error.is_none() {
                    if let Err(e) = self.apply_shortcuts(cx) {
                        self.error = Some(e.to_string());
                    } else {
                        crate::set_menus(cx);
                    }
                }
            }
            UiAction::AgentExpand(kind) => {
                self.expanded_agent = if self.expanded_agent.as_deref() == Some(kind.as_str()) {
                    None
                } else {
                    Some(kind)
                };
            }
            UiAction::AgentApplyPath(kind) => {
                let path = self
                    .fields
                    .get(&format!("agent-path:{kind}"))
                    .map(|input| input.read(cx).text().trim().to_owned())
                    .unwrap_or_default();
                match app_catalog::validate_override(&path) {
                    Ok(()) => {
                        let previous = self.settings.agent_binary_overrides.clone();
                        if path.is_empty() {
                            self.settings.agent_binary_overrides.remove(&kind);
                        } else {
                            self.settings.agent_binary_overrides.insert(kind, path);
                        }
                        if let Err(error) = self.settings.save() {
                            self.settings.agent_binary_overrides = previous;
                            self.error = Some(error.to_string());
                        } else {
                            self.refresh_local_catalog();
                        }
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            UiAction::AgentSettings => {
                self.show_usage = false;
                self.screen = Screen::Settings;
                self.settings_page = SettingsPage::Agents;
                self.fields.clear();
                self.refresh_local_catalog();
            }
            UiAction::AgentPath(kind) => {
                let value = self
                    .settings
                    .agent_binary_overrides
                    .get(&kind)
                    .cloned()
                    .unwrap_or_default();
                self.show_form(
                    FormKind::AgentPath(kind),
                    "Agent 可执行文件",
                    vec![("命令名或绝对路径（留空自动检测）", value)],
                    window,
                    cx,
                );
            }
            UiAction::AgentMove(kind, delta) => {
                if self.settings.agent_kind_order.is_empty() {
                    self.settings.agent_kind_order =
                        known_agents().iter().map(|s| s.to_string()).collect();
                }
                if let Some(index) = self
                    .settings
                    .agent_kind_order
                    .iter()
                    .position(|k| k == &kind)
                {
                    let to = (index as isize + delta)
                        .clamp(0, self.settings.agent_kind_order.len() as isize - 1)
                        as usize;
                    self.settings.agent_kind_order.swap(index, to);
                    self.save_settings();
                    if self.error.is_none() {
                        crate::set_menus(cx);
                    }
                }
            }
            UiAction::MoveWorkspace(id, delta) => {
                self.move_object_by(&id, None, delta, cx);
            }
            UiAction::MoveTab(id, delta) => {
                let workspace = self.snapshot().and_then(|s| {
                    s.tabs
                        .iter()
                        .find(|t| t.tab_id == id)
                        .map(|t| t.workspace_id.clone())
                });
                if let Some(workspace) = workspace {
                    self.move_object_by(&id, Some(workspace), delta, cx);
                }
            }
            UiAction::Confirm(method, params) => {
                if method == "workspace.close" {
                    if let Some(id) = params.get("workspace_id").and_then(Value::as_str) {
                        self.request_close_space(id, window, cx);
                    }
                } else if method == "pane.close" {
                    if let Some(id) = params.get("pane_id").and_then(Value::as_str) {
                        self.request_close_pane(id, window, cx);
                    }
                } else {
                    self.show_confirm(
                        FormKind::Confirm(method, params),
                        "关闭",
                        "这会结束服务端对应会话及其进程，不只是关闭本地视图。此操作不可撤销。",
                        window,
                        cx,
                    );
                }
            }
            UiAction::PickPath(index, directories) => {
                if let Some(input) = self
                    .form
                    .as_ref()
                    .and_then(|f| f.fields.get(index))
                    .map(|(_, v)| v.clone())
                {
                    let picker = cx.prompt_for_paths(PathPromptOptions {
                        files: !directories,
                        directories,
                        multiple: false,
                        prompt: Some(tr("选择").into()),
                    });
                    cx.spawn(async move |_, cx| {
                        if let Ok(Ok(Some(paths))) = picker.await {
                            if let Some(path) = paths.first() {
                                let _ = input.update(cx, |input, cx| {
                                    input.set_text(path.display().to_string(), cx)
                                });
                            }
                        }
                    })
                    .detach();
                }
            }
            UiAction::CheckPi(pane) => match self.connection() {
                Ok(connection) => {
                    let device = self.device().id;
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let result = connection
                            .client
                            .wait_for_started_agent(
                                "pi",
                                &pane,
                                &std::sync::atomic::AtomicBool::new(false),
                            )
                            .map_err(Into::into);
                        let _ = tx.send(Worker::PiReady(device, pane, result));
                    });
                    self.notice = Some("正在检查 Pi 启动状态…".into());
                }
                Err(e) => self.error = Some(e.to_string()),
            },
            UiAction::DeleteDevice(index) => self.show_form(
                FormKind::DeleteDevice(index),
                "移除设备",
                vec![],
                window,
                cx,
            ),
            UiAction::RenameAgent(id) => self.show_form(
                FormKind::RenamePane(id, None, true),
                "重命名 Agent 身份",
                vec![("Agent 名称", String::new())],
                window,
                cx,
            ),
            UiAction::Revive(workspace) => match self.connection() {
                Ok(connection) => {
                    if self.notice.as_deref() == Some("正在恢复空间…") {
                        return;
                    }
                    let device = self.device();
                    let generation = self.states.get(&device.id).map_or(0, |s| s.generation);
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let result = app_persistence::revive_retained(
                            &connection.client,
                            &device.id,
                            &workspace,
                        );
                        let _ = tx.send(Worker::Mutation(device.id, generation, result));
                    });
                    self.notice = Some("正在恢复空间…".into());
                }
                Err(e) => self.error = Some(e.to_string()),
            },
            UiAction::ForgetRetained(id) => {
                if self.workspace.as_deref() == Some(&id) {
                    self.switch_scope(self.selected_device, None);
                }
                if let Err(e) = self.forget_retained_workspace(&id) {
                    self.error = Some(e.to_string());
                }
            }
            UiAction::StartServer => self.show_form(
                FormKind::StartServer,
                "启动本机 Herdr 服务",
                vec![("Herdr CLI 绝对路径", "/opt/homebrew/bin/herdr".into())],
                window,
                cx,
            ),
            UiAction::CopyCommand => {
                if let Some(pane) = self
                    .active_terminal
                    .as_ref()
                    .and_then(|id| self.terminals.get(id))
                {
                    let mut parts = vec![quote(&pane.command.program.to_string_lossy())];
                    parts.extend(
                        pane.command
                            .args
                            .iter()
                            .map(|a| quote(&a.to_string_lossy())),
                    );
                    cx.write_to_clipboard(ClipboardItem::new_string(parts.join(" ")));
                }
            }
            UiAction::CopySpacePath(id) => {
                if let Some(path) = self.snapshot().and_then(|s| space_path(s, &id)) {
                    cx.write_to_clipboard(ClipboardItem::new_string(path));
                }
            }
            UiAction::CopyLocation(id) => {
                cx.write_to_clipboard(ClipboardItem::new_string(format!("herdr : {id}")));
            }
            UiAction::Equalize => {
                if let Some(tree) = self.split.as_mut() {
                    tree.equalize();
                }
            }
            UiAction::StepPane(axis, grow) => {
                if let (Some(tree), Some(active)) =
                    (self.split.as_mut(), self.active_terminal.as_ref())
                {
                    tree.resize(active, axis, grow);
                }
            }
            UiAction::Neighbor(direction, swap) => {
                if let (Some(tree), Some(active)) =
                    (self.split.as_mut(), self.active_terminal.clone())
                {
                    if let Some(next) = tree.focus_neighbor(&active, direction) {
                        if swap {
                            tree.swap(&active, &next);
                        } else {
                            self.active_terminal = Some(next.clone());
                            self.sync_selected_pane();
                            if let Some(p) = self.terminals.get(&next) {
                                p.terminal.focus_handle(cx).focus(window);
                            }
                        }
                    }
                }
            }
            UiAction::Usage => {
                self.show_usage = !self.show_usage;
                if self.show_usage {
                    self.usage.refresh();
                }
            }
            UiAction::ToggleUsageDetails => {
                self.show_usage_details = !self.show_usage_details;
            }
            UiAction::UsageRefresh => {
                self.usage.refresh();
            }
            UiAction::RetryCursor => {
                if crate::usage::UsageService::cursor_authorized() {
                    self.usage.refresh();
                }
            }
            UiAction::ToggleCursorUsage => self.toggle_cursor_usage(window, cx),
            UiAction::FormSelectWorkspace(id) => {
                if let Some(form) = &mut self.form {
                    form.workspace = Some(id);
                }
                window.refresh();
            }
            UiAction::FormSelectKind(kind) => {
                if let Some(form) = &mut self.form {
                    form.agent_kind = Some(kind);
                }
                window.refresh();
            }
        }
        cx.notify();
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .form
            .as_ref()
            .is_some_and(|form| form.device_form.is_some())
        {
            if self.try_save_device_dialog(window, cx) {
                self.close_overlay(window, cx);
            }
            return;
        }
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let kind = form.kind.clone();
        let selected_workspace = form.workspace.clone();
        let selected_agent_kind = form.agent_kind.clone();
        let values = form
            .fields
            .iter()
            .map(|(label, v)| {
                if label.contains("密码") {
                    v.read(cx).text().to_owned()
                } else {
                    v.read(cx).text().trim().to_owned()
                }
            })
            .collect::<Vec<_>>();
        let get = |i: usize| values.get(i).cloned().unwrap_or_default();
        let result = (|| -> Result<()> {
            match kind {
                FormKind::NewSpace => {
                    let cwd = self
                        .form
                        .as_ref()
                        .and_then(|form| form.space_form.as_ref())
                        .map(|state| state.input.read(cx).value().to_string())
                        .ok_or_else(|| anyhow::anyhow!("空间表单已关闭"))?;
                    self.create_empty_space(&cwd)?;
                }
                FormKind::NewItem => {
                    let connection = self.connection()?;
                    let space = self.snapshot().and_then(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|item| {
                                Some(item.workspace_id.as_str()) == selected_workspace.as_deref()
                            })
                            .map(|item| {
                                (
                                    item.workspace_id.clone(),
                                    item.label.clone(),
                                    space_path(snapshot, &item.workspace_id),
                                    item.extra.get("retained") == Some(&Value::Bool(true)),
                                )
                            })
                    });
                    let (workspace, space_label, space_cwd, revive) =
                        space.ok_or_else(|| anyhow::anyhow!("请选择空间"))?;
                    let kind = selected_agent_kind
                        .clone()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| anyhow::anyhow!("请选择 Agent"))?;
                    if kind != "terminal" {
                        if self.settings.disabled_agent_kinds.contains(&kind) {
                            bail!("该 Agent 已在设置中禁用");
                        }
                        if self
                            .states
                            .get(&self.device().id)
                            .is_some_and(|s| !s.catalog.is_empty() && !s.catalog.contains(&kind))
                        {
                            bail!("该 Agent 未在此设备的可用目录中，请刷新或检查可执行路径");
                        }
                    }
                    let name = unique_agent_name(
                        &kind,
                        self.snapshot()
                            .map(|s| s.agents.as_slice())
                            .unwrap_or_default(),
                    );
                    let mut args = Vec::new();
                    if self.settings.agent_bypass_default {
                        if let Some(flag) = bypass_flag(&kind) {
                            args.push(flag.into());
                        }
                    }
                    let device = self.device();
                    let generation = self.states.get(&device.id).map_or(0, |s| s.generation);
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let result = (|| {
                            let mut result = app_persistence::open_tab_in_workspace(
                                &connection.client,
                                &device.id,
                                &workspace,
                                revive,
                                Some(space_label.as_str()),
                                space_cwd.as_deref(),
                            )?;
                            if kind != "terminal" {
                                let pane = result
                                    .pointer("/root_pane/pane_id")
                                    .and_then(Value::as_str)
                                    .ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "已创建标签但未返回 pane_id；请刷新，勿重复提交"
                                        )
                                    })?
                                    .to_owned();
                                let started = match connection
                                    .client
                                    .start_agent_when_ready(&name, &kind, &pane, &args)
                                {
                                    Err(herdr::Error::Rpc { code, .. })
                                        if code == "agent_name_taken" =>
                                    {
                                        connection.client.start_agent_when_ready(
                                            &suffixed_agent_name(if valid_agent_name(&kind) {
                                                &kind
                                            } else {
                                                "agent"
                                            }),
                                            &kind,
                                            &pane,
                                            &args,
                                        )
                                    }
                                    other => other,
                                };
                                if let Err(error) = started {
                                    result["created_pane"] = json!(pane);
                                    result["partial_error"] = json!(format!(
                                        "终端已创建，但 Agent 启动未完成：{error}。请在此终端继续，不要重复创建。"
                                    ));
                                    return Ok(result);
                                }
                                if kind == "pi" {
                                    let ready = connection.client.wait_for_started_agent(
                                        "pi",
                                        &pane,
                                        &std::sync::atomic::AtomicBool::new(false),
                                    )?;
                                    result["pi_ready"] = json!(ready);
                                }
                                result["created_pane"] = json!(pane);
                            } else {
                                result["created_pane"] = result
                                    .pointer("/root_pane/pane_id")
                                    .cloned()
                                    .unwrap_or(Value::Null);
                            }
                            Ok(result)
                        })();
                        let _ = tx.send(Worker::Mutation(device.id, generation, result));
                    });
                    self.notice = Some("正在创建…".into());
                }
                FormKind::RenamePane(id, tab, agent) => {
                    if get(0).is_empty() {
                        bail!("名称不能为空");
                    }
                    if let Some(tab) = tab {
                        self.mutate("tab.rename".into(), json!({"tab_id":tab,"label":get(0)}));
                    } else if agent {
                        if !valid_agent_name(&get(0)) {
                            bail!("Agent 名称格式无效");
                        }
                        self.mutate("agent.rename".into(), json!({"target":id,"name":get(0)}));
                    } else {
                        bail!("此 pane 没有可重命名的 tab，请刷新");
                    }
                }
                FormKind::Confirm(method, params) => self.mutate(method, params),
                FormKind::Device(index) => self.save_device_form(index, &values)?,
                FormKind::AgentPath(kind) => {
                    let path = get(0);
                    app_catalog::validate_override(&path)?;
                    let previous = self.settings.agent_binary_overrides.clone();
                    if path.is_empty() {
                        self.settings.agent_binary_overrides.remove(&kind);
                    } else {
                        self.settings.agent_binary_overrides.insert(kind, path);
                    }
                    if let Err(error) = self.settings.save() {
                        self.settings.agent_binary_overrides = previous;
                        return Err(error);
                    }
                    self.refresh_local_catalog();
                }
                FormKind::Transfer(_) => {
                    self.pending_transfer = None;
                }
                FormKind::DeleteDevice(index) => {
                    let device = self
                        .devices
                        .get(index)
                        .ok_or_else(|| anyhow::anyhow!("设备不存在"))?;
                    if device.is_local() {
                        bail!("不能移除固定本机设备");
                    }
                    let id = device.id.clone();
                    let mut devices = self.devices.clone();
                    devices.remove(index);
                    settings::save_devices(&devices)?;
                    self.devices = devices;
                    self.states.remove(&id);
                    self.selected_device = 0;
                    self.workspace = None;
                    self.selected_pane = None;
                }
                FormKind::StartServer => {
                    let path = PathBuf::from(get(0));
                    if !path.is_absolute() || !path.is_file() {
                        bail!("请输入有效 Herdr CLI 绝对路径");
                    }
                    let device = self.device();
                    if !device.is_local() {
                        bail!("仅能启动本机服务");
                    }
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(Worker::Started(
                            transport::start_local_server(&device, &path).map_err(Into::into),
                        ));
                    });
                    self.notice = Some("正在启动本机服务…".into());
                }
                FormKind::TerminalSearch => self.submit_terminal_search(&get(0), cx)?,
                FormKind::Attachment => {
                    let path = PathBuf::from(get(0));
                    if !path.is_file() {
                        bail!("附件必须是本地普通文件");
                    }
                    let device = self.device();
                    let id = self
                        .active_terminal
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("请先打开终端"))?;
                    let expected = self
                        .selected_pane
                        .as_ref()
                        .map(|pane| format!("{}:{pane}", device.id));
                    if expected.as_deref() != Some(&id) {
                        bail!("请在侧栏选中与活动终端对应的 Agent，再发送附件");
                    }
                    let kind = self
                        .selected_pane
                        .as_ref()
                        .and_then(|id| self.snapshot()?.panes.iter().find(|p| &p.pane_id == id))
                        .and_then(|p| p.agent.as_deref())
                        .unwrap_or("");
                    let manifests = self
                        .states
                        .get(&device.id)
                        .map(|s| s.manifests.as_slice())
                        .unwrap_or(&[]);
                    let image_format = image_format(&path);
                    let action =
                        files::attachment_action(manifests, kind, &device, image_format.is_some());
                    if action == files::AttachmentAction::Unsupported {
                        bail!("该 Agent 未声明文件附件能力");
                    }
                    let token = files::CancelToken::default();
                    self.transfer = Some(token.clone());
                    let tx = self.tx.clone();
                    if action == files::AttachmentAction::NativeClipboard {
                        let format =
                            image_format.ok_or_else(|| anyhow::anyhow!("图片格式无法识别"))?;
                        std::thread::spawn(move || {
                            let result = std::fs::read(&path)
                                .map(|bytes| (format, bytes))
                                .map_err(Into::into);
                            let _ = tx.send(Worker::ImageAttachment(id, result));
                        });
                    } else {
                        std::thread::spawn(move || {
                            let _ = tx.send(Worker::Attachment(
                                id,
                                files::attachment_text(&device, &[path], &token),
                            ));
                        });
                    }
                    self.notice = Some("正在准备附件…".into());
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.close_overlay(window, cx),
            Err(e) => {
                if let Some(state) = self.form.as_mut().and_then(|form| form.space_form.as_mut()) {
                    state.error = Some(tr(&e.to_string()));
                    window.refresh();
                } else if self
                    .form
                    .as_ref()
                    .is_some_and(|form| matches!(form.kind, FormKind::NewItem))
                {
                    if let Some(form) = self.form.as_mut() {
                        form.message = Some(tr(&e.to_string()));
                    }
                    window.refresh();
                } else {
                    self.error = Some(e.to_string());
                }
                cx.notify();
            }
        }
    }
    fn refresh_device(&mut self, id: &str) {
        let Some(state) = self.states.get_mut(id) else {
            return;
        };
        if state.loading {
            state.refresh_pending = true;
            return;
        }
        let Some(connection) = state.connection.clone() else {
            return;
        };
        state.loading = true;
        let generation = state.generation;
        let id = id.to_owned();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Worker::Snapshot(
                id,
                generation,
                connection.client.snapshot().map_err(Into::into),
            ));
        });
    }
    fn start_events(&mut self, id: &str) {
        let Some(state) = self.states.get_mut(id) else {
            return;
        };
        // Invalidate the old subscription before deliberately shutting its socket down.
        state.event_generation += 1;
        let subscription = state.event_generation;
        if let Some(cancel) = state.event_cancel.take() {
            let _ = cancel.shutdown(std::net::Shutdown::Both);
        }
        let Some(connection) = state.connection.clone() else {
            return;
        };
        let pane_ids = state
            .snapshot
            .as_ref()
            .map(|s| {
                s.agents
                    .iter()
                    .map(|a| a.pane_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        state.event_panes = pane_ids.clone();
        let local = self
            .devices
            .iter()
            .find(|d| d.id == id)
            .is_some_and(Device::is_local);
        let overrides = self
            .settings
            .agent_binary_overrides
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<_, _>>();
        let generation = state.generation;
        let id = id.to_owned();
        let tx = self.tx.clone();
        let catalog_tx = tx.clone();
        let catalog_id = id.clone();
        let catalog_connection = connection.clone();
        std::thread::spawn(move || {
            let _ = catalog_tx.send(Worker::Catalog(
                catalog_id,
                generation,
                (move || -> Result<_> {
                    let value = catalog_connection.client.agent_manifests()?;
                    let manifests = value
                        .get("manifests")
                        .or_else(|| value.get("agents"))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let advertised = manifests
                        .iter()
                        .filter_map(|v| {
                            v.get("agent")
                                .or_else(|| v.get("kind"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                        .collect::<Vec<_>>();
                    if local {
                        let installed = app_catalog::installed_agents(&advertised, &overrides);
                        Ok((
                            value,
                            installed.iter().map(|v| v.kind.clone()).collect(),
                            installed
                                .iter()
                                .map(|v| (v.kind.clone(), v.path.display().to_string()))
                                .collect(),
                        ))
                    } else {
                        Ok((value, advertised, HashMap::new()))
                    }
                })(),
            ));
        });
        std::thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut events = connection.client.events(&pane_ids)?;
                let cancel = events.cancellation_handle()?;
                if tx
                    .send(Worker::EventReady(
                        id.clone(),
                        generation,
                        subscription,
                        cancel,
                    ))
                    .is_err()
                {
                    return Ok(());
                }
                loop {
                    let event = events.next()?;
                    if tx
                        .send(Worker::Event(
                            id.clone(),
                            generation,
                            subscription,
                            Ok(event),
                        ))
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            })();
            if let Err(error) = result {
                let _ = tx.send(Worker::Event(id, generation, subscription, Err(error)));
            }
        });
    }
    pub(super) fn quick_agent(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompts_for_new_session_space() {
            self.dispatch(UiAction::NewItem, window, cx);
            if let Some(form) = &mut self.form {
                form.agent_kind = Some(kind.to_owned());
            }
            self.focus.focus(window);
            return;
        }
        let Some((workspace, revive)) = self.session_space_target() else {
            self.dispatch(UiAction::NewItem, window, cx);
            if let Some(form) = &mut self.form {
                form.agent_kind = Some(kind.to_owned());
            }
            self.focus.focus(window);
            return;
        };
        if kind == "terminal" {
            self.switch_scope(self.selected_device, Some(workspace));
            self.dispatch(UiAction::NewTerminal, window, cx);
            return;
        }
        if revive && self.notice.as_deref() == Some("正在恢复空间…") {
            return;
        }
        if self.settings.disabled_agent_kinds.iter().any(|k| k == kind) {
            self.error = Some("该 Agent 已在设置中禁用".into());
            cx.notify();
            return;
        }
        let connection = match self.connection() {
            Ok(c) => c,
            Err(e) => {
                self.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        let name = unique_agent_name(
            kind,
            self.snapshot()
                .map(|s| s.agents.as_slice())
                .unwrap_or_default(),
        );
        let mut args = Vec::new();
        if self.settings.agent_bypass_default {
            if let Some(flag) = bypass_flag(kind) {
                args.push(flag.into());
            }
        }
        let (space_label, space_cwd) = self
            .snapshot()
            .map(|snapshot| {
                let label = snapshot
                    .workspaces
                    .iter()
                    .find(|item| item.workspace_id == workspace)
                    .map(|item| item.label.clone())
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| workspace.clone());
                (label, space_path(snapshot, &workspace))
            })
            .unwrap_or_else(|| (workspace.clone(), None));
        let kind = kind.to_owned();
        let device = self.device();
        let generation = self.states.get(&device.id).map_or(0, |s| s.generation);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut result = app_persistence::open_tab_in_workspace(
                    &connection.client,
                    &device.id,
                    &workspace,
                    revive,
                    Some(space_label.as_str()),
                    space_cwd.as_deref(),
                )?;
                let pane = result
                    .pointer("/root_pane/pane_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        anyhow::anyhow!("已创建标签但未返回 pane_id；请刷新，勿重复提交")
                    })?
                    .to_owned();
                let started = match connection
                    .client
                    .start_agent_when_ready(&name, &kind, &pane, &args)
                {
                    Err(herdr::Error::Rpc { code, .. }) if code == "agent_name_taken" => {
                        connection.client.start_agent_when_ready(
                            &suffixed_agent_name(if valid_agent_name(&kind) {
                                &kind
                            } else {
                                "agent"
                            }),
                            &kind,
                            &pane,
                            &args,
                        )
                    }
                    other => other,
                };
                if let Err(error) = started {
                    result["created_pane"] = json!(pane);
                    result["partial_error"] = json!(format!(
                        "终端已创建，但 Agent 启动未完成：{error}。请在此终端继续，不要重复创建。"
                    ));
                    return Ok(result);
                }
                if kind == "pi" {
                    let ready = connection.client.wait_for_started_agent(
                        "pi",
                        &pane,
                        &std::sync::atomic::AtomicBool::new(false),
                    )?;
                    result["pi_ready"] = json!(ready);
                }
                result["created_pane"] = json!(pane);
                Ok(result)
            })();
            let _ = tx.send(Worker::Mutation(device.id, generation, result));
        });
        self.notice = Some(
            if revive {
                "正在恢复空间…"
            } else {
                "正在创建…"
            }
            .into(),
        );
        cx.notify();
    }
    fn btn(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: UiAction,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label: SharedString = label.into();
        self.btn_raw(id, tr(label.as_ref()), action, p, cx)
    }
    fn btn_raw(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: UiAction,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key_action = action.clone();
        let label: SharedString = label.into();
        let selected = match &action {
            UiAction::AllSpaces => self.workspace.is_none(),
            UiAction::SelectWorkspace(id) => self.workspace.as_ref() == Some(id),
            UiAction::SelectPane(id) => self.selected_pane.as_ref() == Some(id),
            _ => true,
        };
        let hover_color = Hsla::from(rgb(p.active)).opacity(if selected { 1. } else { 0.55 });
        div()
            .id(id.into())
            .tab_index(0)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.dispatch(key_action.clone(), window, cx);
                }
            }))
            .focus(move |d| d.bg(rgb(p.active)).border_color(rgb(p.accent)))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_size(px(12.))
            .cursor_pointer()
            .text_color(rgb(p.text))
            .hover(move |s| s.bg(hover_color))
            .active(move |s| s.bg(rgb(p.line)))
            .children((!label.is_empty()).then_some(label))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.click_count() <= 1 {
                    this.dispatch(action.clone(), window, cx);
                }
            }))
    }
    fn icon_btn(
        &self,
        id: impl Into<SharedString>,
        name: &str,
        label: &str,
        action: UiAction,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.icon_btn_color(id, name, label, action, p.muted, p, cx)
    }
    fn icon_btn_color(
        &self,
        id: impl Into<SharedString>,
        name: &str,
        label: &str,
        action: UiAction,
        color: u32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let shortcut = app_shortcuts::shortcut_action(&action);
        self.btn(id, "", action, p, cx)
            .p_0()
            .size(px(24.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(crate::icons::icon(name, color))
            .tooltip({
                let label = tr(label);
                move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone())
                        .when_some(shortcut.as_ref(), |tooltip, action| {
                            tooltip.action(action.as_ref(), Some("Workbench"))
                        })
                        .build(window, cx)
                }
            })
    }
    pub(super) fn centered_overlay(
        &self,
        panel: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .inset_0()
            .pt(px(88.))
            .pb_4()
            .px_4()
            .flex()
            .justify_center()
            .items_start()
            .key_context("OverlayPopup")
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.close_overlay(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.close_overlay(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.close_overlay(window, cx)),
            )
            .child(panel)
            .into_any_element()
    }
    fn shows_priority_sessions(&self) -> bool {
        self.settings.priority_sessions && !self.settings.spaces_hidden
    }
    fn current_session_pane_id(&self) -> Option<&str> {
        let device_prefix = format!("{}:", self.device().id);
        // Sidebar selection is the session the user is in; the focused terminal
        // may be a leftover split or a pane that is not in the snapshot yet.
        self.selected_pane.as_deref().or_else(|| {
            self.active_terminal
                .as_deref()
                .and_then(|id| id.strip_prefix(&device_prefix))
        })
    }
    fn session_space_target(&self) -> Option<(String, bool)> {
        quick_agent_target(
            self.snapshot(),
            self.current_session_pane_id(),
            self.workspace.as_deref(),
        )
    }
    fn prompts_for_new_session_space(&self) -> bool {
        prompts_for_new_session_space(
            self.shows_priority_sessions(),
            self.snapshot(),
            self.current_session_pane_id(),
            self.workspace.as_deref(),
        )
    }
    fn owned_session(&self, device: &str, pane: &herdr::Pane) -> priority::OwnedSession {
        let agent = self.states.get(device).and_then(|state| {
            state.snapshot.as_ref().and_then(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == pane.pane_id)
            })
        });
        let extra = agent.map(|agent| &agent.extra).unwrap_or(&pane.extra);
        priority::OwnedSession {
            device: device.to_owned(),
            pane: pane.pane_id.clone(),
            status: pane
                .agent_status
                .clone()
                .or_else(|| agent.and_then(|agent| agent.agent_status.clone())),
            unread: self
                .unread
                .contains(&(device.to_owned(), pane.pane_id.clone())),
            seq: priority::extra_seq(extra),
            launch_pending: priority::extra_launch_pending(extra),
        }
    }
    fn device_sessions(&self, device: &str) -> Vec<priority::OwnedSession> {
        self.states
            .get(device)
            .and_then(|state| state.snapshot.as_ref())
            .map(|snapshot| {
                snapshot
                    .panes
                    .iter()
                    .map(|pane| self.owned_session(device, pane))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn note_priority_selection(&mut self, previous: Option<&str>) {
        let device = self.device().id.clone();
        let selected = self.selected_pane.clone();
        let enabled = self.shows_priority_sessions();
        let sessions = self.device_sessions(&device);
        self.priority
            .note_selection(enabled, &device, previous, selected.as_deref(), &sessions);
    }
    fn observe_priority_snapshot(&mut self, device: &str, snapshot: &Snapshot) {
        let agents: Vec<priority::OwnedSession> = snapshot
            .agents
            .iter()
            .map(|agent| priority::OwnedSession {
                device: device.to_owned(),
                pane: agent.pane_id.clone(),
                status: agent.agent_status.clone(),
                unread: self
                    .unread
                    .contains(&(device.to_owned(), agent.pane_id.clone())),
                seq: priority::extra_seq(&agent.extra),
                launch_pending: priority::extra_launch_pending(&agent.extra),
            })
            .collect();
        let infos: Vec<_> = agents.iter().map(|session| session.info()).collect();
        self.priority.note_snapshot(device, &infos);
        let live: HashSet<String> = snapshot
            .panes
            .iter()
            .map(|pane| pane.pane_id.clone())
            .chain(snapshot.agents.iter().map(|agent| agent.pane_id.clone()))
            .collect();
        self.priority.prune(device, &live);
    }
    fn ordered_panes<'a>(&'a self, snapshot: &'a Snapshot) -> Vec<&'a herdr::Pane> {
        let enabled = self.shows_priority_sessions();
        let device = self.device().id.clone();
        let panes: Vec<&herdr::Pane> = snapshot
            .panes
            .iter()
            .filter(|pane| {
                enabled
                    || self
                        .workspace
                        .as_ref()
                        .is_none_or(|id| &pane.workspace_id == id)
            })
            .collect();
        let sessions: Vec<_> = panes
            .iter()
            .map(|pane| self.owned_session(&device, pane))
            .collect();
        let infos: Vec<_> = sessions.iter().map(|session| session.info()).collect();
        self.priority
            .order(enabled, &infos)
            .into_iter()
            .map(|index| panes[index])
            .collect()
    }
    fn ordered_pane_ids(&self, snapshot: &Snapshot) -> Vec<String> {
        self.ordered_panes(snapshot)
            .into_iter()
            .map(|pane| pane.pane_id.clone())
            .collect()
    }
    fn sidebar_panes(&self) -> Vec<&herdr::Pane> {
        match self.snapshot() {
            Some(snapshot) => self.ordered_panes(snapshot),
            None => Vec::new(),
        }
    }
    fn render_sidebar(&self, p: Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let dark = self.is_dark(window);
        let p = if dark {
            Palette {
                side: 0x272729,
                active: 0x323334,
                text: 0xe2e2e2,
                muted: 0x7d7d7d,
                ..p
            }
        } else {
            p
        };
        let device_id = self.device().id;
        let device_view = cx.entity();
        let mut view = div()
            .id("sidebar")
            .w(px(self.sidebar_width))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(p.side))
            .child(
                div()
                    .h(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .pl(px(78.))
                    .pr_2()
                    .gap_1()
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    })
                    .child(self.icon_btn(
                        "hide-sidebar",
                        "sidebar",
                        "隐藏侧栏",
                        UiAction::ToggleSidebar,
                        p,
                        cx,
                    ))
                    .when(!self.sidebar_actions_hidden[3], |d| {
                        d.child(self.icon_btn(
                            "sidebar-search",
                            "search",
                            "搜索",
                            UiAction::Search,
                            p,
                            cx,
                        ))
                    }),
            );
        let mut quick = div().px_2().pt_2().flex().flex_col().gap_1();
        for (i, (id, icon, title, action)) in [
            ("new", "terminal", "新终端", UiAction::NewTerminal),
            ("new-space", "plus", "新建空间", UiAction::NewSpace),
            ("files", "folder", "文件", UiAction::Files),
            ("search", "search", "搜索", UiAction::Search),
        ]
        .into_iter()
        .enumerate()
        {
            if i != 3 && !self.sidebar_actions_hidden[i] {
                let shortcut = app_shortcuts::shortcut_action(&action).and_then(|action| {
                    gpui_component::kbd::Kbd::binding_for_action(
                        action.as_ref(),
                        Some("Workbench"),
                        window,
                    )
                });
                let entry = self
                    .btn(id, "", action, p, cx)
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(crate::icons::icon(icon, p.muted))
                    .child(div().flex_1().child(tr(title)))
                    .children(shortcut);
                quick = quick.child(entry);
            }
        }
        view = view.child(quick);
        let mut tree = div()
            .id("tree-scroll")
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ObjectDrag>, _, _| {
                    this.object_drag_in_tree = event.bounds.contains(&event.event.position);
                }),
            )
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_2()
            .py_2();
        if let Some(snapshot) = self.snapshot() {
            let count_width = (snapshot.panes.len().to_string().len() as f32 * 8.).max(24.);
            if !self.settings.spaces_hidden {
                let priority = self.shows_priority_sessions();
                tree = tree.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pr_2()
                        .h(px(28.))
                        .when(!priority, |d| d.mb_2())
                        .child(if priority {
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .px_2()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(p.accent))
                                .child(tr("优先处理"))
                                .into_any_element()
                        } else {
                            self.btn("spaces-expanded", "", UiAction::ToggleSpaces, p, cx)
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_color(rgb(p.muted))
                                .child("Space")
                                .child(
                                    crate::icons::icon(
                                        if self.spaces_expanded {
                                            "chevron_down"
                                        } else {
                                            "chevron_right"
                                        },
                                        p.muted,
                                    )
                                    .size(px(12.)),
                                )
                                .into_any_element()
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .h(px(24.))
                                .child(if priority {
                                    self.icon_btn(
                                        "space-add",
                                        "plus",
                                        "新建会话",
                                        UiAction::NewItem,
                                        p,
                                        cx,
                                    )
                                    .into_any_element()
                                } else {
                                    self.icon_btn(
                                        "space-add",
                                        "plus",
                                        "新建空间",
                                        UiAction::NewSpace,
                                        p,
                                        cx,
                                    )
                                    .into_any_element()
                                })
                                .child({
                                    let fill = Hsla::from(rgb(p.accent)).opacity(0.22);
                                    let hover = Hsla::from(rgb(p.accent)).opacity(0.32);
                                    self.icon_btn_color(
                                        "priority",
                                        if priority { "bell_fill" } else { "bell" },
                                        "优先会话",
                                        UiAction::Toggle("priority"),
                                        if priority { p.accent } else { p.muted },
                                        p,
                                        cx,
                                    )
                                    .size(px(24.))
                                    .flex_shrink_0()
                                    .when(priority, |d| {
                                        d.bg(fill)
                                            .rounded(px(6.))
                                            .border_0()
                                            .focus(move |s| s.bg(fill))
                                            .hover(move |s| s.bg(hover))
                                    })
                                }),
                        ),
                );
                if self.spaces_expanded && !priority {
                    let (status, unread) =
                        sidebar_group_status(snapshot.panes.iter().map(|pane| {
                            (
                                pane.agent_status.as_deref(),
                                self.unread
                                    .contains(&(device_id.clone(), pane.pane_id.clone())),
                            )
                        }));
                    tree = tree.child(
                        self.btn("all-spaces", "", UiAction::AllSpaces, p, cx)
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(32.))
                            .rounded(px(8.))
                            .text_size(px(13.))
                            .mb_1()
                            .when(self.workspace.is_none(), |d| d.bg(rgb(p.active)))
                            .child(crate::icons::icon("spaces", p.muted))
                            .child(div().flex_1().child(tr("全部空间")))
                            .child(crate::sidebar_status::indicator(
                                "all-spaces-status".into(),
                                status,
                                unread,
                                dark,
                            ))
                            .child(
                                div()
                                    .w(px(count_width))
                                    .text_center()
                                    .flex_shrink_0()
                                    .text_color(rgb(p.muted))
                                    .child(snapshot.panes.len().to_string()),
                            ),
                    );
                    for workspace in &snapshot.workspaces {
                        let id = workspace.workspace_id.clone();
                        let retained = workspace.extra.get("retained") == Some(&Value::Bool(true));
                        let drop_id = id.clone();
                        let hover_id = id.clone();
                        let rename = id.clone();
                        let menu_id = id.clone();
                        let count = snapshot
                            .panes
                            .iter()
                            .filter(|pane| pane.workspace_id == id)
                            .count();
                        let (status, unread) = sidebar_group_status(
                            snapshot
                                .panes
                                .iter()
                                .filter(|pane| pane.workspace_id == id)
                                .map(|pane| {
                                    (
                                        pane.agent_status.as_deref(),
                                        self.unread
                                            .contains(&(device_id.clone(), pane.pane_id.clone())),
                                    )
                                }),
                        );
                        if let Some(edit) = self.space_edit.as_ref().filter(|edit| {
                            edit.device == device_id
                                && edit.workspace == id
                                && !edit.editor.read(cx).is_dismissed()
                        }) {
                            tree = tree.child(
                                div()
                                    .id(SharedString::from(format!("workspace-{id}")))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .h(px(32.))
                                    .mb_1()
                                    .rounded(px(8.))
                                    .text_size(px(13.))
                                    .when(self.workspace.as_ref() == Some(&id), |d| {
                                        d.bg(rgb(p.active))
                                    })
                                    .child(self.project_icon(&self.device(), &id, 16., p.muted))
                                    .child(div().flex_1().min_w_0().child(edit.editor.clone()))
                                    .child(crate::sidebar_status::indicator(
                                        format!("space-status-{id}").into(),
                                        status,
                                        unread,
                                        dark,
                                    ))
                                    .child(
                                        div()
                                            .w(px(count_width))
                                            .text_center()
                                            .flex_shrink_0()
                                            .text_color(rgb(p.muted))
                                            .child(count.to_string()),
                                    ),
                            );
                            continue;
                        }
                        let key_rename = id.clone();
                        let drag = ObjectDrag {
                            device: device_id.clone(),
                            id: id.clone(),
                            workspace: None,
                            label: workspace.label.clone(),
                            palette: p,
                        };
                        tree = tree.child(
                            self.btn_raw(
                                format!("workspace-{id}"),
                                "",
                                UiAction::SelectWorkspace(id.clone()),
                                p,
                                cx,
                            )
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(32.))
                            .rounded(px(8.))
                            .text_size(px(13.))
                            .mb_1()
                            .when(self.workspace.as_ref() == Some(&id), |d| {
                                d.bg(rgb(p.active))
                            })
                            .on_key_down(cx.listener(
                                move |this, event: &KeyDownEvent, window, cx| {
                                    if event.keystroke.key == "f2" {
                                        cx.stop_propagation();
                                        this.dispatch(
                                            UiAction::RenameWorkspace(key_rename.clone()),
                                            window,
                                            cx,
                                        );
                                    }
                                },
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    if event.click_count == 2 {
                                        cx.stop_propagation();
                                        this.dispatch(
                                            UiAction::RenameWorkspace(rename.clone()),
                                            window,
                                            cx,
                                        );
                                    }
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    let mut rows = vec![(
                                        tr("重命名"),
                                        UiAction::RenameWorkspace(menu_id.clone()),
                                    )];
                                    if this
                                        .snapshot()
                                        .and_then(|snapshot| space_path(snapshot, &menu_id))
                                        .is_some()
                                    {
                                        rows.push((
                                            tr("复制空间路径"),
                                            UiAction::CopySpacePath(menu_id.clone()),
                                        ));
                                    }
                                    rows.push((
                                        tr("复制 Herdr 空间"),
                                        UiAction::CopyLocation(menu_id.clone()),
                                    ));
                                    rows.push((
                                        tr("新终端"),
                                        UiAction::NewTerminalIn(menu_id.clone()),
                                    ));
                                    rows.push((
                                        tr("关闭空间"),
                                        UiAction::CloseSpace(menu_id.clone()),
                                    ));
                                    this.open_menu(event.position, rows, window, cx)
                                }),
                            )
                            .when(!retained, |d| {
                                d.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                            })
                            .relative()
                            .children(self.object_indicator(&id, p, cx))
                            .on_drag_move(cx.listener(
                                move |this, event: &DragMoveEvent<ObjectDrag>, _, cx| {
                                    this.update_object_drop(event, &hover_id, cx);
                                },
                            ))
                            .on_drop(cx.listener(move |this, drag: &ObjectDrag, _, cx| {
                                this.finish_object_drop(drag, &drop_id, cx);
                            }))
                            .child(self.project_icon(&self.device(), &id, 16., p.muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .id(SharedString::from(format!("space-label-{id}")))
                                    .when(count == 0, |d| d.text_color(rgb(p.muted)))
                                    .child(fade_label(workspace.label.clone())),
                            )
                            .child(crate::sidebar_status::indicator(
                                format!("space-status-{id}").into(),
                                status,
                                unread,
                                dark,
                            ))
                            .child(
                                div()
                                    .w(px(count_width))
                                    .text_center()
                                    .flex_shrink_0()
                                    .text_color(rgb(p.muted))
                                    .child(count.to_string()),
                            ),
                        );
                    }
                }
            }
            if !self.shows_priority_sessions() {
                tree = tree.child(div().h(px(8.)));
            }
            let empty_priority = self.shows_priority_sessions() && {
                let device = self.device().id.clone();
                !self.sidebar_panes().iter().any(|pane| {
                    self.priority
                        .is_priority_group(self.owned_session(&device, pane).info())
                })
            };
            if empty_priority {
                tree = tree.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_size(px(12.))
                        .text_color(Hsla::from(rgb(p.muted)).opacity(0.7))
                        .child(tr("暂无待处理")),
                );
            }
            let mut saw_other = false;
            for pane in self.sidebar_panes() {
                if self.shows_priority_sessions() {
                    let device = self.device().id.clone();
                    let other = !self
                        .priority
                        .is_priority_group(self.owned_session(&device, pane).info());
                    if other && !saw_other {
                        tree = tree.child(
                            div()
                                .px_2()
                                .py_1()
                                .text_size(px(12.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(p.muted))
                                .child(tr("其他会话")),
                        );
                        saw_other = true;
                    }
                }
                tree = tree.child(self.pane_row(pane, p, dark, count_width, cx));
            }
            if self.shows_priority_sessions() && !saw_other {
                tree = tree.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(p.muted))
                        .child(tr("其他会话")),
                );
            }
            if snapshot.workspaces.is_empty() && !self.shows_priority_sessions() {
                tree = tree.child(
                    div()
                        .px_2()
                        .py_4()
                        .text_color(rgb(p.muted))
                        .child(tr("尚无空间，使用上方新建空间。")),
                );
            }
        } else {
            tree = tree.child(
                div()
                    .px_2()
                    .py_4()
                    .text_color(rgb(p.muted))
                    .child(tr("尚未获取设备快照")),
            );
        }
        let state = self.states.get(&self.device().id);
        let connected = state
            .is_some_and(|state| state.connection.is_some() && state.status.starts_with("已连接"));
        let status = state
            .map(|state| state.status.clone())
            .unwrap_or_else(|| tr("未连接"));
        if !connected {
            tree = tree.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .child(tr(&status))
                    .child(self.btn(
                        "sidebar-reconnect",
                        "重新连接",
                        UiAction::ReconnectDevice,
                        p,
                        cx,
                    )),
            );
        }
        view.child(tree)
            .child(
                div()
                    .h(px(36.))
                    .px_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child({
                        let device_view = device_view.clone();
                        self.btn_raw("device-menu", "", UiAction::DeviceMenu, p, cx)
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(crate::icons::icon("device", p.muted).flex_shrink_0())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(fade_label(self.device().name.clone())),
                            )
                            .child(crate::icons::icon("chevron_down", p.muted).flex_shrink_0())
                            .child(
                                canvas(
                                    move |bounds, _, cx| {
                                        device_view.update(cx, |this, cx| {
                                            if this.device_trigger_bounds != Some(bounds) {
                                                this.device_trigger_bounds = Some(bounds);
                                                if this.device_picker {
                                                    cx.notify();
                                                }
                                            }
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                    })
                    .child(div().flex_1())
                    .child({
                        let usage_view = device_view.clone();
                        self.icon_btn("usage", "usage", "用量", UiAction::Usage, p, cx)
                            .child(
                                canvas(
                                    move |bounds, _, cx| {
                                        usage_view.update(cx, |this, cx| {
                                            if this.usage_trigger_bounds != Some(bounds) {
                                                this.usage_trigger_bounds = Some(bounds);
                                                if this.show_usage {
                                                    cx.notify();
                                                }
                                            }
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                    })
                    .child(self.icon_btn(
                        "settings",
                        "settings",
                        "设置",
                        UiAction::Settings,
                        p,
                        cx,
                    )),
            )
            .into_any_element()
    }
    fn render_device_picker(
        &self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let trigger = self.device_trigger_bounds?;
        let bounds = device_picker_bounds(
            trigger,
            window.viewport_size(),
            window.client_inset().unwrap_or(px(0.)),
        );
        let mut picker = div()
            .id("device-picker")
            .key_context("OverlayPopup")
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                this.dispatch(UiAction::Dismiss, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.dispatch(UiAction::Dismiss, window, cx);
                }),
            )
            .w(bounds.size.width)
            .max_h(bounds.size.height)
            .overflow_y_scroll()
            .track_scroll(&self.device_picker_scroll)
            .p_1()
            .bg(rgb(p.raised))
            .border_1()
            .border_color(rgb(p.line))
            .rounded(px(8.))
            .shadow_md()
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.close_overlay(window, cx);
            }));

        picker = picker.child(
            div()
                .px_2()
                .h(px(24.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .text_size(px(11.))
                .text_color(rgb(p.muted))
                .child(tr("设备")),
        );

        for (index, device) in self.devices.iter().enumerate() {
            let selected = index == self.device_picker_index;
            let connected = self.device_connected(device);
            let subtitle = self.device_subtitle(device);
            let name = device.name.clone();
            let is_local = device.is_local();
            let mut row = div()
                .id(SharedString::from(format!("device-picker-row-{index}")))
                .w_full()
                .h(px(40.))
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
                    this.dispatch(UiAction::SelectDevice(index), window, cx);
                }));
            row = row.child(crate::icons::icon("device", p.muted));
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(p.text))
                                    .overflow_hidden()
                                    .id(SharedString::from(format!("device-name-{index}")))
                                    .min_w_0()
                                    .child(fade_label(name)),
                            )
                            .child(
                                div()
                                    .size(px(6.))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(rgb(if connected { 0x65b888 } else { p.muted })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .overflow_hidden()
                            .id(SharedString::from(format!("device-subtitle-{index}")))
                            .w_full()
                            .child(fade_label(subtitle)),
                    ),
            );
            if selected {
                row = row.child(crate::icons::icon("check", p.text));
            } else {
                row = row.child(div().size(px(16.)));
            }
            if !is_local {
                let rows = vec![
                    (tr("编辑设备"), UiAction::EditDevice(index)),
                    (tr("重新连接"), UiAction::ReconnectDeviceAt(index)),
                    (tr("删除设备"), UiAction::DeleteDevice(index)),
                ];
                row = row.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.device_picker = false;
                        this.open_menu(event.position, rows.clone(), window, cx);
                    }),
                );
            }
            picker = picker.child(row);
        }

        picker = picker.child(
            div()
                .mx_1()
                .my(px(6.))
                .h(px(1.))
                .flex_shrink_0()
                .bg(rgb(p.line)),
        );
        picker = picker.child(
            div()
                .id("device-picker-add")
                .w_full()
                .h(px(32.))
                .flex_shrink_0()
                .px_2()
                .rounded(px(5.))
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(rgb(p.text))
                .hover(|d| d.bg(rgb(p.active)))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.dispatch(UiAction::AddDevice, window, cx);
                }))
                .child(crate::icons::icon("plus", p.muted))
                .child(tr("添加设备...")),
        );

        Some(
            deferred(
                anchored()
                    .anchor(Corner::BottomLeft)
                    .position(bounds.bottom_left())
                    .snap_to_window_with_margin(px(8.))
                    .child(picker),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
    fn device_connected(&self, device: &Device) -> bool {
        self.states
            .get(&device.id)
            .is_some_and(|state| state.connection.is_some() && state.status.starts_with("已连接"))
    }
    fn device_subtitle(&self, device: &Device) -> String {
        match &device.kind {
            DeviceKind::Local => {
                let socket = device
                    .socket_path
                    .as_deref()
                    .and_then(|path| std::path::Path::new(path).file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("herdr.sock");
                tr(&format!("这台 Mac · {socket}"))
            }
            DeviceKind::Ssh { target } => tr(&format!("{target} · SSH")),
            DeviceKind::Tailcat => tr("tailcat 隧道"),
        }
    }
    fn pane_row(
        &self,
        pane: &herdr::Pane,
        p: Palette,
        dark: bool,
        _count_width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = pane.pane_id.clone();
        let rename = id.clone();
        let menu_id = id.clone();
        let label = self
            .snapshot()
            .map(|snapshot| snapshot_session_title(snapshot, &pane.pane_id))
            .unwrap_or_else(|| tr("终端"));
        let kind = crate::icons::pane_kind(&pane.extra, pane.agent.as_deref());
        let space = self
            .snapshot()
            .and_then(|s| {
                s.workspaces
                    .iter()
                    .find(|w| w.workspace_id == pane.workspace_id)
            })
            .map(|w| w.label.clone())
            .unwrap_or_default();
        let unread = self.unread.contains(&(self.device().id, id.clone()));
        let device = self.device().id.clone();
        let session = self.owned_session(&device, pane);
        let info = session.info();
        let priority_mode = self.shows_priority_sessions();
        let in_priority = priority_mode && self.priority.is_priority_group(info);
        let status_title = if !priority_mode {
            None
        } else if pane.agent_status.as_deref() == Some("blocked") {
            Some(tr("需输入"))
        } else if pane.agent_status.as_deref() == Some("done") && unread {
            Some(tr("完成"))
        } else if in_priority {
            Some(tr("正在查看"))
        } else {
            None
        };
        let tab_id = pane.tab_id.clone();
        let workspace = pane.workspace_id.clone();
        let mut row = self
            .btn_raw(
                format!("pane-{id}"),
                "",
                UiAction::SelectPane(id.clone()),
                p,
                cx,
            )
            .py_1()
            .rounded(px(8.))
            .text_size(px(13.))
            .mb_1()
            .flex()
            .flex_col()
            .gap(px(2.))
            .when(self.selected_pane.as_ref() == Some(&id), |d| {
                d.bg(rgb(p.active))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count == 2 {
                        cx.stop_propagation();
                        this.dispatch(UiAction::RenamePane(rename.clone()), window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_menu(
                        event.position,
                        vec![
                            (tr("重命名"), UiAction::RenamePane(menu_id.clone())),
                            (tr("接管会话"), UiAction::Attach(menu_id.clone())),
                            (tr("关闭会话"), UiAction::CloseRemote(menu_id.clone())),
                        ],
                        window,
                        cx,
                    )
                }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .id(SharedString::from(format!("pane-label-{id}")))
                            .when(in_priority, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(fade_label(label.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(crate::sidebar_status::indicator(
                                format!("pane-status-{id}").into(),
                                pane.agent_status.as_deref(),
                                unread,
                                dark,
                            ))
                            .children(status_title.map(|title| {
                                div()
                                    .text_size(px(11.))
                                    .flex_shrink_0()
                                    .text_color(rgb(p.muted))
                                    .child(title)
                            })),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(
                        crate::icons::agent_icon(kind.as_deref().unwrap_or("terminal"), p.muted)
                            .size(px(12.))
                            .flex_shrink_0(),
                    )
                    .child(kind.unwrap_or_else(|| tr("终端")))
                    .child("·")
                    .child(self.project_icon(&self.device(), &pane.workspace_id, 12., p.muted))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .id(SharedString::from(format!("pane-space-{id}")))
                            .flex_1()
                            .child(fade_label(space)),
                    ),
            );
        if let Some(tab_id) = tab_id.filter(|_| !priority_mode) {
            let drag = ObjectDrag {
                device: self.device().id,
                id: tab_id.clone(),
                workspace: Some(workspace.clone()),
                label,
                palette: p,
            };
            let hover_id = tab_id.clone();
            row = row
                .relative()
                .children(self.object_indicator(&tab_id, p, cx))
                .on_drag_move(
                    cx.listener(move |this, event: &DragMoveEvent<ObjectDrag>, _, cx| {
                        this.update_object_drop(event, &hover_id, cx);
                    }),
                )
                .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                .on_drop(cx.listener(move |this, drag: &ObjectDrag, _, cx| {
                    this.finish_object_drop(drag, &tab_id, cx);
                }));
        }
        row
    }
    fn object_ids<'a>(&'a self, drag: &ObjectDrag, target: &str) -> Option<Vec<&'a str>> {
        drag.ids(self.snapshot()?, &self.device().id, target)
    }

    fn update_object_drop(
        &mut self,
        event: &DragMoveEvent<ObjectDrag>,
        target: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.object_drag_in_tree
            || self.shows_priority_sessions()
            || !event.bounds.contains(&event.event.position)
            || self.form.is_some()
            || self.search.is_some()
            || self.menu.is_some()
            || self.device_picker
            || self.error.is_some()
            || self.show_usage
            || self.inspector.is_some()
            || self.pi_waiting.is_some()
        {
            return;
        }
        let drag = event.drag(cx);
        let after = event.event.position.y > event.bounds.origin.y + event.bounds.size.height / 2.;
        if self
            .object_ids(drag, target)
            .and_then(|ids| drop_index(&ids, &drag.id, target, after))
            .is_some()
        {
            self.object_drop = Some((target.to_owned(), after));
            cx.notify();
        }
    }

    /// Turn dropped Finder directories into spaces; files and non-local devices are ignored.
    fn create_spaces_from_folders(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.folder_drop = false;
        let directories = self
            .device()
            .is_local()
            .then(|| dropped_space_directories(paths.paths()))
            .unwrap_or_default();
        if !directories.is_empty() {
            for cwd in directories {
                if let Err(error) = self.create_empty_space(&cwd) {
                    self.error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }

    fn finish_object_drop(&mut self, drag: &ObjectDrag, target: &str, cx: &mut Context<Self>) {
        let candidate = self.object_drop.take();
        self.object_drag_in_tree = false;
        cx.notify();
        let Some((id, after)) = candidate.filter(|(id, _)| id == target) else {
            return;
        };
        let Some(ids) = self.object_ids(drag, &id) else {
            return;
        };
        let Some(index) = drop_index(&ids, &drag.id, &id, after) else {
            return;
        };
        if drag.workspace.is_some() {
            self.mutate(
                "tab.move".into(),
                json!({"tab_id":drag.id,"insert_index":index}),
            );
        } else {
            let before = ids.get(index).copied().map(str::to_owned);
            self.mutate(
                "workspace.move_block".into(),
                json!({"workspace_ids":[drag.id],"before_workspace_id":before}),
            );
        }
    }

    fn move_object_by(
        &mut self,
        id: &str,
        workspace: Option<String>,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let drag = ObjectDrag {
            device: self.device().id,
            id: id.to_owned(),
            workspace,
            label: String::new(),
            palette: Palette::new(false),
        };
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let ids: Vec<_> = match &drag.workspace {
            Some(workspace) => snapshot
                .tabs
                .iter()
                .filter(|t| &t.workspace_id == workspace)
                .map(|t| t.tab_id.as_str())
                .collect(),
            None => snapshot
                .workspaces
                .iter()
                .filter(|w| w.extra.get("retained") != Some(&Value::Bool(true)))
                .map(|w| w.workspace_id.as_str())
                .collect(),
        };
        let Some(index) = ids.iter().position(|value| *value == id) else {
            return;
        };
        let Some(target) = index
            .checked_add_signed(delta)
            .and_then(|i| ids.get(i))
            .map(|s| s.to_string())
        else {
            return;
        };
        self.object_drop = Some((target.clone(), delta > 0));
        self.finish_object_drop(&drag, &target, cx);
    }

    fn object_indicator(&self, id: &str, p: Palette, cx: &App) -> Option<Div> {
        let (_, after) = self
            .object_drop
            .as_ref()
            .filter(|(target, _)| target == id && cx.has_active_drag())?;
        Some(
            div()
                .absolute()
                .left(px(0.))
                .right(px(0.))
                .h(px(1.))
                .when(*after, |d| d.bottom(px(0.)))
                .when(!*after, |d| d.top(px(0.)))
                .bg(rgb(p.text))
                .child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .top(px(-3.))
                        .size(px(7.))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(p.text))
                        .bg(rgb(p.side)),
                ),
        )
    }

    fn begin_terminal_pane_drag(&self, source: &str, label: String) -> Option<TerminalPaneDrag> {
        let tree = self.split.as_ref()?;
        let leaves = tree.leaves();
        if leaves.len() <= 1 || !leaves.iter().any(|id| id == source) {
            return None;
        }
        Some(TerminalPaneDrag {
            source: source.to_owned(),
            root: tree.clone(),
            workspace: self.workspace.clone(),
            token: next_pane_drag_token(),
            label,
        })
    }

    fn can_drop_terminal_pane(&self, drag: &TerminalPaneDrag, target: &str) -> bool {
        drag.token != 0
            && drag.workspace == self.workspace
            && self.split.as_ref() == Some(&drag.root)
            && drag.source != target
            && self.split.as_ref().is_some_and(|tree| {
                let leaves = tree.leaves();
                leaves.iter().any(|id| id == &drag.source) && leaves.iter().any(|id| id == target)
            })
    }

    fn drop_terminal_pane(&mut self, drag: &TerminalPaneDrag, target: &str) -> bool {
        if !self.can_drop_terminal_pane(drag, target) {
            return false;
        }
        self.split
            .as_mut()
            .is_some_and(|tree| tree.swap(&drag.source, target))
    }

    fn render_split_grabber(
        &self,
        id: &str,
        leaf_id: &str,
        title: String,
        drag: Option<TerminalPaneDrag>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let group: SharedString = format!("split-handle-{leaf_id}").into();
        let close = div()
            .absolute()
            .right_0()
            .top_0()
            .opacity(0.)
            .group_hover(group.clone(), |s| s.opacity(1.))
            .child(self.icon_btn(
                format!("detach-{id}"),
                "close",
                if id.starts_with("shell-") {
                    "关闭视图"
                } else {
                    "关闭会话"
                },
                close_terminal_action(&self.device().id, id),
                p,
                cx,
            ));
        let mut grabber = div()
            .id(group.clone())
            .group(group.clone())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_grab()
            .occlude()
            .tooltip({
                let label = title;
                move |_, cx| cx.new(|_| ShellTooltip(label.clone())).into()
            })
            .child(
                div()
                    .w(px(36.))
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(rgb(p.muted))
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.)),
            )
            .child(close);
        if let Some(drag) = drag {
            grabber = grabber.on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()));
        }
        grabber
    }

    fn render_split(&self, tree: &SplitTree, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        self.render_split_at(tree, vec![], p, cx)
    }
    fn render_split_at(
        &self,
        tree: &SplitTree,
        path: Vec<bool>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match tree {
            SplitTree::Leaf(id) => {
                let leaf_id = id.clone();
                let multi = self
                    .split
                    .as_ref()
                    .is_some_and(|tree| tree.leaves().len() > 1);
                let title = self
                    .terminals
                    .get(id)
                    .map(|t| {
                        if id.starts_with("shell-") {
                            tr("独立终端")
                        } else {
                            id.strip_prefix(&format!("{}:", self.device().id))
                                .and_then(|pane| {
                                    self.snapshot().map(|s| snapshot_session_title(s, pane))
                                })
                                .unwrap_or_else(|| t.title.clone())
                        }
                    })
                    .unwrap_or_else(|| tr("连接中…"));
                let drag = self.begin_terminal_pane_drag(id, title.clone());
                let mut body = div()
                    .id(SharedString::from(format!("split-leaf-{leaf_id}")))
                    .size_full()
                    .overflow_hidden()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .relative()
                    .bg(rgb(p.bg))
                    .on_drop({
                        let drop_id = leaf_id.clone();
                        cx.listener(move |this, drag: &TerminalPaneDrag, _, cx| {
                            if this.drop_terminal_pane(drag, &drop_id) {
                                cx.notify();
                            }
                        })
                    });
                if let Some(pane) = self.terminals.get(id) {
                    let focus_id = id.clone();
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .overflow_hidden()
                            .p(px(self.settings.terminal_margin as f32))
                            .when(multi && self.active_terminal.as_ref() != Some(id), |d| {
                                d.opacity(self.settings.unfocused_split_opacity as f32)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    this.dispatch(UiAction::Focus(focus_id.clone()), window, cx);
                                }),
                            )
                            .child(pane.terminal.clone()),
                    );
                    if let Some(feedback) = self.render_terminal_feedback(id, p, cx) {
                        body = body.child(feedback);
                    }
                }
                if multi {
                    body = body.child(self.render_split_grabber(id, &leaf_id, title, drag, p, cx));
                }
                body.into_any_element()
            }
            SplitTree::Split {
                axis,
                ratio,
                first,
                second,
                ..
            } => {
                let row = *axis == Axis::Vertical;
                let axis = *axis;
                let ratio = *ratio;
                let mut first_path = path.clone();
                first_path.push(false);
                let mut second_path = path.clone();
                second_path.push(true);
                let drag_path = path.clone();
                let divider = div()
                    .flex_shrink_0()
                    .id(SharedString::from(format!("divider-{path:?}")))
                    .when(row, |d| d.w(px(5.)).h_full().cursor_col_resize())
                    .when(!row, |d| d.h(px(5.)).w_full().cursor_row_resize())
                    .bg(rgb(p.line))
                    .hover(move |d| d.bg(rgb(p.accent)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.dragging = Some((drag_path.clone(), axis, event.position, ratio));
                            cx.stop_propagation();
                        }),
                    );
                div()
                    .size_full()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .when(!row, |d| d.flex_col())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .min_h_0()
                            .min_w_0()
                            .when(row, |d| d.w(relative(ratio)))
                            .when(!row, |d| d.h(relative(ratio)))
                            .child(self.render_split_at(first, first_path, p, cx)),
                    )
                    .child(divider)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .child(self.render_split_at(second, second_path, p, cx)),
                    )
                    .into_any_element()
            }
        }
    }
    fn render_workspace(&self, p: Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let state = self.states.get(&self.device().id);
        let connected = state
            .is_some_and(|state| state.connection.is_some() && state.status.starts_with("已连接"));
        let content = if let Some(tree) = &self.split {
            self.render_split(tree, p, cx)
        } else {
            let loading = !connected && state.is_some_and(|state| state.loading);
            let snapshot = self.snapshot();
            let has_spaces = snapshot.is_some_and(|s| !s.workspaces.is_empty());
            let empty_space = self
                .workspace
                .as_ref()
                .is_some_and(|id| snapshot.is_some_and(|s| workspace_has_no_sessions(id, s)));
            let message = if loading {
                tr("连接中…")
            } else if !connected {
                tr(state.map_or("尚未连接", |state| state.status.as_str()))
            } else if empty_space {
                tr("此空间没有终端")
            } else if has_spaces {
                tr("选择一个会话")
            } else {
                tr("尚无空间")
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_4()
                .when(loading, |d| {
                    d.child(crate::icons::icon("terminal", p.muted).size(px(48.)))
                })
                .when(!loading, |d| {
                    d.child(
                        crate::icons::illustration(
                            if connected {
                                "empty-space"
                            } else {
                                "connection"
                            },
                            if connected { 200. } else { 144. },
                        )
                        .when(connected, |image| image.object_fit(ObjectFit::Contain)),
                    )
                })
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(p.muted))
                        .child(message),
                )
                .when(connected && empty_space, |d| {
                    let kinds = enabled_agent_kinds(
                        &self.settings,
                        state.map(|s| s.catalog.as_slice()).unwrap_or_default(),
                    );
                    let mut agents = div()
                        .w(px(480.))
                        .max_w_full()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_2();
                    for row in kinds.chunks(4) {
                        let mut buttons = div().flex().w_full().min_w_0().gap_2();
                        for kind in row {
                            let label = agent_display_name(kind);
                            buttons = buttons.child(
                                self.btn_raw(
                                    format!("empty-agent-{kind}"),
                                    "",
                                    UiAction::QuickAgent(kind.clone()),
                                    p,
                                    cx,
                                )
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(crate::icons::agent_icon(kind, p.muted))
                                .child(div().whitespace_nowrap().child(label))
                                .children(
                                    gpui_component::kbd::Kbd::binding_for_action(
                                        &app_shortcuts::QuickAgent { kind: kind.clone() },
                                        Some("Workbench"),
                                        window,
                                    ),
                                ),
                            );
                        }
                        for _ in row.len()..4 {
                            buttons = buttons.child(div().flex_1().min_w_0());
                        }
                        agents = agents.child(buttons);
                    }
                    d.child(
                        self.btn("empty-terminal", "", UiAction::NewTerminal, p, cx)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(crate::icons::icon("terminal", p.muted))
                            .child(tr("新建终端")),
                    )
                    .when(!kinds.is_empty(), |d| d.child(agents))
                })
                .when(connected && !has_spaces, |d| {
                    d.child(self.btn("empty-new", "新建空间", UiAction::NewSpace, p, cx))
                })
                .when(!connected && !loading, |d| {
                    d.child(self.btn(
                        "empty-refresh",
                        "重新连接",
                        UiAction::ReconnectDevice,
                        p,
                        cx,
                    ))
                    .when(self.device().is_local(), |d| {
                        d.child(self.btn(
                            "start-local",
                            "启动本机服务…",
                            UiAction::StartServer,
                            p,
                            cx,
                        ))
                    })
                })
                .into_any_element()
        };
        let center = div().flex_1().min_w_0().min_h_0().flex().flex_col().child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(content),
        );
        if self.details {
            let mut details = div()
                .id("workspace-details")
                .min_h_0()
                .overflow_y_scroll()
                .w(px(230.))
                .flex_shrink_0()
                .p_3()
                .bg(rgb(p.side))
                .border_l_1()
                .border_color(rgb(p.line))
                .flex()
                .flex_col()
                .gap_2()
                .child(section("当前对象", p))
                .child(self.device().name.clone());
            {
                let mut meta = div()
                    .text_color(rgb(p.muted))
                    .text_size(px(12.))
                    .child(self.space_label());
                if let Some(info) = &self.git_metadata {
                    if info.branch.is_some() || info.is_worktree {
                        meta = meta.child(" · ");
                    }
                    if let Some(branch) = &info.branch {
                        meta = meta.child(div().font_family("Menlo").child(branch.clone()));
                    }
                    if info.is_worktree {
                        meta = meta.child(format!(" Worktree {}", info.repository_name));
                    }
                }
                details = details.child(meta);
            }
            if let Some(pane) = connected.then_some(()).and_then(|_| {
                self.selected_pane
                    .as_ref()
                    .and_then(|id| self.snapshot()?.panes.iter().find(|p| &p.pane_id == id))
            }) {
                details = details
                    .child(pane.pane_id.clone())
                    .child(pane.cwd.clone().unwrap_or_default())
                    .child(self.btn(
                        "inspector",
                        "检查会话 / 发送输入",
                        UiAction::Inspector,
                        p,
                        cx,
                    ))
                    .child(self.btn(
                        "rename-pane",
                        "重命名",
                        UiAction::RenamePane(pane.pane_id.clone()),
                        p,
                        cx,
                    ))
                    .when(pane.agent.is_some(), |d| {
                        d.child(self.btn(
                            "rename-agent",
                            "重命名 Agent 身份…",
                            UiAction::RenameAgent(pane.pane_id.clone()),
                            p,
                            cx,
                        ))
                    })
                    .child(self.btn(
                        "close-remote",
                        "结束服务端会话…",
                        UiAction::CloseRemote(pane.pane_id.clone()),
                        p,
                        cx,
                    ));
                if let Some(tab) = &pane.tab_id {
                    details = details
                        .child(self.btn(
                            "tab-up",
                            "向前移动",
                            UiAction::MoveTab(tab.clone(), -1),
                            p,
                            cx,
                        ))
                        .child(self.btn(
                            "tab-down",
                            "向后移动",
                            UiAction::MoveTab(tab.clone(), 1),
                            p,
                            cx,
                        ));
                }
            }
            if let Some(workspace) = &self.workspace {
                let retained = self.snapshot().is_some_and(|s| {
                    s.workspaces.iter().any(|w| {
                        &w.workspace_id == workspace
                            && w.extra.get("retained") == Some(&Value::Bool(true))
                    })
                });
                details = details
                    .child(section("空间", p))
                    .when(connected && !retained, |d| {
                        d.child(self.btn(
                            "space-up",
                            "向前移动空间",
                            UiAction::MoveWorkspace(workspace.clone(), -1),
                            p,
                            cx,
                        ))
                        .child(self.btn(
                            "space-down",
                            "向后移动空间",
                            UiAction::MoveWorkspace(workspace.clone(), 1),
                            p,
                            cx,
                        ))
                    })
                    .when(connected || retained, |d| {
                        d.child(self.btn(
                            "space-close",
                            "关闭空间",
                            UiAction::CloseSpace(workspace.clone()),
                            p,
                            cx,
                        ))
                    });
            }
            if connected && self.split.is_some() {
                details = details
                    .child(section("终端", p))
                    .child(self.btn("copy-command", "复制连接命令", UiAction::CopyCommand, p, cx))
                    .child(self.btn("attachment", "发送附件…", UiAction::Attachment, p, cx))
                    .child(self.btn(
                        "terminal-search",
                        "搜索终端",
                        UiAction::TerminalSearch,
                        p,
                        cx,
                    ))
                    .when(
                        self.split
                            .as_ref()
                            .is_some_and(|tree| tree.leaves().len() > 1),
                        |d| d.child(self.btn("equalize", "均分窗格", UiAction::Equalize, p, cx)),
                    );
            }
            return div()
                .flex()
                .size_full()
                .child(center)
                .child(details)
                .into_any_element();
        }
        center.into_any_element()
    }
    fn render_files(
        &mut self,
        window: &mut Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let local_path = self.input("local-path", self.local_path.clone(), "~", window, cx);
        let remote_path = self.input("file-path", self.file_path.clone(), "~", window, cx);
        let mut local_list = div()
            .id("local-files")
            .overflow_y_scroll()
            .flex_1()
            .min_h_0();
        if self.local_loading {
            local_list = local_list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child(tr("正在读取目录…")),
            );
        } else if let Some(Err(error)) = &self.local_read {
            local_list = local_list.child(
                div()
                    .p_4()
                    .flex()
                    .gap_2()
                    .child(crate::icons::icon("warning", p.muted))
                    .child(tr(error)),
            );
        } else if directory_is_empty(
            &self.local_read,
            self.local_loading,
            self.local_entries.is_empty(),
        ) {
            local_list = local_list.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(p.muted))
                    .child(crate::icons::illustration("files", 120.))
                    .child(tr("此目录为空")),
            );
        } else if self.local_read.is_none() {
            local_list = local_list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child(tr("尚未读取目录")),
            );
        }
        for entry in &self.local_entries {
            let directory = entry.kind == files::FileKind::Directory;
            let selected = self.local_selected.as_deref() == Some(&entry.path);
            let action = if directory {
                UiAction::BrowseLocal(entry.path.clone())
            } else {
                UiAction::SelectLocalFile(entry.path.clone())
            };
            local_list = local_list.child(
                div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .when(selected, |d| d.bg(rgb(p.active)))
                    .child(
                        self.btn_raw(format!("local-{}", entry.path), "", action, p, cx)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                crate::icons::icon(
                                    if directory { "folder" } else { "copy" },
                                    p.muted,
                                )
                                .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(fade_label(entry.name.clone())),
                            )
                            .child(div().flex_shrink_0().child(file_entry_metadata(entry)))
                            .min_w_0()
                            .flex_1(),
                    ),
            );
        }
        let mut remote_list = div()
            .id("files-list")
            .overflow_y_scroll()
            .flex_1()
            .min_h_0();
        if self.file_loading {
            remote_list = remote_list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child(tr("正在读取目录…")),
            );
        } else if let Some(Err(error)) = &self.file_read {
            remote_list = remote_list.child(
                div()
                    .p_4()
                    .flex()
                    .gap_2()
                    .child(crate::icons::icon("warning", p.muted))
                    .child(tr(error)),
            );
        } else if directory_is_empty(
            &self.file_read,
            self.file_loading,
            self.file_entries.is_empty(),
        ) {
            remote_list = remote_list.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(p.muted))
                    .child(crate::icons::illustration("files", 120.))
                    .child(tr("此目录为空")),
            );
        } else if self.file_read.is_none() {
            remote_list = remote_list.child(
                div()
                    .p_4()
                    .text_color(rgb(p.muted))
                    .child(tr("尚未读取目录")),
            );
        }
        for entry in &self.file_entries {
            let directory = entry.kind == files::FileKind::Directory;
            let selected = self.file_selected.as_deref() == Some(&entry.path);
            let action = if directory {
                UiAction::Browse(entry.path.clone())
            } else {
                UiAction::SelectFile(entry.path.clone())
            };
            remote_list = remote_list.child(
                div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .when(selected, |d| d.bg(rgb(p.active)))
                    .child(
                        self.btn_raw(format!("file-{}", entry.path), "", action, p, cx)
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                crate::icons::icon(
                                    if directory { "folder" } else { "copy" },
                                    p.muted,
                                )
                                .flex_shrink_0(),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(fade_label(entry.name.clone())),
                            )
                            .child(div().flex_shrink_0().child(file_entry_metadata(entry)))
                            .min_w_0()
                            .flex_1(),
                    ),
            );
        }
        let local_root = files::parent(&self.local_path).is_none();
        let remote_root = files::parent(&self.file_path).is_none();
        let progress = self
            .transfer_progress
            .map(|(done, total)| format!("{} / {}", format_bytes(done), format_bytes(total)));
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(p.line))
                    .child(self.btn(
                        "file-hidden",
                        if self.file_hidden {
                            "隐藏项：显示"
                        } else {
                            "隐藏项：隐藏"
                        },
                        UiAction::Hidden,
                        p,
                        cx,
                    ))
                    .child(self.btn("file-refresh", "刷新", UiAction::Refresh, p, cx))
                    .child(div().flex_1())
                    .when_some(progress, |d, text| {
                        d.child(tr(&format!("文件传输：{text}")))
                    })
                    .when_some(self.transfer.clone(), |d, token| {
                        d.child(
                            div()
                                .id("cancel-transfer")
                                .cursor_pointer()
                                .child(tr("取消传输"))
                                .on_click(move |_, _, _| token.cancel()),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .text_color(rgb(p.muted))
                                    .child(tr("本机")),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(!local_root, |d| {
                                        d.child(self.btn(
                                            "local-up",
                                            "上级",
                                            UiAction::LocalUp,
                                            p,
                                            cx,
                                        ))
                                    })
                                    .child(self.btn(
                                        "local-home",
                                        "主页",
                                        UiAction::LocalHome,
                                        p,
                                        cx,
                                    ))
                                    .child(div().flex_1().min_w_0().child(local_path))
                                    .child(self.btn(
                                        "local-go",
                                        "前往",
                                        UiAction::LocalBrowseInput,
                                        p,
                                        cx,
                                    )),
                            )
                            .child(local_list),
                    )
                    .child(
                        div()
                            .w(px(56.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(self.btn(
                                "upload",
                                "→",
                                UiAction::Transfer(files::Direction::Upload),
                                p,
                                cx,
                            ))
                            .child(self.btn(
                                "download",
                                "←",
                                UiAction::Transfer(files::Direction::Download),
                                p,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .px_3()
                                    .py_1()
                                    .text_color(rgb(p.muted))
                                    .child(tr("设备")),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(!remote_root, |d| {
                                        d.child(self.btn("file-up", "上级", UiAction::Up, p, cx))
                                    })
                                    .child(self.btn("file-home", "主页", UiAction::Home, p, cx))
                                    .child(div().flex_1().min_w_0().child(remote_path))
                                    .child(self.btn(
                                        "file-go",
                                        "前往",
                                        UiAction::BrowseInput,
                                        p,
                                        cx,
                                    )),
                            )
                            .child(remote_list),
                    ),
            )
            .into_any_element()
    }
    fn render_overlay(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.inspector.is_some() {
            return Some(self.render_inspector(p, cx));
        }
        if self.show_usage {
            return Some(self.render_usage(p, cx));
        }
        if self.search.is_some() {
            return Some(self.render_search(p, cx));
        }
        let form = self.form.as_ref()?;
        if matches!(form.kind, FormKind::NewItem) {
            return Some(self.render_new_item_modal(form, p, cx));
        }
        if matches!(form.kind, FormKind::Device(_) | FormKind::NewSpace) {
            return None;
        }
        let mut fields = div().flex().flex_col().gap_4();
        for (index, (label, input)) in form.fields.iter().enumerate() {
            let browse =
                label.contains("目录") || label.contains("路径") || label.contains("本地文件");
            fields = fields.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(p.muted))
                            .child(tr(label)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(input.clone()))
                            .when(browse, |d| {
                                d.child(self.btn(
                                    format!("browse-{index}"),
                                    "选择…",
                                    UiAction::PickPath(index, label.contains("目录")),
                                    p,
                                    cx,
                                ))
                            }),
                    ),
            );
        }
        if matches!(form.kind, FormKind::DeleteDevice(_)) {
            fields=fields.child(tr("仅从客户端移除此设备，不结束远端会话、不删除文件。已有终端视图保留至手动关闭；Keychain凭据不会自动删除。"));
        }
        if let Some(message) = &form.message {
            fields = fields.child(tr(message));
        } else if matches!(form.kind, FormKind::Confirm(_, _)) {
            fields = fields.child(tr(
                "这会结束服务端对应会话及其进程，不只是关闭本地视图。此操作不可撤销。",
            ));
        }
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_action(cx.listener(|this, _: &Escape, window, cx| {
                    this.dispatch(UiAction::Dismiss, window, cx);
                }))
                .on_action(
                    cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                        this.dispatch(UiAction::Dismiss, window, cx);
                    }),
                )
                .child(
                    div()
                        .key_context("OverlayPopup")
                        .w(px(440.))
                        .max_w_full()
                        .p_4()
                        .bg(rgb(p.raised))
                        .border_1()
                        .border_color(rgb(p.line))
                        .rounded(px(8.))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(div().text_size(px(18.)).child(tr(&form.title)))
                        .child(fields)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .when(matches!(form.kind, FormKind::Transfer(_)), |d| {
                                    let direction = match &form.kind {
                                        FormKind::Transfer(direction) => *direction,
                                        _ => files::Direction::Upload,
                                    };
                                    d.child(self.btn(
                                        "transfer-replace",
                                        "替换",
                                        UiAction::ConfirmTransfer(
                                            direction,
                                            files::ConflictPolicy::Replace,
                                        ),
                                        p,
                                        cx,
                                    ))
                                    .child(self.btn(
                                        "transfer-keep",
                                        "保留两者",
                                        UiAction::ConfirmTransfer(
                                            direction,
                                            files::ConflictPolicy::KeepBoth,
                                        ),
                                        p,
                                        cx,
                                    ))
                                    .child(self.btn(
                                        "form-cancel",
                                        "取消",
                                        UiAction::Dismiss,
                                        p,
                                        cx,
                                    ))
                                })
                                .when(!matches!(form.kind, FormKind::Transfer(_)), |d| {
                                    let submit = if !form.submit.is_empty() {
                                        form.submit.clone()
                                    } else if matches!(form.kind, FormKind::Confirm(_, _)) {
                                        tr("关闭")
                                    } else {
                                        tr("保存 / 执行")
                                    };
                                    d.child(self.btn(
                                        "form-cancel",
                                        "取消",
                                        UiAction::Dismiss,
                                        p,
                                        cx,
                                    ))
                                    .child(self.btn(
                                        "form-submit",
                                        submit,
                                        UiAction::Submit,
                                        p,
                                        cx,
                                    ))
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}
fn section(label: &str, p: Palette) -> Div {
    div()
        .px_2()
        .py_2()
        .text_size(px(11.))
        .text_color(rgb(p.muted))
        .child(tr(label))
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn valid_agent_name(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
/// Installed agent kinds in the user's order. The catalog is the only source of
/// availability, so an uninstalled kind never appears, not even before the first scan.
pub(crate) fn available_agent_kinds(settings: &Settings, catalog: &[String]) -> Vec<String> {
    let mut kinds = Vec::new();
    for kind in settings
        .agent_kind_order
        .iter()
        .cloned()
        .chain(catalog.iter().cloned())
    {
        if kind == "terminal"
            || !catalog.contains(&kind)
            || kinds.iter().any(|value| value == &kind)
        {
            continue;
        }
        kinds.push(kind);
    }
    kinds
}
pub(crate) fn enabled_agent_kinds(settings: &Settings, catalog: &[String]) -> Vec<String> {
    available_agent_kinds(settings, catalog)
        .into_iter()
        .filter(|kind| !settings.disabled_agent_kinds.contains(kind))
        .collect()
}

/// Last local install scan. Menus are built outside `AppView`, so they read it here.
static INSTALLED_AGENT_KINDS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);
fn publish_installed_agent_kinds(kinds: Vec<String>) -> bool {
    let mut published = INSTALLED_AGENT_KINDS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if published.as_deref() == Some(kinds.as_slice()) {
        return false;
    }
    *published = Some(kinds);
    true
}
pub(crate) fn installed_agent_kinds() -> Vec<String> {
    INSTALLED_AGENT_KINDS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .unwrap_or_default()
}
pub(crate) fn known_agents() -> &'static [&'static str] {
    &[
        "claude", "codex", "cursor", "gemini", "grok", "hermes", "kimi", "opencode", "pi", "omp",
        "copilot",
    ]
}
pub(crate) fn agent_display_name(kind: &str) -> String {
    tr(match kind {
        "claude" => "Claude",
        "codex" => "Codex",
        "cursor" => "Cursor",
        "gemini" => "Gemini",
        "grok" => "Grok",
        "hermes" => "Hermes",
        "kimi" => "Kimi",
        "opencode" => "OpenCode",
        "pi" => "Pi",
        "omp" => "Oh My Pi",
        "copilot" => "Copilot",
        _ => kind,
    })
}
pub(crate) fn agent_command_hint(kind: &str) -> String {
    if kind == "cursor" {
        "cursor-agent".into()
    } else {
        kind.to_owned()
    }
}
fn bump_disabled_kinds_revision() {
    let next = crate::settings::read_preference("agents.disabledKinds.revision")
        .ok()
        .flatten()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.)
        + 1.;
    let _ = crate::settings::write_preference("agents.disabledKinds.revision", Some(&json!(next)));
}
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}
fn format_modified(ts: Option<u64>) -> String {
    let Some(secs) = ts else {
        return "—".into();
    };
    if secs > i64::MAX as u64 {
        return "—".into();
    }
    let t = secs as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    let ptr = unsafe { libc::localtime_r(&t, tm.as_mut_ptr()) };
    if ptr.is_null() {
        return "—".into();
    }
    let tm = unsafe { tm.assume_init() };
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    )
}
fn file_entry_metadata(entry: &files::FileEntry) -> String {
    let size = entry.size.map(format_bytes).unwrap_or_else(|| "—".into());
    format!("{size}  {}", format_modified(entry.modified))
}
#[path = "app_catalog.rs"]
pub(crate) mod app_catalog;
#[path = "app_feedback.rs"]
mod app_feedback;
#[path = "app_persistence.rs"]
mod app_persistence;
#[path = "app_shortcuts.rs"]
pub(crate) mod app_shortcuts;
#[path = "app_device.rs"]
mod device_ui;
#[path = "app_inspector.rs"]
mod inspector_ui;
#[path = "app_runtime.rs"]
mod runtime_ui;
#[path = "app_search.rs"]
mod search_ui;
#[path = "app_space.rs"]
mod space_ui;
#[path = "app_title.rs"]
mod title_ui;
use title_ui::snapshot_session_title;
#[path = "context_menu.rs"]
mod context_menu;
#[path = "priority.rs"]
mod priority;
#[path = "app_settings.rs"]
mod settings_ui;
use context_menu::menu_step;
impl Focusable for AppView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.error.is_some()
            && self.form.as_ref().is_none_or(|form| {
                form.device_form.is_none()
                    && form.space_form.is_none()
                    && !matches!(form.kind, FormKind::NewItem)
            })
            && !self.feedback_focus.is_focused(window)
            && self.feedback_previous_focus.is_none()
        {
            self.feedback_previous_focus = window.focused(cx);
            self.feedback_focus.focus(window);
        } else if self.error.is_none() {
            if let Some(focus) = self.feedback_previous_focus.take() {
                focus.focus(window);
            }
        }
        let p = self.palette(window);
        let dark = self.is_dark(window);
        let component_locale = if crate::i18n::language() == "zh-Hans" {
            "zh-CN"
        } else {
            "en"
        };
        if &*gpui_component::locale() != component_locale {
            gpui_component::set_locale(component_locale);
        }
        if gpui_component::Theme::global(cx).is_dark() != dark {
            let mode = if dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            };
            gpui_component::Theme::change(mode, Some(window), cx);
        }
        p.apply_component_theme(cx);
        self.sync_terminal_themes(window, cx);
        if let Some(edit) = &self.space_edit {
            edit.editor
                .update(cx, |editor, cx| editor.set_theme(dark, cx));
        }
        for input in self.fields.values().chain(self.search.iter()).chain(
            self.form
                .iter()
                .flat_map(|form| form.fields.iter().map(|(_, input)| input)),
        ) {
            input.update(cx, |input, cx| input.set_theme(dark, cx));
        }
        let content = match self.screen {
            Screen::Workspace => self.render_workspace(p, window, cx),
            Screen::Files => self.render_files(window, p, cx),
            Screen::Settings => self.render_settings(window, cx, p),
        };
        let folder_drop = self.folder_drop && cx.has_active_drag();
        if !cx.has_active_drag() {
            self.object_drop = None;
            self.folder_drop = false;
        }
        let mut root = div()
            .id("workbench")
            .on_drag_move(cx.listener(|this, _: &DragMoveEvent<ObjectDrag>, _, cx| {
                this.object_drop = None;
                this.object_drag_in_tree = false;
                cx.notify();
            }))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let folders = this.device().is_local()
                        && event.drag(cx).paths().iter().any(|path| path.is_dir());
                    if this.folder_drop != folders {
                        this.folder_drop = folders;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.create_spaces_from_folders(paths, cx);
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key != "escape" {
                    return;
                }
                if cx.has_active_drag() {
                    this.object_drop = None;
                    this.object_drag_in_tree = false;
                    cx.stop_active_drag(window);
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if this.overlay_ime_composing(cx) {
                    return;
                }
                if this.overlay_open() {
                    this.dispatch(UiAction::Dismiss, window, cx);
                    cx.stop_propagation();
                    window.prevent_default();
                }
            }))
            .key_context(if self.menu.is_some() {
                "ContextMenu"
            } else {
                "Workbench"
            })
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(p.bg))
            .text_color(rgb(p.text))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont");
        macro_rules! on {
            ($ty:ty,$action:expr) => {
                root = root.on_action(
                    cx.listener(|this, _: &$ty, window, cx| this.dispatch($action, window, cx)),
                );
            };
        }
        on!(FindTerminal, UiAction::TerminalSearch);
        on!(Search, UiAction::Search);
        on!(NewItem, UiAction::NewItem);
        on!(NewTerminal, UiAction::NewTerminal);
        on!(NewSpace, UiAction::NewSpace);
        on!(OpenSettings, UiAction::Settings);
        on!(ToggleSidebar, UiAction::ToggleSidebar);
        on!(SplitRight, UiAction::Split(Axis::Vertical));
        on!(SplitDown, UiAction::Split(Axis::Horizontal));
        on!(FocusLeft, UiAction::Neighbor(Direction::Left, false));
        on!(FocusRight, UiAction::Neighbor(Direction::Right, false));
        on!(FocusUp, UiAction::Neighbor(Direction::Up, false));
        on!(FocusDown, UiAction::Neighbor(Direction::Down, false));
        on!(SwapLeft, UiAction::Neighbor(Direction::Left, true));
        on!(SwapRight, UiAction::Neighbor(Direction::Right, true));
        on!(SwapUp, UiAction::Neighbor(Direction::Up, true));
        on!(SwapDown, UiAction::Neighbor(Direction::Down, true));
        on!(Widen, UiAction::StepPane(Axis::Vertical, true));
        on!(Narrow, UiAction::StepPane(Axis::Vertical, false));
        on!(Grow, UiAction::StepPane(Axis::Horizontal, true));
        on!(Shrink, UiAction::StepPane(Axis::Horizontal, false));
        on!(Equalize, UiAction::Equalize);
        on!(Escape, UiAction::Dismiss);
        on!(gpui_component::input::Escape, UiAction::Dismiss);
        on!(Refresh, UiAction::Refresh);
        root = root.on_action(cx.listener(|this, _: &CopySpacePath, window, cx| {
            this.copy_space_path_from_shortcut(window, cx)
        }));
        root =
            root.on_action(cx.listener(|_, _: &FocusNext, window, _| window.focus_next()))
                .on_action(cx.listener(|_, _: &FocusPrevious, window, _| window.focus_prev()))
                .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
                .on_action(cx.listener(|_, _: &Fullscreen, window, _| window.toggle_fullscreen()))
                .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                    this.cycle_session(true, window, cx)
                }))
                .on_action(cx.listener(|this, _: &PreviousSession, window, cx| {
                    this.cycle_session(false, window, cx)
                }))
                .on_action(
                    cx.listener(|this, action: &app_shortcuts::QuickAgent, window, cx| {
                        this.quick_agent(&action.kind, window, cx)
                    }),
                )
                .on_action(cx.listener(|this, action: &GoSession, window, cx| {
                    this.go_session(action.n, window, cx)
                }))
                .on_action(cx.listener(|this, action: &GoSpace, window, cx| {
                    this.go_space(action.n, window, cx)
                }))
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    if this.sidebar_resizing {
                        this.sidebar_width = f32::from(event.position.x).clamp(200., 420.);
                        cx.notify();
                    }
                    if let Some((path, axis, start, initial)) = &this.dragging {
                        let size = window.bounds().size;
                        let delta = if *axis == Axis::Vertical {
                            (event.position.x - start.x) / size.width
                        } else {
                            (event.position.y - start.y) / size.height
                        };
                        if let Some(tree) = this.split.as_mut() {
                            set_split_ratio(tree, path, (*initial + delta).clamp(0.2, 0.8));
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.object_drop = None;
                        this.object_drag_in_tree = false;
                        cx.notify();
                        this.dragging = None;
                        this.sidebar_resizing = false;
                    }),
                );
        root = root.on_action(cx.listener(|this, _: &Close, window, cx| {
            if this.error.is_some() {
                this.dismiss_feedback(window, cx);
                cx.stop_propagation();
                return;
            }
            if this.form.is_some() || this.search.is_some() {
                this.close_overlay(window, cx);
            } else if this.screen != Screen::Workspace {
                this.screen = Screen::Workspace;
                this.focus_active(window, cx);
                cx.notify();
            } else if this
                .split
                .as_ref()
                .is_some_and(|tree| tree.leaves().len() > 1)
            {
                if let Some(id) = this.active_terminal.clone() {
                    this.dispatch(close_terminal_action(&this.device().id, &id), window, cx);
                }
            } else if let Some(id) = this
                .active_terminal
                .clone()
                .filter(|id| id.starts_with("shell-"))
            {
                this.dispatch(UiAction::Detach(id), window, cx);
            } else if let Some(pane) = this.selected_pane.clone() {
                this.request_close_pane(&pane, window, cx);
            } else if let Some(id) = this.active_terminal.clone() {
                this.dispatch(UiAction::Detach(id), window, cx);
            } else if let Some(workspace) = this.workspace.clone() {
                this.close_command_space(&workspace, window, cx);
            } else {
                window.remove_window();
            }
        }));
        let mut titlebar = div()
            .bg(rgb(p.titlebar))
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(rgb(p.line))
            .when(!self.sidebar || self.screen == Screen::Settings, |d| {
                d.pl(px(78.))
            })
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    window.titlebar_double_click();
                } else {
                    window.start_window_move();
                }
            });
        if self.screen != Screen::Workspace {
            titlebar = titlebar.child(self.icon_btn(
                "return-workspace",
                "arrow_left",
                "返回工作台",
                UiAction::Workspace,
                p,
                cx,
            ));
        }
        if !self.sidebar && self.screen != Screen::Settings {
            titlebar = titlebar.child(self.icon_btn(
                "show-sidebar",
                "sidebar",
                "显示侧栏",
                UiAction::ToggleSidebar,
                p,
                cx,
            ));
        }
        if !self.sidebar && self.screen != Screen::Settings && !self.sidebar_actions_hidden[3] {
            titlebar = titlebar.child(self.icon_btn(
                "titlebar-search",
                "search",
                "搜索",
                UiAction::Search,
                p,
                cx,
            ));
        }
        titlebar = titlebar.child(self.render_session_heading(p, window, cx));
        if self.screen == Screen::Workspace {
            // ponytail: extreme widths scroll intact metadata; add collapsing only if requested.
            let mut metadata = div()
                .id("titlebar-metadata")
                .min_w_0()
                .flex_shrink()
                .overflow_x_scroll()
                .flex()
                .items_center()
                .gap_2();
            if let Some(pane) = self
                .selected_pane
                .as_ref()
                .and_then(|id| self.snapshot()?.panes.iter().find(|p| &p.pane_id == id))
            {
                if pane.agent_status.as_deref() == Some("blocked") {
                    metadata =
                        metadata.child(crate::icons::icon("warning", p.muted).flex_shrink_0());
                }
                let kind = crate::icons::pane_kind(&pane.extra, pane.agent.as_deref());
                metadata = metadata
                    .child(
                        crate::icons::agent_icon(kind.as_deref().unwrap_or("terminal"), p.muted)
                            .size(px(10.))
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .id("titlebar-agent")
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .child(kind.unwrap_or_default()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.))
                            .text_color(rgb(p.muted))
                            .child("·"),
                    );
            }
            metadata = metadata.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(p.muted))
                    .id("titlebar-space")
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(self.space_label()),
            );
            if self.devices.len() > 1 {
                metadata = metadata.child(
                    div()
                        .id("titlebar-device")
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_size(px(10.))
                        .text_color(rgb(p.muted))
                        .px_1()
                        .rounded(px(4.))
                        .bg(rgb(p.active))
                        .child(self.device().name.clone()),
                );
            }
            if let Some(info) = self.git_metadata.as_ref() {
                if let Some(branch) = info.branch.clone() {
                    metadata = metadata
                        .child(crate::icons::icon("branch", p.muted).flex_shrink_0())
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(p.muted))
                                .font_family("Menlo")
                                .id("titlebar-branch")
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .child(branch),
                        );
                }
                if info.is_worktree {
                    metadata = metadata.child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_size(px(10.))
                            .text_color(rgb(p.muted))
                            .px_1()
                            .rounded(px(8.))
                            .bg(rgb(p.active))
                            .child(tr("Worktree")),
                    );
                }
            }
            titlebar = titlebar.child(metadata);
            if let Some(pane_id) = self.selected_pane.clone() {
                titlebar = titlebar.child(
                    self.btn_raw(
                        "copy-herdr-location",
                        pane_id.clone(),
                        UiAction::CopyLocation(pane_id),
                        p,
                        cx,
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex_shrink_0()
                    .text_size(px(11.))
                    .font_family("Menlo")
                    .text_color(rgb(p.muted))
                    .tooltip(|_, cx| cx.new(|_| ShellTooltip(tr("复制 Herdr 位置"))).into()),
                );
            }
        }
        root = root.child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .when(self.sidebar && self.screen != Screen::Settings, |d| {
                    d.child(self.render_sidebar(p, window, cx)).child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .flex_shrink_0()
                            .bg(rgb(p.line))
                            .cursor_col_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.sidebar_resizing = true;
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .when(self.screen != Screen::Settings, |d| d.child(titlebar))
                        .child(content),
                ),
        );
        if let Some((_, pane)) = self.pi_waiting.clone() {
            root = root.child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x00000066))
                    .flex()
                    .items_center()
                    .justify_center()
                    .key_context("OverlayPopup")
                    .on_action(cx.listener(|this, _: &Escape, window, cx| {
                        this.dispatch(UiAction::Dismiss, window, cx);
                    }))
                    .child(
                        div()
                            .w(px(360.))
                            .p_5()
                            .bg(rgb(p.raised))
                            .border_1()
                            .border_color(rgb(p.line))
                            .rounded(px(8.))
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(tr("Pi 启动尚未就绪，可检查状态或显示终端"))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(self.btn(
                                        "pi-check",
                                        "再检查一次",
                                        UiAction::CheckPi(pane.clone()),
                                        p,
                                        cx,
                                    ))
                                    .child(self.btn(
                                        "pi-show",
                                        "显示终端",
                                        UiAction::SelectPane(pane),
                                        p,
                                        cx,
                                    )),
                            ),
                    ),
            );
        }
        if let Some(overlay) = self.render_overlay(p, cx) {
            root = root.child(overlay);
        }
        if self.device_picker {
            if let Some(picker) = self.render_device_picker(p, window, cx) {
                root = root.child(picker);
            }
        }
        if let Some(picker) = self.render_space_picker(window, cx) {
            root = root.child(picker);
        }
        if let Some((at, rows, selected)) = &self.menu {
            root = root.child(self.render_context_menu(*at, rows, selected, p, window, cx));
        }
        if let Some(feedback) = self.render_feedback(p, cx) {
            root = root.child(feedback);
        }
        if folder_drop {
            root = root.child(folder_drop_overlay(p));
        }
        root.on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
            if this.device_picker && this.menu.is_none() {
                let len = this.devices.len();
                if len == 0 {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" => {
                        this.device_picker_index =
                            menu_step(this.device_picker_index.min(len - 1), len, false)
                    }
                    "down" => {
                        this.device_picker_index =
                            menu_step(this.device_picker_index.min(len - 1), len, true)
                    }
                    "enter" => {
                        let index = this.device_picker_index.min(len - 1);
                        this.dispatch(UiAction::SelectDevice(index), window, cx);
                    }
                    "escape" => this.close_overlay(window, cx),
                    _ => return,
                }
                this.device_picker_scroll
                    .scroll_to_item(this.device_picker_index + 1);
                cx.stop_propagation();
                cx.notify();
            }
        }))
    }
}

/// Scrim shown while Finder is dragging folders over the window.
fn folder_drop_overlay(p: Palette) -> Div {
    div()
        .absolute()
        .inset_0()
        .bg(rgba(0x00000066))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .px_4()
                .py_2()
                .bg(rgb(p.raised))
                .border_1()
                .border_color(rgb(p.line))
                .rounded(px(8.))
                .text_size(px(13.))
                .text_color(rgb(p.text))
                .child(tr("松开以添加为空间")),
        )
}

fn sidebar_group_status<'a>(
    statuses: impl Iterator<Item = (Option<&'a str>, bool)>,
) -> (Option<&'static str>, bool) {
    let rank = statuses
        .map(|(status, unread)| match status {
            Some("blocked") => 0,
            Some("done") if unread => 1,
            Some("working") => 2,
            _ => 3,
        })
        .min()
        .unwrap_or(3);
    match rank {
        0 => (Some("blocked"), false),
        1 => (Some("done"), true),
        2 => (Some("working"), false),
        _ => (None, false),
    }
}
// ponytail: stable snapshot order within each priority group; add recency state only when the backend supplies it.
fn session_rank(status: Option<&str>, unread: bool) -> u8 {
    match priority::Attention::of(status, unread) {
        priority::Attention::Blocked => 0,
        priority::Attention::UnreadDone => 1,
        priority::Attention::Working => 2,
        priority::Attention::None => 3,
    }
}
fn drop_before<'a>(ids: &[&'a str], moving: &str, target: &str, after: bool) -> Option<&'a str> {
    let index = ids.iter().position(|id| *id == target)?;
    ids.iter()
        .skip(index + usize::from(after))
        .copied()
        .find(|id| *id != moving)
}
// The service indexes the original list, then removes the source itself.
fn drop_index(ids: &[&str], moving: &str, target: &str, after: bool) -> Option<usize> {
    let from = ids.iter().position(|id| *id == moving)?;
    let target_index = ids.iter().position(|id| *id == target)?;
    let index = target_index + usize::from(after);
    if moving == target || index == from || index == from + 1 {
        None
    } else {
        Some(index)
    }
}

// Available space above the trigger; scrolling keeps the menu clear of the footer.
fn device_picker_bounds(
    trigger: Bounds<Pixels>,
    viewport: Size<Pixels>,
    inset: Pixels,
) -> Bounds<Pixels> {
    let margin = inset + px(8.);
    let width = trigger
        .size
        .width
        .max(px(240.))
        .min(px(280.))
        .min((viewport.width - margin * 2.).max(px(0.)));
    let left = trigger
        .left()
        .max(margin)
        .min((viewport.width - margin - width).max(margin));
    let bottom = (trigger.top() - px(8.)).min(viewport.height - margin);
    Bounds::new(
        point(left, margin),
        size(width, (bottom - margin).max(px(0.))),
    )
}
struct ShellTooltip(String);
impl Render for ShellTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = gpui_component::Theme::global(cx).is_dark();
        let p = Palette::new(dark);
        div().child(
            div()
                .m_2()
                .px_2()
                .py(px(6.))
                .max_w(px(420.))
                .bg(rgb(p.raised))
                .border_1()
                .border_color(rgb(p.tooltip_border))
                .text_color(rgb(p.tooltip_text))
                .text_size(px(14.))
                .line_height(px(18.))
                .rounded(px(8.))
                .when(!dark, |d| d.shadow_sm())
                .child(self.0.clone()),
        )
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn sidebar_group_status_prioritizes_attention() {
        use super::sidebar_group_status;
        assert_eq!(sidebar_group_status([].into_iter()), (None, false));
        assert_eq!(
            sidebar_group_status([(Some("done"), false), (None, true)].into_iter()),
            (None, false)
        );
        assert_eq!(
            sidebar_group_status([(Some("done"), false), (Some("working"), false)].into_iter()),
            (Some("working"), false)
        );
        assert_eq!(
            sidebar_group_status([(Some("working"), true), (Some("done"), true)].into_iter()),
            (Some("done"), true)
        );
        assert_eq!(
            sidebar_group_status([(Some("done"), true), (Some("blocked"), false)].into_iter()),
            (Some("blocked"), false)
        );
    }
    #[test]
    fn context_menu_selection_and_groups() {
        use super::UiAction;
        use super::context_menu::{menu_enabled, menu_selection, menu_separator};
        assert_eq!(menu_selection(None, 0, true, |_| true), None);
        assert_eq!(menu_selection(None, 3, true, |_| true), Some(0));
        assert_eq!(menu_selection(None, 3, false, |_| true), Some(2));
        assert_eq!(menu_selection(Some(2), 3, true, |_| true), Some(0));
        assert_eq!(menu_selection(Some(0), 3, false, |_| true), Some(2));
        assert_eq!(menu_selection(None, 3, true, |i| i == 2), Some(2));
        assert_eq!(menu_selection(Some(2), 3, true, |i| i == 2), Some(2));
        assert_eq!(menu_selection(None, 3, false, |_| false), None);
        for command in [
            super::DeviceInputCommand::Cut,
            super::DeviceInputCommand::Copy,
        ] {
            assert!(!menu_enabled(&UiAction::EditDeviceInput(3, command, true)));
            assert!(!menu_enabled(&UiAction::EditDeviceInput(4, command, true)));
            assert!(menu_enabled(&UiAction::EditDeviceInput(0, command, true)));
            assert!(!menu_enabled(&UiAction::EditDeviceInput(0, command, false)));
        }
        assert!(menu_separator(&UiAction::EditDeviceInput(
            0,
            super::DeviceInputCommand::SelectAll,
            true
        )));
        assert!(menu_separator(&UiAction::CloseSpace("a".into())));
        assert!(menu_separator(&UiAction::CloseRemote("a".into())));
        assert!(menu_separator(&UiAction::DeleteDevice(1)));
        assert!(!menu_separator(&UiAction::RenamePane("a".into())));
    }

    #[test]
    fn new_space_uses_server_name_and_optional_directory() {
        use super::new_space_params;
        use serde_json::json;

        for cwd in ["", "  "] {
            assert_eq!(new_space_params(cwd).unwrap(), json!({"focus": false}));
        }
        for cwd in ["/Users/example/项目", "/", "~/work", "/remote/my project/"] {
            assert_eq!(
                new_space_params(&format!("  {cwd}  ")).unwrap(),
                json!({"focus": false, "cwd": cwd})
            );
        }
        assert!(new_space_params("/invalid\0path").is_err());
    }

    #[test]
    fn new_space_label_uses_folder_name() {
        assert_eq!(super::empty_space_label("/Users/example/skills"), "skills");
        assert_eq!(super::empty_space_label("/"), "/");
    }

    #[test]
    fn folder_drop_keeps_only_existing_directories() {
        let root =
            std::env::temp_dir().join(format!("goose-herdr-folder-drop-{}", std::process::id()));
        let space = root.join("space");
        std::fs::create_dir_all(&space).unwrap();
        std::fs::write(root.join("file.txt"), b"x").unwrap();
        let dropped = [space.clone(), root.join("file.txt"), root.join("missing")];
        assert_eq!(
            super::dropped_space_directories(&dropped),
            [space.to_str().unwrap().to_owned()]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn inline_space_name_validation() {
        assert_eq!(super::space_name("  我的空间 🪿  ").unwrap(), "我的空间 🪿");
        for invalid in ["", " \t", "bad\0name", "bad\nname"] {
            assert!(super::space_name(invalid).is_err());
        }
    }

    #[test]
    fn device_menu_alignment_and_safe_edges() {
        use gpui::{Bounds, point, px, size};
        let viewport = size(px(720.), px(480.));
        let trigger = Bounds::new(point(px(12.), px(440.)), size(px(100.), px(28.)));
        let menu = super::device_picker_bounds(trigger, viewport, px(4.));
        assert_eq!(menu.left(), trigger.left());
        assert_eq!(trigger.top() - menu.bottom(), px(8.));
        assert_eq!(menu.top(), px(12.));
        let edge = super::device_picker_bounds(
            Bounds::new(point(px(700.), px(440.)), trigger.size),
            viewport,
            px(4.),
        );
        assert_eq!(edge.right(), viewport.width - px(12.));
        assert_eq!(crate::herdr::Device::local().name, "Local");
    }

    #[test]
    fn empty_space_agents_follow_enabled_catalog_order() {
        let settings = crate::settings::Settings {
            agent_kind_order: vec!["pi".into(), "codex".into(), "pi".into(), "missing".into()],
            disabled_agent_kinds: vec!["codex".into()],
            ..Default::default()
        };
        let catalog = [
            "codex", "claude", "pi", "terminal", "gemini", "kimi", "grok",
        ]
        .map(str::to_owned);
        let kinds = super::enabled_agent_kinds(&settings, &catalog);
        assert_eq!(kinds, ["pi", "claude", "gemini", "kimi", "grok"]);
        assert_eq!(
            kinds.chunks(4).map(|row| row.len()).collect::<Vec<_>>(),
            [4, 1]
        );
        assert_eq!(
            super::available_agent_kinds(&settings, &catalog),
            ["pi", "codex", "claude", "gemini", "kimi", "grok"]
        );
        // No install scan yet: nothing is offered rather than guessing known agents.
        assert!(super::enabled_agent_kinds(&settings, &[]).is_empty());
        assert!(super::available_agent_kinds(&settings, &[]).is_empty());
        let disabled = crate::settings::Settings {
            disabled_agent_kinds: catalog.to_vec(),
            ..Default::default()
        };
        assert!(super::enabled_agent_kinds(&disabled, &catalog).is_empty());
        assert!(super::available_agent_kinds(&disabled, &catalog).contains(&"codex".to_owned()));
    }

    #[test]
    fn generated_agent_name_avoids_existing_names() {
        let agents = ["pi", "pi-2"].map(|name| crate::herdr::Agent {
            name: Some(name.into()),
            ..Default::default()
        });
        assert_eq!(super::unique_agent_name("pi", &[]), "pi");
        assert_eq!(super::unique_agent_name("pi", &agents), "pi-3");
    }

    use super::{drop_before, quote, session_rank, valid_agent_name};
    #[test]
    fn project_icon_cwd_prefers_workspace_and_ignores_other_spaces() {
        let mut snapshot: crate::herdr::Snapshot = serde_json::from_value(serde_json::json!({
            "workspaces": [{"workspace_id":"a", "cwd":" /project "}],
            "agents": [],
            "panes": [
                {"pane_id":"1", "workspace_id":"other", "cwd":"/wrong"},
                {"pane_id":"2", "workspace_id":"a", "cwd":"~/fallback"}
            ]
        }))
        .unwrap();
        assert_eq!(super::workspace_icon_path(&snapshot, "a"), Some("/project"));
        snapshot.workspaces[0].cwd = Some(" ".into());
        assert_eq!(
            super::workspace_icon_path(&snapshot, "a"),
            Some("~/fallback")
        );
        assert_eq!(super::workspace_icon_path(&snapshot, "missing"), None);
        assert_eq!(
            super::space_path(&snapshot, "a").as_deref(),
            Some("~/fallback")
        );
        snapshot.workspaces[0].cwd = Some("/project".into());
        assert_eq!(
            super::space_path(&snapshot, "a").as_deref(),
            Some("/project")
        );
    }
    #[test]
    fn session_title_follows_herdr_priority() {
        assert_eq!(
            super::resolved_session_title(
                Some("custom"),
                Some("tab"),
                Some("term"),
                Some("pi-abcd"),
                "pi",
                Some("/tmp"),
                "wAM:p3",
            ),
            "custom"
        );
        assert_eq!(
            super::resolved_session_title(
                None,
                Some("pi"),
                Some("goose-herdr-gpui"),
                Some("pi"),
                "pi",
                Some("/Users/eachann/Work/goose-herdr-gpui"),
                "wAM:p3",
            ),
            // A cwd-only OSC title is generic, so preserve the Herdr name.
            "pi"
        );
    }
    #[test]
    fn trust_boundaries() {
        assert_eq!(super::menu_step(0, 3, false), 2);
        assert_eq!(super::menu_step(2, 3, true), 0);
        assert_eq!(super::menu_step(0, 0, true), 0);
        assert_eq!(session_rank(Some("blocked"), true), 0);
        assert_eq!(session_rank(Some("done"), true), 1);
        assert_eq!(session_rank(Some("working"), true), 2);
        assert_eq!(drop_before(&["a", "b", "c"], "a", "c", true), None);
        assert_eq!(drop_before(&["a", "b", "c"], "c", "a", false), Some("a"));
        assert_eq!(drop_before(&["a", "b", "c"], "b", "a", true), Some("c"));
        assert!(valid_agent_name("build_1"));
        assert!(!valid_agent_name("Bad"));
        assert!(!valid_agent_name("a;rm"));
        assert_eq!(quote("a'b"), "'a'\\''b'");
    }
}

fn random_hex4() -> String {
    new_uuid()[..4].to_ascii_lowercase()
}

fn unique_agent_name(kind: &str, agents: &[herdr::Agent]) -> String {
    let mut name = kind.to_owned();
    let mut suffix = 2;
    while agents
        .iter()
        .any(|agent| agent.name.as_deref() == Some(name.as_str()))
    {
        name = format!("{kind}-{suffix}");
        suffix += 1;
    }
    name
}

fn suffixed_agent_name(kind: &str) -> String {
    format!("{kind}-{}", random_hex4())
}

fn new_uuid() -> String {
    let mut b = [0u8; 16];
    unsafe {
        libc::arc4random_buf(b.as_mut_ptr().cast(), b.len());
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7],
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    )
}

fn set_split_ratio(tree: &mut SplitTree, path: &[bool], value: f32) {
    if let SplitTree::Split {
        ratio,
        first,
        second,
        ..
    } = tree
    {
        if let Some((direction, rest)) = path.split_first() {
            set_split_ratio(if *direction { second } else { first }, rest, value);
        } else {
            *ratio = value;
        }
    }
}

fn image_format(path: &std::path::Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::Webp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "bmp" => Some(ImageFormat::Bmp),
        _ => None,
    }
}

fn bypass_flag(kind: &str) -> Option<&'static str> {
    match kind {
        "claude" => Some("--dangerously-skip-permissions"),
        "codex" => Some("--dangerously-bypass-approvals-and-sandbox"),
        "grok" => Some("--always-approve"),
        "gemini" => Some("--yolo"),
        "opencode" => Some("--auto"),
        "cursor" => Some("--force"),
        "copilot" => Some("--allow-all-tools"),
        _ => None,
    }
}

impl Drop for DeviceState {
    fn drop(&mut self) {
        if let Some(cancel) = self.event_cancel.take() {
            let _ = cancel.shutdown(std::net::Shutdown::Both);
        }
    }
}

fn resolved_session_title(
    custom_title: Option<&str>,
    tab_label: Option<&str>,
    terminal_title: Option<&str>,
    name: Option<&str>,
    agent_kind: &str,
    cwd: Option<&str>,
    pane_id: &str,
) -> String {
    let nonempty = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    if let Some(title) = nonempty(custom_title) {
        return title;
    }
    if let Some(label) = nonempty(tab_label) {
        if !generic_tab_label(&label, agent_kind, name) {
            return label;
        }
    }
    if let Some(name) = nonempty(name) {
        if !auto_generated_name(&name, agent_kind) {
            return name;
        }
    }
    if let Some(title) = nonempty(terminal_title) {
        if !generic_terminal_title(&title, agent_kind, cwd) && title != pane_id {
            return title;
        }
    }
    if let Some(name) = nonempty(name) {
        return name;
    }
    agent_kind.to_owned()
}

fn generic_tab_label(label: &str, agent_kind: &str, name: Option<&str>) -> bool {
    if label.eq_ignore_ascii_case(agent_kind) {
        return true;
    }
    name.is_some_and(|name| {
        let name = name.trim();
        !name.is_empty() && label == name && auto_generated_name(name, agent_kind)
    })
}

fn auto_generated_name(name: &str, agent_kind: &str) -> bool {
    if name.eq_ignore_ascii_case(agent_kind) {
        return true;
    }
    let prefix = format!("{agent_kind}-");
    name.to_ascii_lowercase()
        .strip_prefix(&prefix.to_ascii_lowercase())
        .is_some_and(|suffix| suffix.len() == 4 && suffix.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn generic_terminal_title(title: &str, agent_kind: &str, cwd: Option<&str>) -> bool {
    if title.eq_ignore_ascii_case(agent_kind) {
        return true;
    }
    let Some(cwd) = cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return false;
    };
    if title == cwd {
        return true;
    }
    let base = std::path::Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if base.is_empty() {
        return false;
    }
    if title == base {
        return true;
    }
    let stem = title
        .strip_suffix("...")
        .or_else(|| title.strip_suffix('\u{2026}'));
    stem.is_some_and(|stem| !stem.is_empty() && base.starts_with(stem))
}

fn space_path(snapshot: &Snapshot, workspace: &str) -> Option<String> {
    snapshot
        .workspaces
        .iter()
        .filter(|w| w.workspace_id == workspace)
        .filter_map(|w| w.cwd.as_deref())
        .chain(
            snapshot
                .agents
                .iter()
                .filter(|a| a.workspace_id == workspace)
                .filter_map(|a| a.cwd.as_deref()),
        )
        .chain(
            snapshot
                .panes
                .iter()
                .filter(|p| p.workspace_id == workspace)
                .filter_map(|p| p.cwd.as_deref()),
        )
        .map(str::trim)
        .find(|path| !path.is_empty())
        .map(str::to_owned)
}

fn workspace_icon_path<'a>(snapshot: &'a Snapshot, workspace: &str) -> Option<&'a str> {
    snapshot
        .workspaces
        .iter()
        .filter(|w| w.workspace_id == workspace)
        .filter_map(|w| w.cwd.as_deref())
        .chain(
            snapshot
                .agents
                .iter()
                .filter(|a| a.workspace_id == workspace)
                .filter_map(|a| a.cwd.as_deref()),
        )
        .chain(
            snapshot
                .panes
                .iter()
                .filter(|p| p.workspace_id == workspace)
                .filter_map(|p| p.cwd.as_deref()),
        )
        .map(str::trim)
        .find(|path| path.starts_with('/') || path.starts_with("~/") || *path == "~")
}

fn split_shell_cwd(
    device: &Device,
    source_id: Option<&str>,
    snapshot: Option<&Snapshot>,
    workspace_id: Option<&str>,
) -> Option<PathBuf> {
    if device.is_local() {
        if let (Some(source), Some(snapshot)) = (source_id, snapshot) {
            let prefix = format!("{}:", device.id);
            if let Some(pane_id) = source.strip_prefix(&prefix) {
                let cwd = snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == pane_id)
                    .and_then(|pane| pane.cwd.clone())
                    .or_else(|| {
                        snapshot
                            .agents
                            .iter()
                            .find(|agent| agent.pane_id == pane_id)
                            .and_then(|agent| agent.cwd.clone())
                    });
                if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
                    return Some(PathBuf::from(cwd));
                }
            }
        }
    }
    snapshot
        .and_then(|snapshot| {
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace_id.is_some_and(|id| workspace.workspace_id == id))
        })
        .and_then(|workspace| workspace.cwd.as_ref())
        .filter(|cwd| !cwd.trim().is_empty())
        .map(PathBuf::from)
}

fn next_pane_drag_token() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone)]
struct TerminalPaneDrag {
    source: String,
    root: SplitTree,
    workspace: Option<String>,
    token: u64,
    label: String,
}

impl Render for TerminalPaneDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .bg(rgb(0x2d2f34))
            .text_color(rgb(0xffffff))
            .rounded(px(4.))
            .child(self.label.clone())
    }
}

#[derive(Clone)]
struct ObjectDrag {
    device: String,
    id: String,
    workspace: Option<String>,
    label: String,
    palette: Palette,
}
impl ObjectDrag {
    fn ids<'a>(&self, snapshot: &'a Snapshot, device: &str, target: &str) -> Option<Vec<&'a str>> {
        if self.device != device || self.id == target {
            return None;
        }
        let ids: Vec<_> = if let Some(workspace) = &self.workspace {
            if !snapshot.workspaces.iter().any(|w| {
                &w.workspace_id == workspace && w.extra.get("retained") != Some(&Value::Bool(true))
            }) {
                return None;
            }
            snapshot
                .tabs
                .iter()
                .filter(|t| &t.workspace_id == workspace)
                .map(|t| t.tab_id.as_str())
                .collect()
        } else {
            snapshot
                .workspaces
                .iter()
                .filter(|w| w.extra.get("retained") != Some(&Value::Bool(true)))
                .map(|w| w.workspace_id.as_str())
                .collect()
        };
        (ids.contains(&self.id.as_str()) && ids.contains(&target)).then_some(ids)
    }
}

#[cfg(test)]
mod object_reorder_tests {
    use super::{ObjectDrag, Palette, Snapshot, drop_index};
    use serde_json::json;

    #[test]
    fn object_reorder_boundaries_and_scope() {
        let ids = ["a", "b", "c", "d"];
        for from in 0..ids.len() {
            for target in 0..ids.len() {
                for after in [false, true] {
                    let mut expected = ids.to_vec();
                    if from != target {
                        expected.remove(from);
                        let index = expected.iter().position(|id| *id == ids[target]).unwrap();
                        expected.insert(index + usize::from(after), ids[from]);
                    }
                    let plan = drop_index(&ids, ids[from], ids[target], after);
                    assert_eq!(plan.is_none(), expected == ids);
                    if let Some(index) = plan {
                        let mut actual = ids.to_vec();
                        actual.remove(from);
                        actual.insert(index - usize::from(from < index), ids[from]);
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
        assert_eq!(drop_index(&ids, "missing", "a", false), None);
        assert_eq!(drop_index(&ids, "a", "missing", true), None);
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id":"one"}, {"workspace_id":"two"}, {"workspace_id":"saved","retained":true}],
            "tabs": [{"tab_id":"a","workspace_id":"one"}, {"tab_id":"b","workspace_id":"two"}, {"tab_id":"c","workspace_id":"one"}, {"tab_id":"s","workspace_id":"saved"}]
        })).unwrap();
        let mut drag = ObjectDrag {
            device: "local".into(),
            id: "a".into(),
            workspace: Some("one".into()),
            label: String::new(),
            palette: Palette::new(false),
        };
        assert_eq!(drag.ids(&snapshot, "local", "c"), Some(vec!["a", "c"]));
        for target in ["a", "b", "s", "missing"] {
            assert!(drag.ids(&snapshot, "local", target).is_none());
        }
        assert!(drag.ids(&snapshot, "other-device", "c").is_none());
        drag.workspace = None;
        drag.id = "one".into();
        assert_eq!(
            drag.ids(&snapshot, "local", "two"),
            Some(vec!["one", "two"])
        );
        assert!(drag.ids(&snapshot, "local", "saved").is_none());
        drag.id = "saved".into();
        assert!(drag.ids(&snapshot, "local", "one").is_none());
    }
}

impl Render for ObjectDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .max_w(px(280.))
            .truncate()
            .text_size(px(13.))
            .bg(rgb(self.palette.side))
            .text_color(rgb(self.palette.text))
            .border_1()
            .border_color(rgb(self.palette.tooltip_border))
            .rounded(px(3.))
            .shadow_sm()
            .child(self.label.clone())
    }
}

impl Drop for AppView {
    fn drop(&mut self) {
        if let Some(token) = &self.transfer {
            token.cancel();
        }
    }
}

#[cfg(test)]
mod scope_tests {
    use super::{ScopeView, exchange_scope};
    use crate::layout::SplitTree;
    use std::collections::HashMap;
    #[test]
    fn switching_device_or_space_never_reuses_the_previous_terminal() {
        let mut views = HashMap::new();
        let a = ("device-a".into(), Some("space-1".into()));
        let b = ("device-b".into(), Some("space-1".into()));
        let c = ("device-a".into(), Some("space-2".into()));
        let original = ScopeView {
            split: Some(SplitTree::new("device-a:pane-1")),
            active: Some("device-a:pane-1".into()),
            selected: Some("pane-1".into()),
        };
        let blank = exchange_scope(&mut views, a.clone(), b.clone(), original);
        assert!(blank.active.is_none() && blank.split.is_none());
        let blank = exchange_scope(&mut views, b, c.clone(), blank);
        assert!(blank.active.is_none() && blank.selected.is_none());
        let restored = exchange_scope(&mut views, c, a, blank);
        assert_eq!(restored.active.as_deref(), Some("device-a:pane-1"));
        assert_eq!(restored.selected.as_deref(), Some("pane-1"));
        assert_eq!(restored.split.unwrap().leaves(), ["device-a:pane-1"]);
    }
}

fn prompts_for_new_session_space(
    priority: bool,
    snapshot: Option<&Snapshot>,
    pane: Option<&str>,
    workspace: Option<&str>,
) -> bool {
    if quick_agent_target(snapshot, pane, workspace).is_none() {
        return true;
    }
    if priority {
        return false;
    }
    workspace.is_none_or(|id| {
        !snapshot.is_some_and(|snapshot| {
            snapshot
                .workspaces
                .iter()
                .any(|item| item.workspace_id == id)
        })
    })
}

fn session_workspace_id<'a>(snapshot: &'a Snapshot, pane: &str) -> Option<&'a str> {
    snapshot
        .panes
        .iter()
        .find(|item| item.pane_id == pane)
        .map(|item| item.workspace_id.as_str())
        .or_else(|| {
            snapshot
                .agents
                .iter()
                .find(|item| item.pane_id == pane)
                .map(|item| item.workspace_id.as_str())
        })
        .filter(|id| !id.is_empty())
}

fn quick_agent_target(
    snapshot: Option<&Snapshot>,
    pane: Option<&str>,
    workspace: Option<&str>,
) -> Option<(String, bool)> {
    let workspace = pane
        .and_then(|pane| snapshot.and_then(|snapshot| session_workspace_id(snapshot, pane)))
        .or(workspace.filter(|id| !id.is_empty()))?;
    let revive = snapshot.is_some_and(|s| {
        s.workspaces.iter().any(|w| {
            w.workspace_id == workspace && w.extra.get("retained") == Some(&Value::Bool(true))
        })
    });
    Some((workspace.to_owned(), revive))
}

fn created_pane_to_attach(
    device: &str,
    pending: Option<&(String, String)>,
    snapshot: Option<&Snapshot>,
) -> Option<String> {
    let (owner, pane) = pending?;
    // Terminal attachment does not require the agent's interactive-ready signal.
    (owner == device && snapshot?.panes.iter().any(|p| &p.pane_id == pane)).then(|| pane.clone())
}

#[cfg(test)]
mod created_pane_tests {
    use super::{
        Snapshot, created_pane_to_attach, prompts_for_new_session_space, quick_agent_target,
    };
    use serde_json::json;

    #[test]
    fn quick_agent_uses_current_session_before_sidebar_scope() {
        let snapshot: Snapshot = serde_json::from_value(json!({"panes": [{
            "pane_id": "pane-1", "workspace_id": "space-1"
        }]}))
        .unwrap();
        assert_eq!(
            quick_agent_target(Some(&snapshot), Some("pane-1"), None),
            Some(("space-1".into(), false))
        );
        assert_eq!(
            quick_agent_target(Some(&snapshot), Some("pane-1"), Some("space-2")),
            Some(("space-1".into(), false))
        );
        assert_eq!(
            quick_agent_target(Some(&snapshot), None, Some("space-2")),
            Some(("space-2".into(), false))
        );
        assert_eq!(
            quick_agent_target(Some(&snapshot), Some("missing"), None),
            None
        );
        assert_eq!(quick_agent_target(None, None, None), None);
    }

    #[test]
    fn quick_agent_restores_selected_empty_project_without_a_picker() {
        for retained in [false, true] {
            let snapshot: Snapshot = serde_json::from_value(json!({"workspaces": [{
                "workspace_id": "harmony", "retained": retained
            }], "panes": []}))
            .unwrap();
            assert_eq!(
                quick_agent_target(Some(&snapshot), None, Some("harmony")),
                Some(("harmony".into(), retained))
            );
            assert_eq!(quick_agent_target(Some(&snapshot), None, None), None);
        }
    }

    #[test]
    fn priority_mode_adds_to_current_session_space_without_a_picker() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space-1"}, {"workspace_id": "space-2"}],
            "panes": [{"pane_id": "pane-1", "workspace_id": "space-1"}]
        }))
        .unwrap();
        assert!(!prompts_for_new_session_space(
            true,
            Some(&snapshot),
            Some("pane-1"),
            None,
        ));
        assert_eq!(
            quick_agent_target(Some(&snapshot), Some("pane-1"), None),
            Some(("space-1".into(), false))
        );
        assert!(prompts_for_new_session_space(
            false,
            Some(&snapshot),
            Some("pane-1"),
            None,
        ));
        assert!(prompts_for_new_session_space(
            true,
            Some(&snapshot),
            None,
            None,
        ));
    }

    #[test]
    fn priority_mode_adds_to_selected_project_without_a_picker() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space-1"}, {"workspace_id": "space-2"}],
            "panes": []
        }))
        .unwrap();
        assert!(!prompts_for_new_session_space(
            true,
            Some(&snapshot),
            None,
            Some("space-1"),
        ));
        assert_eq!(
            quick_agent_target(Some(&snapshot), None, Some("space-1")),
            Some(("space-1".into(), false))
        );
        assert!(!prompts_for_new_session_space(
            false,
            Some(&snapshot),
            None,
            Some("space-1"),
        ));
    }

    #[test]
    fn priority_mode_uses_agent_session_space_when_pane_list_omits_it() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space-1"}],
            "agents": [{"pane_id": "pi-1", "workspace_id": "space-1", "tab_id": "tab-1"}],
            "panes": []
        }))
        .unwrap();
        assert_eq!(
            quick_agent_target(Some(&snapshot), Some("pi-1"), None),
            Some(("space-1".into(), false))
        );
        assert!(!prompts_for_new_session_space(
            true,
            Some(&snapshot),
            Some("pi-1"),
            None,
        ));
    }

    #[test]
    fn created_terminal_opens_without_pi_readiness_but_never_on_another_device() {
        let pending = ("device-a".to_owned(), "pane-1".to_owned());
        let snapshot: Snapshot = serde_json::from_value(json!({"panes": [{
            "pane_id": "pane-1", "workspace_id": "space-1",
            "interactive_ready": false, "launch_pending": true
        }]}))
        .unwrap();
        let candidate = |device, snapshot| created_pane_to_attach(device, Some(&pending), snapshot);
        assert_eq!(
            candidate("device-a", Some(&snapshot)),
            Some("pane-1".into())
        );
        assert_eq!(candidate("device-b", Some(&snapshot)), None);
        assert_eq!(candidate("device-a", None), None);
        assert_eq!(candidate("device-a", Some(&Snapshot::default())), None);
        assert_eq!(
            created_pane_to_attach("device-a", None, Some(&snapshot)),
            None
        );
    }
}

fn drop_closed_pane_from_snapshot(snapshot: &mut Snapshot, pane: &str) {
    snapshot.panes.retain(|item| item.pane_id != pane);
    snapshot.agents.retain(|item| item.pane_id != pane);
}

fn next_in_session_list(closed: &str, ids: &[&str]) -> Option<String> {
    let index = ids.iter().position(|id| *id == closed)?;
    ids.get(index + 1)
        .copied()
        .or_else(|| index.checked_sub(1).and_then(|i| ids.get(i).copied()))
        .map(str::to_owned)
}

fn next_session_after_close(
    closed: &str,
    snapshot: &Snapshot,
    ordered: Option<&[String]>,
) -> Option<String> {
    if let Some(ids) = ordered {
        let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        return next_in_session_list(closed, &ids);
    }
    let workspace = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == closed)?
        .workspace_id
        .as_str();
    let ids = snapshot
        .panes
        .iter()
        .filter(|pane| pane.workspace_id == workspace)
        .map(|pane| pane.pane_id.as_str())
        .collect::<Vec<_>>();
    next_in_session_list(closed, &ids)
}

fn restore_session_after_snapshot(
    selected: Option<&str>,
    workspace: Option<&str>,
    old: &Snapshot,
    new: &Snapshot,
    ordered: Option<&[String]>,
) -> Option<String> {
    if selected.is_some_and(|id| new.panes.iter().any(|pane| pane.pane_id == id)) {
        return None;
    }
    let closed = selected
        .filter(|id| old.panes.iter().any(|pane| pane.pane_id == *id))
        .map(str::to_owned)
        .or_else(|| {
            let space = workspace?;
            old.panes.iter().find_map(|pane| {
                (pane.workspace_id == space
                    && !new.panes.iter().any(|item| item.pane_id == pane.pane_id))
                .then(|| pane.pane_id.clone())
            })
        })?;
    next_session_after_close(&closed, old, ordered)
        .filter(|next| new.panes.iter().any(|pane| &pane.pane_id == next))
}

fn workspace_has_no_sessions(workspace: &str, snapshot: &Snapshot) -> bool {
    snapshot
        .workspaces
        .iter()
        .any(|item| item.workspace_id == workspace)
        && !snapshot
            .panes
            .iter()
            .any(|pane| pane.workspace_id == workspace)
}

fn directory_is_empty(read: &Option<Result<(), String>>, loading: bool, no_entries: bool) -> bool {
    !loading && no_entries && matches!(read, Some(Ok(())))
}

#[cfg(test)]
mod directory_illustration_tests {
    use super::directory_is_empty;
    #[test]
    fn directory_illustration_requires_a_successful_empty_read() {
        assert!(directory_is_empty(&Some(Ok(())), false, true));
        assert!(!directory_is_empty(&None, false, true));
        assert!(!directory_is_empty(
            &Some(Err("offline".into())),
            false,
            true
        ));
        assert!(!directory_is_empty(&Some(Ok(())), true, true));
        assert!(!directory_is_empty(&Some(Ok(())), false, false));
    }
}

#[cfg(test)]
mod close_session_tests {
    use super::{DeviceState, Snapshot};
    use super::{
        UiAction, close_terminal_action, drop_closed_pane_from_snapshot, next_session_after_close,
        restore_session_after_snapshot, workspace_has_no_sessions,
    };
    use serde_json::json;
    #[test]
    fn remote_close_is_not_a_local_detach() {
        assert!(
            matches!(close_terminal_action("device", "device:pane"), UiAction::CloseRemote(p) if p == "pane")
        );
        assert!(
            matches!(close_terminal_action("device", "shell-1"), UiAction::Detach(p) if p == "shell-1")
        );
    }
    #[test]
    fn last_closed_session_is_empty_space_not_pick_a_session() {
        let mut snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [{"pane_id": "only", "workspace_id": "space"}],
            "agents": [{"pane_id": "only", "workspace_id": "space", "tab_id": "tab"}]
        }))
        .unwrap();
        assert!(!workspace_has_no_sessions("space", &snapshot));
        drop_closed_pane_from_snapshot(&mut snapshot, "only");
        assert!(workspace_has_no_sessions("space", &snapshot));
        assert!(snapshot.agents.is_empty());
    }
    #[test]
    fn closing_one_session_keeps_the_space_occupied() {
        let mut snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [
                {"pane_id": "a", "workspace_id": "space"},
                {"pane_id": "b", "workspace_id": "space"}
            ]
        }))
        .unwrap();
        drop_closed_pane_from_snapshot(&mut snapshot, "a");
        assert!(!workspace_has_no_sessions("space", &snapshot));
        assert_eq!(snapshot.panes.len(), 1);
    }
    #[test]
    fn closing_a_session_selects_the_next_then_previous() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [
                {"pane_id": "a", "workspace_id": "space"},
                {"pane_id": "b", "workspace_id": "space"},
                {"pane_id": "c", "workspace_id": "space"}
            ]
        }))
        .unwrap();
        assert_eq!(
            next_session_after_close("a", &snapshot, None).as_deref(),
            Some("b")
        );
        assert_eq!(
            next_session_after_close("b", &snapshot, None).as_deref(),
            Some("c")
        );
        assert_eq!(
            next_session_after_close("c", &snapshot, None).as_deref(),
            Some("b")
        );
        assert_eq!(next_session_after_close("only", &snapshot, None), None);
    }
    #[test]
    fn last_session_has_no_neighbor_to_restore() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [{"pane_id": "only", "workspace_id": "space"}]
        }))
        .unwrap();
        assert_eq!(next_session_after_close("only", &snapshot, None), None);
    }
    #[test]
    fn priority_close_follows_ranked_list_across_spaces() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [
                {"workspace_id": "empty-soon"},
                {"workspace_id": "other"}
            ],
            "panes": [
                {"pane_id": "done", "workspace_id": "empty-soon"},
                {"pane_id": "jev", "workspace_id": "other"},
                {"pane_id": "retry", "workspace_id": "other"}
            ]
        }))
        .unwrap();
        assert_eq!(next_session_after_close("done", &snapshot, None), None);
        let ranked = ["done", "jev", "retry"].map(str::to_owned);
        assert_eq!(
            next_session_after_close("done", &snapshot, Some(&ranked)).as_deref(),
            Some("jev")
        );
        assert_eq!(
            next_session_after_close("jev", &snapshot, Some(&ranked)).as_deref(),
            Some("retry")
        );
        assert_eq!(
            next_session_after_close("retry", &snapshot, Some(&ranked)).as_deref(),
            Some("jev")
        );
    }
    #[test]
    fn snapshot_restores_next_session_after_the_selected_pane_vanishes() {
        let old: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [
                {"pane_id": "a", "workspace_id": "space"},
                {"pane_id": "b", "workspace_id": "space"}
            ]
        }))
        .unwrap();
        let new: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id": "space"}],
            "panes": [{"pane_id": "b", "workspace_id": "space"}]
        }))
        .unwrap();
        assert_eq!(
            restore_session_after_snapshot(Some("a"), Some("space"), &old, &new, None).as_deref(),
            Some("b")
        );
        assert_eq!(
            restore_session_after_snapshot(None, Some("space"), &old, &new, None).as_deref(),
            Some("b")
        );
        assert_eq!(
            restore_session_after_snapshot(Some("b"), Some("space"), &old, &new, None),
            None
        );
    }
    #[test]
    fn snapshot_restores_the_next_ranked_session_in_another_space() {
        let old: Snapshot = serde_json::from_value(json!({
            "workspaces": [
                {"workspace_id": "empty-soon"},
                {"workspace_id": "other"}
            ],
            "panes": [
                {"pane_id": "done", "workspace_id": "empty-soon"},
                {"pane_id": "jev", "workspace_id": "other"}
            ]
        }))
        .unwrap();
        let new: Snapshot = serde_json::from_value(json!({
            "workspaces": [
                {"workspace_id": "empty-soon"},
                {"workspace_id": "other"}
            ],
            "panes": [{"pane_id": "jev", "workspace_id": "other"}]
        }))
        .unwrap();
        assert_eq!(
            restore_session_after_snapshot(Some("done"), Some("empty-soon"), &old, &new, None,),
            None
        );
        let ranked = ["done", "jev"].map(str::to_owned);
        assert_eq!(
            restore_session_after_snapshot(
                Some("done"),
                Some("empty-soon"),
                &old,
                &new,
                Some(&ranked),
            )
            .as_deref(),
            Some("jev")
        );
    }
    #[test]
    fn replaced_subscription_cannot_report_a_disconnect() {
        let mut state = DeviceState {
            connection: None,
            snapshot: None,
            status: String::new(),
            generation: 1,
            loading: false,
            event_cancel: None,
            event_generation: 1,
            refresh_pending: false,
            manifests: vec![],
            event_panes: vec![],
            catalog: vec![],
            catalog_paths: Default::default(),
        };
        assert!(state.accepts_event(1, 1));
        state.event_generation += 1;
        assert!(!state.accepts_event(1, 1));
        assert!(state.accepts_event(1, 2));
        state.generation += 1;
        assert!(!state.accepts_event(1, 2));
    }
}
