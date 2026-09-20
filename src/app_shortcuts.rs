use super::*;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = workbench, no_json)]
pub(crate) struct QuickAgent {
    pub kind: String,
}
fn agent_chords() -> Result<Value> {
    let mut v = match settings::read_preference("app.keyboardShortcuts.agentKinds") {
        Ok(Some(value)) => value,
        Ok(None) => json!({}),
        Err(error) => {
            log::warn!("无法读取 Agent 快捷键，已仅在本次运行使用默认值：{error}");
            json!({})
        }
    };
    if !v.is_object() {
        log::warn!("Agent 快捷键配置不是对象，已仅在本次运行使用默认值");
        v = json!({});
    }
    v.as_object_mut()
        .expect("object checked above")
        .entry("pi")
        .or_insert_with(|| json!({"key":"p","modifierRaw":2}));
    Ok(v)
}
fn agent_default_chord(kind: &str) -> Option<&'static str> {
    (kind == "pi").then_some("alt-p")
}
pub(super) fn chord_from_keystroke(keystroke: &Keystroke) -> Option<String> {
    let modifiers = &keystroke.modifiers;
    if !modifiers.platform && !modifiers.alt && !modifiers.control && !modifiers.shift {
        return None;
    }
    let key = keystroke.key.as_str();
    if key.is_empty()
        || matches!(
            key,
            "shift"
                | "control"
                | "ctrl"
                | "alt"
                | "option"
                | "cmd"
                | "platform"
                | "fn"
                | "function"
        )
    {
        return None;
    }
    let mut parts = Vec::new();
    if modifiers.platform {
        parts.push("cmd");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if modifiers.control {
        parts.push("ctrl");
    }
    parts.push(key);
    Some(parts.join("-"))
}
fn resolved_agent_chord(kind: &str, stored: Option<&Value>) -> Option<String> {
    let fallback = agent_default_chord(kind).map(str::to_owned);
    stored
        .and_then(|chord| shortcut_text(&json!({"newItem": chord}), "newItem").ok())
        .and_then(|chord| {
            validate_application_chord(&format!("Agent {kind}"), &chord)
                .ok()
                .map(|_| chord)
        })
        .or(fallback)
}
fn push_agent_binding(
    bindings: &mut Vec<KeyBinding>,
    seen: &mut std::collections::HashSet<String>,
    kind: &str,
    chord: &str,
) -> Result<()> {
    let key = canonical(chord)?;
    if !seen.insert(key) {
        log::warn!("Agent {kind} 的快捷键冲突，已在本次运行忽略");
        return Ok(());
    }
    let action = QuickAgent {
        kind: kind.to_owned(),
    };
    bindings.push(KeyBinding::new(chord, action.clone(), Some("Workbench")));
    bindings.push(KeyBinding::new(chord, action, Some("ContextMenu")));
    Ok(())
}
pub(super) fn agent_shortcut_text(kind: &str) -> Result<String> {
    let v = agent_chords()?;
    match v.get(kind) {
        Some(v) => shortcut_text(&json!({"newItem":v}), "newItem"),
        None => Ok(String::new()),
    }
}
pub(super) const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("newItem", "新建", "cmd-t"),
    ("quickNewTerminal", "新建终端", "cmd-shift-t"),
    ("newSpace", "新建空间", "cmd-n"),
    ("search", "搜索", "cmd-k"),
    ("settings", "设置", "cmd-,"),
    ("toggleSidebar", "侧栏", "cmd-b"),
    ("close", "关闭", "cmd-w"),
    ("copySpacePath", "复制空间路径", "cmd-shift-c"),
    ("splitVertical", "左右分栏", "cmd-d"),
    ("splitHorizontal", "上下分栏", "cmd-shift-d"),
    ("focusLeft", "焦点左移", "cmd-alt-left"),
    ("focusRight", "焦点右移", "cmd-alt-right"),
    ("focusUp", "焦点上移", "cmd-alt-up"),
    ("focusDown", "焦点下移", "cmd-alt-down"),
    ("swapLeft", "交换左侧", "cmd-alt-shift-left"),
    ("swapRight", "交换右侧", "cmd-alt-shift-right"),
    ("swapUp", "交换上方", "cmd-alt-shift-up"),
    ("swapDown", "交换下方", "cmd-alt-shift-down"),
    ("widenPane", "加宽面板", "cmd-ctrl-right"),
    ("narrowPane", "收窄面板", "cmd-ctrl-left"),
    ("growPane", "增高面板", "cmd-ctrl-down"),
    ("shrinkPane", "降低面板", "cmd-ctrl-up"),
    ("equalizeSplits", "均分面板", "cmd-ctrl-="),
];
// ponytail: keymaps are platform-aware; packaging/backends remain macOS-only until ported.
pub(super) fn platform_default(chord: &str) -> String {
    default_for_platform(chord, cfg!(target_os = "macos"))
}
fn default_for_platform(chord: &str, mac: bool) -> String {
    if mac {
        return chord.into();
    }
    if chord == "cmd-," {
        return "ctrl-,".into();
    }
    // Terminal apps must leave plain Ctrl letters to the shell.
    for (from, to) in [
        ("cmd-alt-shift-", "ctrl-alt-shift-"),
        ("cmd-ctrl-", "ctrl-alt-"),
        ("cmd-alt-", "alt-shift-"),
        ("cmd-shift-", "ctrl-alt-shift-"),
        ("cmd-", "ctrl-shift-"),
    ] {
        if let Some(key) = chord.strip_prefix(from) {
            return format!("{to}{key}");
        }
    }
    chord.into()
}
fn key_name(key: &str) -> &str {
    match key {
        "\u{f700}" => "up",
        "\u{f701}" => "down",
        "\u{f702}" => "left",
        "\u{f703}" => "right",
        " " => "space",
        "\r" => "enter",
        "\u{1b}" => "escape",
        s => s,
    }
}
pub(super) fn shortcut_text(value: &Value, id: &str) -> Result<String> {
    let default = SHORTCUTS
        .iter()
        .find(|(key, _, _)| *key == id)
        .ok_or_else(|| anyhow::anyhow!("未知快捷动作：{id}"))?
        .2;
    let Some(v) = value.get(id) else {
        return Ok(platform_default(default));
    };
    if let Some(s) = v.as_str() {
        return Ok(s.trim().into());
    }
    let key = v["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("快捷键缺少 key：{id}"))?;
    let raw = v["modifierRaw"]
        .as_u64()
        .or_else(|| {
            v["modifierRaw"]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0)
                .map(|n| n as u64)
        })
        .ok_or_else(|| anyhow::anyhow!("快捷键缺少 modifierRaw：{id}"))?;
    if raw > 15 {
        bail!("无效快捷键修饰键：{id}")
    }
    let mut parts = Vec::new();
    for (bit, name) in [(1, "cmd"), (2, "alt"), (4, "shift"), (8, "ctrl")] {
        if raw & bit != 0 {
            parts.push(name)
        }
    }
    parts.push(key_name(key));
    Ok(parts.join("-"))
}
fn parsed(chord: &str) -> Result<Keystroke> {
    if chord.split_whitespace().count() != 1 {
        bail!("请输入单组快捷键，例如 cmd-shift-t")
    }
    Keystroke::parse(chord).map_err(|e| anyhow::anyhow!("无效快捷键：{e}"))
}
fn canonical(chord: &str) -> Result<String> {
    let k = parsed(chord)?;
    Ok(format!(
        "{}:{}:{}:{}:{}",
        k.modifiers.platform, k.modifiers.alt, k.modifiers.shift, k.modifiers.control, k.key
    ))
}
fn compatible(value: &Value) -> Result<Value> {
    let mut out = value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("快捷键必须是对象"))?;
    for (id, _, _) in SHORTCUTS {
        if !out.contains_key(*id) {
            continue;
        }
        let k = parsed(&shortcut_text(value, id)?)?;
        let raw = u8::from(k.modifiers.platform)
            | u8::from(k.modifiers.alt) * 2
            | u8::from(k.modifiers.shift) * 4
            | u8::from(k.modifiers.control) * 8;
        let key = match k.key.as_str() {
            "up" => "\u{f700}",
            "down" => "\u{f701}",
            "left" => "\u{f702}",
            "right" => "\u{f703}",
            "space" => " ",
            "enter" => "\r",
            "escape" => "\u{1b}",
            s => s,
        };
        out.insert((*id).into(), json!({"key":key,"modifierRaw":raw}));
    }
    Ok(Value::Object(out))
}

fn validate_application_chord(label: &str, chord: &str) -> Result<String> {
    let k = parsed(chord)?;
    // Ctrl-only chords remain terminal input; Ctrl-Shift is an explicit original-app shortcut.
    if !k.modifiers.platform
        && !k.modifiers.alt
        && !(k.modifiers.control && k.modifiers.shift)
        && !(cfg!(not(target_os = "macos")) && chord == "ctrl-,")
    {
        bail!("{label}：应用快捷键须含 cmd、alt 或 ctrl-shift，Ctrl 和普通文字保留给终端")
    }
    let normalized = canonical(chord)?;
    for reserved in reserved_chords() {
        if normalized == canonical(&reserved)? {
            bail!("{label} 与文字编辑、终端或系统快捷键 {reserved} 冲突")
        }
    }
    if label != "设置" && normalized == canonical(&platform_default("cmd-,"))? {
        bail!("{label} 与设置快捷键冲突")
    }
    Ok(normalized)
}

fn reserved_chords() -> Vec<String> {
    let mut keys = if cfg!(target_os = "macos") {
        vec![
            "cmd-c",
            "cmd-v",
            "cmd-x",
            "cmd-a",
            "cmd-z",
            "cmd-shift-z",
            "cmd-f",
            "cmd-q",
            "cmd-h",
            "cmd-alt-h",
            "cmd-m",
            "ctrl-cmd-f",
            "ctrl-cmd-space",
            "cmd-r",
            "cmd-left",
            "cmd-right",
            "cmd-shift-left",
            "cmd-shift-right",
            "alt-left",
            "alt-right",
            "alt-shift-left",
            "alt-shift-right",
            "alt-backspace",
            "alt-delete",
            "cmd-backspace",
            "cmd-delete",
        ]
    } else {
        vec![
            "ctrl-c",
            "ctrl-v",
            "ctrl-x",
            "ctrl-a",
            "ctrl-z",
            "ctrl-y",
            "ctrl-shift-z",
            "ctrl-shift-c",
            "ctrl-shift-v",
            "ctrl-shift-a",
            "ctrl-shift-f",
            "ctrl-shift-r",
            "ctrl-shift-q",
            "ctrl-left",
            "ctrl-right",
            "ctrl-shift-left",
            "ctrl-shift-right",
            "ctrl-backspace",
            "ctrl-delete",
            "alt-f4",
            "alt-tab",
            "alt-space",
            "f11",
            "f5",
        ]
    }
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    keys.extend(["ctrl-tab", "ctrl-shift-tab"].map(str::to_owned));
    for n in 0..=9 {
        if n != 0 {
            keys.push(platform_default(&format!("cmd-{n}")));
        }
        keys.push(format!(
            "{}-{n}",
            if cfg!(target_os = "macos") {
                "ctrl"
            } else {
                "ctrl-alt"
            }
        ));
    }
    keys
}
fn resolved_shortcuts(value: &Value) -> Value {
    let stored = match value.as_object() {
        Some(stored) => stored,
        None => {
            log::warn!("快捷键配置不是对象，已仅在本次运行使用默认值");
            return json!({});
        }
    };
    let mut resolved = SHORTCUTS
        .iter()
        .map(|(id, _, default)| ((*id).to_owned(), (platform_default(default), false)))
        .collect::<HashMap<_, _>>();
    for (id, label, _) in SHORTCUTS {
        if !stored.contains_key(*id) {
            continue;
        }
        let Ok(chord) = shortcut_text(value, id) else {
            log::warn!("快捷键 {label} 无法读取，已仅在本次运行使用默认值");
            continue;
        };
        if validate_application_chord(label, &chord).is_err() {
            log::warn!("快捷键 {label} 不安全或无效，已仅在本次运行使用默认值");
            continue;
        }
        resolved.insert((*id).to_owned(), (chord, true));
    }
    loop {
        let mut groups = HashMap::<String, Vec<String>>::new();
        for (id, (chord, _)) in &resolved {
            groups
                .entry(canonical(chord).expect("known shortcut"))
                .or_default()
                .push(id.clone());
        }
        let fallback = groups
            .values()
            .filter(|ids| ids.len() > 1)
            .flat_map(|ids| ids.iter())
            .filter(|id| resolved[*id].1)
            .cloned()
            .collect::<Vec<_>>();
        if fallback.is_empty() {
            break;
        }
        for id in fallback {
            let (_, _, default) = SHORTCUTS
                .iter()
                .find(|(key, _, _)| *key == id)
                .expect("known shortcut");
            log::warn!("快捷键 {id} 与其他动作冲突，已仅在本次运行使用默认值");
            resolved.insert(id, (platform_default(default), false));
        }
    }
    Value::Object(
        resolved
            .into_iter()
            .map(|(id, (chord, _))| (id, json!(chord)))
            .collect(),
    )
}
pub(super) fn default_bindings() -> Vec<KeyBinding> {
    configured_bindings(&json!({})).expect("valid default shortcuts")
}
fn configured_bindings(value: &Value) -> Result<Vec<KeyBinding>> {
    let mut bindings = vec![
        KeyBinding::new("enter", SubmitSpace, Some("SpaceModal")),
        KeyBinding::new("escape", Escape, Some("SpaceModal")),
        KeyBinding::new(&platform_default("cmd-o"), OpenFolder, Some("SpaceModal")),
        KeyBinding::new("space", SubmitSpace, Some("SpaceModalButton")),
    ];
    macro_rules! bind {
        ($id:literal,$action:expr) => {
            bindings.push(KeyBinding::new(
                &shortcut_text(value, $id)?,
                $action,
                Some("Workbench"),
            ));
        };
    }
    bind!("newItem", NewItem);
    bind!("quickNewTerminal", NewTerminal);
    bind!("newSpace", NewSpace);
    bind!("search", Search);
    bind!("settings", OpenSettings);
    bind!("toggleSidebar", ToggleSidebar);
    bind!("close", Close);
    bind!("splitVertical", SplitRight);
    bind!("splitHorizontal", SplitDown);
    bind!("focusLeft", FocusLeft);
    bind!("focusRight", FocusRight);
    bind!("focusUp", FocusUp);
    bind!("focusDown", FocusDown);
    bind!("swapLeft", SwapLeft);
    bind!("swapRight", SwapRight);
    bind!("swapUp", SwapUp);
    bind!("swapDown", SwapDown);
    bind!("widenPane", Widen);
    bind!("narrowPane", Narrow);
    bind!("growPane", Grow);
    bind!("shrinkPane", Shrink);
    bind!("equalizeSplits", Equalize);
    bind!("copySpacePath", CopySpacePath);
    bindings.push(KeyBinding::new("escape", Escape, Some("Workbench")));
    for key in ["escape", "tab", "shift-tab"] {
        bindings.push(KeyBinding::new(key, Escape, Some("ContextMenu")));
    }
    bindings.push(KeyBinding::new(
        &shortcut_text(value, "copySpacePath")?,
        CopySpacePath,
        Some("ContextMenu"),
    ));
    bindings.push(KeyBinding::new(
        &platform_default("cmd-r"),
        Refresh,
        Some("Workbench"),
    ));
    bindings.push(KeyBinding::new(
        &platform_default("cmd-f"),
        FindTerminal,
        Some("Workbench"),
    ));
    for n in 1u8..=9 {
        bindings.push(KeyBinding::new(
            &platform_default(&format!("cmd-{n}")),
            super::GoSession { n },
            Some("Workbench"),
        ));
    }
    for n in 0u8..=9 {
        bindings.push(KeyBinding::new(
            &format!(
                "{}-{n}",
                if cfg!(target_os = "macos") {
                    "ctrl"
                } else {
                    "ctrl-alt"
                }
            ),
            super::GoSpace { n },
            Some("Workbench"),
        ));
    }
    bindings.extend([
        KeyBinding::new("enter", RenameTitle, Some("SessionTitle")),
        KeyBinding::new("space", RenameTitle, Some("SessionTitle")),
        KeyBinding::new("f2", RenameTitle, Some("SessionTitle")),
        KeyBinding::new(&platform_default("cmd-,"), OpenSettings, Some("Workbench")),
        KeyBinding::new("tab", FocusNext, Some("Workbench && !Terminal")),
        KeyBinding::new("shift-tab", FocusPrevious, Some("Workbench && !Terminal")),
        // `gpui_component::Root` binds tab/shift-tab to its own focus cycling in the
        // ancestor "Root" context, which would swallow them before the PTY sees them.
        // Unbind both while the terminal forwards them (Tab = \t, Shift-Tab = ESC[Z,
        // which Pi uses to switch agents).
        KeyBinding::new("tab", NoAction {}, Some("Terminal && PtyKeys")),
        KeyBinding::new("shift-tab", NoAction {}, Some("Terminal && PtyKeys")),
        KeyBinding::new("ctrl-tab", NextSession, Some("Workbench")),
        KeyBinding::new("ctrl-shift-tab", PreviousSession, Some("Workbench")),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "ctrl-cmd-f"
            } else {
                "f11"
            },
            Fullscreen,
            Some("Workbench"),
        ),
    ]);
    if cfg!(target_os = "macos") {
        bindings.push(KeyBinding::new("cmd-m", Minimize, Some("Workbench")));
    } else {
        bindings.push(KeyBinding::new(
            "ctrl-f",
            FindTerminal,
            Some("Workbench && !Terminal"),
        ));
        bindings.push(KeyBinding::new("f5", Refresh, Some("Workbench")));
        bindings.extend([
            KeyBinding::new("ctrl-t", NewItem, Some("Workbench && !Terminal")),
            KeyBinding::new("ctrl-n", NewSpace, Some("Workbench && !Terminal")),
            KeyBinding::new("ctrl-w", Close, Some("Workbench && !Terminal")),
            KeyBinding::new("ctrl-r", Refresh, Some("Workbench && !Terminal")),
        ]);
    }
    Ok(bindings)
}
// Query the installed keymap for these actions; never display unvalidated preference strings.
pub(super) fn shortcut_action(action: &UiAction) -> Option<Box<dyn gpui::Action>> {
    Some(match action {
        UiAction::Search => Box::new(Search),
        UiAction::NewItem => Box::new(NewItem),
        UiAction::NewTerminal => Box::new(NewTerminal),
        UiAction::NewSpace => Box::new(NewSpace),
        UiAction::Settings => Box::new(OpenSettings),
        UiAction::ToggleSidebar => Box::new(ToggleSidebar),
        UiAction::Refresh => Box::new(Refresh),
        UiAction::TerminalSearch => Box::new(FindTerminal),
        UiAction::Split(Axis::Vertical) => Box::new(SplitRight),
        UiAction::Split(Axis::Horizontal) => Box::new(SplitDown),
        UiAction::Equalize => Box::new(Equalize),
        UiAction::QuickAgent(kind) => Box::new(QuickAgent { kind: kind.clone() }),
        _ => return None,
    })
}

impl AppView {
    pub(super) fn validate_shortcuts(value: &Value) -> Result<()> {
        if !value.is_object() {
            bail!("快捷键必须是对象")
        }
        let mut seen = HashMap::new();
        for (id, label, _) in SHORTCUTS {
            let chord = shortcut_text(value, id)?;
            let normalized = validate_application_chord(label, &chord)?;
            if let Some(other) = seen.insert(normalized, *label) {
                bail!("{label} 与 {other} 快捷键冲突")
            }
        }
        Ok(())
    }
    pub(super) fn apply_shortcuts(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let compatible = compatible(&resolved_shortcuts(&self.settings.shortcuts))?;
        let mut bindings = configured_bindings(&compatible)?;
        let stored = agent_chords()?;
        let mut catalog = super::installed_agent_kinds();
        if let Some(local) = self.states.get(&Device::local().id) {
            if !local.catalog.is_empty() {
                catalog = local.catalog.clone();
            }
        }
        let mut seen = std::collections::HashSet::new();
        for (id, _, _) in SHORTCUTS {
            seen.insert(canonical(&shortcut_text(&compatible, id)?)?);
        }
        for kind in super::enabled_agent_kinds(&self.settings, &catalog) {
            let Some(chord) = resolved_agent_chord(&kind, stored.get(&kind)) else {
                continue;
            };
            if validate_application_chord(&format!("Agent {kind}"), &chord).is_err() {
                log::warn!("Agent {kind} 的快捷键无效，已在本次运行忽略");
                continue;
            }
            push_agent_binding(&mut bindings, &mut seen, &kind, &chord)?;
        }
        // Preserve component input/modal actions while replacing the application keymap.
        let component_bindings = cx
            .key_bindings()
            .borrow()
            .bindings()
            .filter(|binding| {
                ["input::", "root::", "ui::"]
                    .iter()
                    .any(|prefix| binding.action().name().starts_with(prefix))
            })
            .cloned()
            .collect::<Vec<_>>();
        cx.clear_key_bindings();
        cx.bind_keys(component_bindings);
        crate::input::init(cx);
        crate::bind_system_shortcuts(cx);
        cx.bind_keys(bindings);
        crate::set_menus(cx);
        Ok(())
    }
    pub(super) fn set_agent_shortcut(
        &mut self,
        kind: &str,
        chord: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if kind.is_empty() || kind.contains('\0') {
            bail!("Agent 类型无效")
        }
        let old = settings::read_preference("app.keyboardShortcuts.agentKinds")?;
        let mut next = old.clone().unwrap_or_else(|| json!({}));
        let map = next
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Agent 快捷键数据损坏"))?;
        if chord.trim().is_empty() {
            map.remove(kind);
        } else {
            validate_application_chord(&format!("Agent {kind}"), chord)?;
            let general = compatible(&resolved_shortcuts(&self.settings.shortcuts))?;
            let key = canonical(chord)?;
            if SHORTCUTS.iter().any(|(id, _, _)| {
                shortcut_text(&general, id)
                    .ok()
                    .and_then(|existing| canonical(&existing).ok())
                    .as_deref()
                    == Some(key.as_str())
            }) {
                bail!("Agent {kind} 的快捷键与其他动作冲突");
            }
            for (other, existing) in map.iter() {
                if other != kind
                    && shortcut_text(&json!({"newItem": existing}), "newItem")
                        .ok()
                        .and_then(|existing| canonical(&existing).ok())
                        .as_deref()
                        == Some(key.as_str())
                {
                    bail!("Agent {kind} 的快捷键与其他动作冲突");
                }
            }
            let v = compatible(&json!({"newItem":chord}))?;
            map.insert(kind.into(), v["newItem"].clone());
        }
        settings::write_preference("app.keyboardShortcuts.agentKinds", Some(&next))?;
        if let Err(e) = self.apply_shortcuts(cx) {
            settings::write_preference("app.keyboardShortcuts.agentKinds", old.as_ref())?;
            return Err(e);
        }
        Ok(())
    }
    #[allow(dead_code)]
    pub(super) fn restore_shortcut(&mut self, id: &str, cx: &mut Context<Self>) -> Result<()> {
        let old = self.settings.shortcuts.clone();
        if let Some(map) = self.settings.shortcuts.as_object_mut() {
            map.remove(id);
        }
        if let Err(e) = self.apply_shortcuts(cx) {
            self.settings.shortcuts = old;
            return Err(e);
        }
        self.settings.save()
    }
    pub(super) fn is_advanced_shortcut(id: &str) -> bool {
        matches!(
            id,
            "focusLeft"
                | "focusRight"
                | "focusUp"
                | "focusDown"
                | "swapLeft"
                | "swapRight"
                | "swapUp"
                | "swapDown"
                | "widenPane"
                | "narrowPane"
                | "growPane"
                | "shrinkPane"
                | "equalizeSplits"
        )
    }
    pub(super) fn restore_shortcuts(&mut self, cx: &mut Context<Self>) -> Result<()> {
        let old = self.settings.shortcuts.clone();
        let old_agents = settings::read_preference("app.keyboardShortcuts.agentKinds")?;
        self.settings.shortcuts = json!({});
        if let Err(e) = settings::write_preference("app.keyboardShortcuts.agentKinds", None) {
            self.settings.shortcuts = old;
            return Err(e);
        }
        if let Err(e) = self.apply_shortcuts(cx) {
            self.settings.shortcuts = old;
            let _ =
                settings::write_preference("app.keyboardShortcuts.agentKinds", old_agents.as_ref());
            return Err(e);
        }
        if let Err(e) = self.settings.save() {
            self.settings.shortcuts = old;
            let _ =
                settings::write_preference("app.keyboardShortcuts.agentKinds", old_agents.as_ref());
            let _ = self.apply_shortcuts(cx);
            return Err(e);
        }
        Ok(())
    }

    pub(super) fn capture_shortcut_if_focused(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.screen != Screen::Settings || self.settings_page != SettingsPage::Shortcuts {
            return;
        }
        let Some(key) = self.fields.iter().find_map(|(key, input)| {
            let input = input.read(cx);
            if input.is_shortcut_capture() && input.focus_handle().is_focused(window) {
                Some(key.clone())
            } else {
                None
            }
        }) else {
            return;
        };
        if keystroke.key == "escape" {
            self.focus.focus(window);
            cx.stop_propagation();
            return;
        }
        if keystroke.key == "enter" {
            if let Some(id) = key.strip_prefix("shortcut:") {
                self.save_shortcut_field(id, cx);
            } else if let Some(kind) = key.strip_prefix("agent-shortcut:") {
                self.save_agent_shortcut_field(kind, false, cx);
            }
            cx.stop_propagation();
            return;
        }
        let Some(chord) = chord_from_keystroke(keystroke) else {
            return;
        };
        if let Some(input) = self.fields.get(&key).cloned() {
            input.update(cx, |input, cx| input.set_text(&chord, cx));
        }
        if let Some(id) = key.strip_prefix("shortcut:") {
            self.save_shortcut_field(id, cx);
        } else if let Some(kind) = key.strip_prefix("agent-shortcut:") {
            self.save_agent_shortcut_field(kind, false, cx);
        }
        cx.stop_propagation();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn tooltip_actions_only_map_equivalent_commands() {
        use super::*;
        assert!(
            shortcut_action(&UiAction::Search)
                .unwrap()
                .as_any()
                .is::<Search>()
        );
        assert!(
            shortcut_action(&UiAction::Settings)
                .unwrap()
                .as_any()
                .is::<OpenSettings>()
        );
        assert!(shortcut_action(&UiAction::DeviceMenu).is_none());
        assert!(shortcut_action(&UiAction::Usage).is_none());
        assert!(shortcut_action(&UiAction::CloseRemote("other-pane".into())).is_none());
        let action = shortcut_action(&UiAction::QuickAgent("pi".into())).unwrap();
        assert_eq!(
            action.as_any().downcast_ref::<QuickAgent>().unwrap().kind,
            "pi"
        );
    }

    #[test]
    fn agent_defaults_and_keystroke_capture_format() {
        assert_eq!(super::agent_default_chord("pi"), Some("alt-p"));
        assert_eq!(super::agent_default_chord("cursor"), None);
        assert_eq!(
            super::resolved_agent_chord("pi", None).as_deref(),
            Some("alt-p")
        );
        assert!(super::resolved_agent_chord("cursor", None).is_none());
        assert_eq!(
            super::chord_from_keystroke(&super::parsed("alt-p").unwrap()).as_deref(),
            Some("alt-p")
        );
        assert_eq!(
            super::chord_from_keystroke(&super::parsed("cmd-shift-t").unwrap()).as_deref(),
            Some("cmd-shift-t")
        );
        assert!(super::chord_from_keystroke(&super::parsed("p").unwrap()).is_none());
    }

    #[test]
    fn agent_shortcuts_bind_workbench_and_context_menu() {
        let mut bindings = Vec::new();
        let mut seen = std::collections::HashSet::new();
        super::push_agent_binding(&mut bindings, &mut seen, "pi", "alt-p").unwrap();
        assert_eq!(bindings.len(), 2);
        let workbench = [gpui::KeyContext::parse("Workbench").unwrap()];
        let menu = [gpui::KeyContext::parse("ContextMenu").unwrap()];
        let action = super::QuickAgent { kind: "pi".into() };
        for context in [&workbench[..], &menu[..]] {
            let binding = bindings
                .iter()
                .find(|binding| {
                    binding.action().partial_eq(&action)
                        && binding
                            .predicate()
                            .is_some_and(|predicate| predicate.eval_inner(context, context))
                })
                .expect("agent shortcut is bound");
            assert_eq!(binding.keystrokes()[0].key(), "p");
        }
    }

    use super::{AppView, compatible, resolved_shortcuts, shortcut_text};
    use serde_json::json;
    #[test]
    fn defaults_are_unique_on_all_platforms_and_leave_terminal_keys_alone() {
        for mac in [true, false] {
            let mut seen = std::collections::HashSet::new();
            for (_, _, default) in super::SHORTCUTS {
                let chord = super::default_for_platform(default, mac);
                let key = super::parsed(&chord).unwrap();
                assert!(
                    seen.insert(super::canonical(&chord).unwrap()),
                    "duplicate {chord}"
                );
                if !mac {
                    assert!(
                        !key.modifiers.platform,
                        "Windows/Linux cannot depend on Command"
                    );
                    assert!(key.modifiers.alt || key.modifiers.shift || key.key == ",");
                }
            }
        }
        assert_eq!(super::default_for_platform("cmd-,", false), "ctrl-,");
        assert!(AppView::validate_shortcuts(&json!({"search":"cmd-z"})).is_err());
        assert!(AppView::validate_shortcuts(&json!({"search":"alt-left"})).is_err());
        assert!(AppView::validate_shortcuts(&json!({"search":"cmd-,"})).is_err());
        assert!(!super::default_bindings().is_empty());
        let predicate = gpui::KeyBindingContextPredicate::parse("Workbench && !Terminal").unwrap();
        let root = [gpui::KeyContext::parse("Workbench").unwrap()];
        let terminal = [
            root[0].clone(),
            gpui::KeyContext::parse("Terminal").unwrap(),
        ];
        assert!(predicate.eval_inner(&root, &root));
        assert!(!predicate.eval_inner(&root, &terminal));
    }
    #[test]
    fn terminal_tab_shadows_ancestor_focus_bindings() {
        // `gpui_component::Root` holds tab/shift-tab in the ancestor "Root" context, so
        // the terminal must unbind them while it forwards them to the PTY, and hand them
        // back once it cannot (exited process or pending IME composition).
        let mut keymap = gpui::Keymap::default();
        keymap.add_bindings([
            gpui::KeyBinding::new("tab", super::Escape, Some("Root")),
            gpui::KeyBinding::new("shift-tab", super::Escape, Some("Root")),
        ]);
        keymap.add_bindings(super::configured_bindings(&json!({})).unwrap());
        let root = gpui::KeyContext::parse("Root").unwrap();
        let workbench = gpui::KeyContext::parse("Workbench").unwrap();
        let accepting = [
            root.clone(),
            workbench.clone(),
            gpui::KeyContext::parse(crate::terminal::PTY_KEYS_CONTEXT).unwrap(),
        ];
        let ended = [
            root,
            workbench,
            gpui::KeyContext::parse("Terminal").unwrap(),
        ];
        // Copy/paste bindings in `input` still match the extra flag: the terminal
        // identifier must survive alongside it.
        let copy = gpui::KeyBindingContextPredicate::parse("Terminal").unwrap();
        assert!(copy.eval_inner(&accepting, &accepting));
        for chord in ["tab", "shift-tab"] {
            let keystroke = super::parsed(chord).unwrap();
            assert!(
                keymap
                    .bindings_for_input(&[keystroke.clone()], &accepting)
                    .0
                    .is_empty(),
                "{chord} must reach the PTY"
            );
            let fallback = keymap.bindings_for_input(&[keystroke], &ended).0;
            assert_eq!(
                fallback.len(),
                1,
                "{chord} must fall back to the ancestor binding"
            );
            assert_eq!(fallback[0].action().name(), "workbench::Escape");
        }
    }
    #[test]
    fn space_modal_shortcuts_survive_reconfiguration_and_stay_scoped() {
        let bindings = super::configured_bindings(&json!({"search":"cmd-shift-k"})).unwrap();
        let modal = [gpui::KeyContext::parse("SpaceModal").unwrap()];
        let workbench = [gpui::KeyContext::parse("Workbench").unwrap()];
        for action in [
            &super::SubmitSpace as &dyn gpui::Action,
            &super::Escape,
            &super::OpenFolder,
        ] {
            let binding = bindings
                .iter()
                .find(|binding| {
                    binding.action().name() == action.name()
                        && binding.predicate().unwrap().eval_inner(&modal, &modal)
                })
                .expect("modal binding survives shortcut reload");
            assert!(
                !binding
                    .predicate()
                    .unwrap()
                    .eval_inner(&workbench, &workbench)
            );
        }
    }

    #[test]
    fn modal_button_space_key_does_not_capture_text_input() {
        let bindings = super::default_bindings();
        let modal = [gpui::KeyContext::parse("SpaceModal").unwrap()];
        let button = [
            modal[0].clone(),
            gpui::KeyContext::parse("SpaceModalButton").unwrap(),
        ];
        let space = bindings
            .iter()
            .find(|binding| {
                binding.keystrokes()[0].key() == "space"
                    && binding.predicate().unwrap().eval_inner(&button, &button)
            })
            .expect("focused buttons support Space");
        assert!(!space.predicate().unwrap().eval_inner(&modal, &modal));
    }

    #[test]
    fn swift_and_conflicts() {
        assert_eq!(
            shortcut_text(
                &json!({"focusLeft":{"key":"\u{f702}","modifierRaw":3}}),
                "focusLeft"
            )
            .unwrap(),
            "cmd-alt-left"
        );
        assert!(AppView::validate_shortcuts(&json!({})).is_ok());
        assert!(AppView::validate_shortcuts(&json!({"search":"cmd-t"})).is_err());
        assert!(AppView::validate_shortcuts(&json!({"search":"ctrl-c"})).is_err());
        assert!(AppView::validate_shortcuts(&json!({"search":"ctrl-shift-t"})).is_ok());
        assert_eq!(
            compatible(&json!({"search":"cmd-shift-k"})).unwrap()["search"]["modifierRaw"],
            5
        );
        assert_eq!(
            shortcut_text(
                &resolved_shortcuts(&json!({
                    "newItem":{"key":"t","modifierRaw":1.0},
                    "search":"ctrl-c"
                })),
                "newItem"
            )
            .unwrap(),
            "cmd-t"
        );
        assert_eq!(
            shortcut_text(&resolved_shortcuts(&json!({"search":"ctrl-c"})), "search").unwrap(),
            "cmd-k"
        );
        let swapped = resolved_shortcuts(&json!({"newItem":"cmd-k","search":"cmd-t"}));
        assert_eq!(shortcut_text(&swapped, "newItem").unwrap(), "cmd-k");
        assert_eq!(shortcut_text(&swapped, "search").unwrap(), "cmd-t");
        let fallback = resolved_shortcuts(&json!({"newItem":"ctrl-c","search":"cmd-t"}));
        assert_eq!(shortcut_text(&fallback, "newItem").unwrap(), "cmd-t");
        assert_eq!(shortcut_text(&fallback, "search").unwrap(), "cmd-k");
        let mut unique = std::collections::HashSet::new();
        for (id, _, _) in super::SHORTCUTS {
            assert!(
                unique.insert(super::canonical(&shortcut_text(&fallback, id).unwrap()).unwrap())
            );
        }
    }
}
