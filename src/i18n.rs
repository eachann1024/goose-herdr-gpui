//! Small UI-only dictionary. User names, paths and external errors are never token-translated.
use std::sync::{
    OnceLock,
    atomic::{AtomicU8, Ordering},
};

static LANGUAGE: AtomicU8 = AtomicU8::new(0);
static SYSTEM_LOCALE: OnceLock<String> = OnceLock::new();

fn system_locale() -> &'static str {
    SYSTEM_LOCALE.get_or_init(|| {
        std::process::Command::new("/usr/bin/defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .map(|s| s.trim().trim_matches(|c| c == '"' || c == ',' || c == ' '))
                    .find(|s| !s.is_empty() && *s != "(" && *s != ")")
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| std::env::var("LANG").unwrap_or_else(|_| "en".into()))
    })
}

pub fn resolve_language(setting: &str, locale: &str) -> &'static str {
    let choice = if setting.is_empty() || setting == "system" {
        locale
    } else {
        setting
    };
    if choice.to_ascii_lowercase().starts_with("zh") {
        "zh-Hans"
    } else {
        "en"
    }
}

pub fn set_language(setting: &str) {
    LANGUAGE.store(
        if resolve_language(setting, system_locale()) == "zh-Hans" {
            1
        } else {
            2
        },
        Ordering::Relaxed,
    );
}

pub fn language() -> &'static str {
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => "zh-Hans",
        2 => "en",
        _ => resolve_language("", system_locale()),
    }
}

pub fn tr(source: &str) -> String {
    if language() == "zh-Hans" {
        return source.to_owned();
    }
    if let Some((_, translated)) = DICTIONARY.iter().find(|(key, _)| *key == source) {
        return (*translated).to_owned();
    }
    for (pattern, translated) in TEMPLATES {
        if let Some(values) = capture(pattern, source) {
            return substitute(translated, &values);
        }
    }
    source.to_owned()
}

pub fn format(template: &str, args: &[&str]) -> String {
    substitute(&tr(template), args)
}

fn substitute(template: &str, args: &[&str]) -> String {
    let mut result = String::new();
    let mut parts = template.split("{}");
    result.push_str(parts.next().unwrap_or_default());
    for (index, part) in parts.enumerate() {
        result.push_str(args.get(index).copied().unwrap_or("{}"));
        result.push_str(part);
    }
    result
}

// Anchored, complete templates only: captures are copied unchanged, even if they contain UI words.
fn capture<'a>(template: &str, source: &'a str) -> Option<Vec<&'a str>> {
    if !template.contains("{}") {
        return None;
    }
    let parts = template.split("{}").collect::<Vec<_>>();
    if parts[0].is_empty() && parts.last().is_some_and(|p| p.is_empty()) {
        return None;
    }
    let mut rest = source.strip_prefix(parts[0])?;
    let mut values = Vec::new();
    for (index, separator) in parts.iter().enumerate().skip(1) {
        if index == parts.len() - 1 {
            values.push(rest.strip_suffix(separator)?);
            return Some(values);
        }
        if separator.is_empty() {
            return None;
        }
        let position = rest.find(separator)?;
        values.push(&rest[..position]);
        rest = &rest[position + separator.len()..];
    }
    None
}

const DICTIONARY: &[(&str, &str)] = &[
    (
        "Ctrl-C 会中断当前进程。",
        "Ctrl-C interrupts the current process.",
    ),
    ("搜索设置…", "Search settings…"),
    ("通用", "General"),
    ("没有匹配的设置", "No matching settings"),
    ("已开启", "On"),
    ("未检测到可执行文件", "No executable detected"),
    ("检查命令", "Check command"),
    ("启用", "Enable"),
    ("操作未完成", "Operation not completed"),
    ("关闭通知", "Dismiss notification"),
    (
        "会话已被其他客户端连接",
        "Session is connected by another client",
    ),
    ("终端连接已结束", "Terminal connection ended"),
    (
        "此会话已由另一客户端连接；如需切换控制端，请点击“接管会话”。",
        "This session is connected by another client. To switch control, click Take Over Session.",
    ),
    (
        "命令名或绝对路径（留空自动检测）",
        "Command name or absolute path (leave empty to detect automatically)",
    ),
    (
        "可执行文件不存在或没有执行权限",
        "Executable does not exist or is not executable",
    ),
    (
        "请输入命令名或可执行文件的绝对路径，不要包含参数",
        "Enter a command name or an executable absolute path; do not include arguments",
    ),
    ("关闭会话", "Close Session"),
    ("删除设备", "Remove Device"),
    ("接管会话", "Take Over Session"),
    (
        "接管会话将断开原客户端的控制连接。",
        "Taking over disconnects the previous client's control connection.",
    ),
    ("保留空间不存在", "Retained space does not exist"),
    (
        "保留空间数据损坏，未覆盖原数据",
        "Retained space data is damaged; original data preserved",
    ),
    (
        "保留空间缺少标识",
        "Retained space is missing its identifier",
    ),
    (
        "创建已提交，但响应缺少空间标识；请刷新，勿重复创建",
        "Creation submitted, but the response has no space identifier. Refresh; do not create again.",
    ),
    ("空间保存锁不可用", "Space persistence lock is unavailable"),
    ("空间状态锁不可用", "Space state lock is unavailable"),
    ("搜索", "Search"),
    ("设置", "Settings"),
    ("设置…", "Settings…"),
    ("新建", "New"),
    ("新增", "New"),
    (
        "没有可用空间，请先新建。",
        "No spaces yet. Create one first.",
    ),
    ("新建…", "New…"),
    ("＋ 新建…", "＋ New…"),
    ("新建空间", "New Space"),
    ("创建空间", "Create Space"),
    ("打开文件夹…", "Open Folder…"),
    ("搜索项目…", "Search projects…"),
    ("没有匹配的项目", "No matching projects"),
    ("空间表单已关闭", "The space form is closed"),
    ("松开以添加为空间", "Release to add as a space"),
    ("新建终端", "New Terminal"),
    ("新终端", "New Terminal"),
    ("新建 Agent / 终端", "New Agent / Terminal"),
    ("工作台", "Workbench"),
    ("← 工作台", "← Workbench"),
    ("文件", "Files"),
    ("编辑", "Edit"),
    ("视图", "View"),
    ("服务", "Services"),
    ("退出 Goose herdr GPUI", "Quit Goose herdr GPUI"),
    ("撤销", "Undo"),
    ("重做", "Redo"),
    ("剪切", "Cut"),
    ("复制", "Copy"),
    ("粘贴", "Paste"),
    ("全选", "Select All"),
    ("关闭", "Close"),
    ("取消", "Cancel"),
    ("刷新", "Refresh"),
    ("保存", "Save"),
    ("保存 / 执行", "Save / Run"),
    ("还原", "Reset"),
    ("设备", "Device"),
    ("空间", "Spaces"),
    ("终端", "Terminal"),
    ("侧栏", "Sidebar"),
    ("详情", "Details"),
    ("收起", "Collapse"),
    ("展开", "Expand"),
    ("用量", "Usage"),
    ("目录", "Directory"),
    ("路径", "Path"),
    ("名称", "Name"),
    ("工作目录", "Working directory"),
    ("显示名称", "Display name"),
    ("标签", "Label"),
    ("选择", "Choose"),
    ("选择…", "Choose…"),
    ("添加设备", "Add Device"),
    ("添加设备...", "Add Device..."),
    ("这台 Mac · {}", "This Mac · {}"),
    ("{} · SSH", "{} · SSH"),
    ("tailcat 隧道", "Tailcat tunnel"),
    ("编辑设备", "Edit Device"),
    ("移除设备", "Remove Device"),
    ("连接中…", "Connecting…"),
    ("未连接", "Not connected"),
    ("连接失败", "Connection failed"),
    ("重新连接", "Reconnect"),
    ("重连", "Reconnect"),
    ("控制连接中断", "Control connection interrupted"),
    ("运行中", "Running"),
    ("完成", "Done"),
    ("已完成", "finished"),
    ("需输入", "Needs input"),
    ("需要你的输入", "needs your input"),
    ("优先处理", "High priority"),
    ("优先会话", "Priority sessions"),
    ("暂无待处理", "No sessions need attention"),
    ("新建会话", "New Session"),
    ("正在查看", "Viewing"),
    ("其他会话", "Other sessions"),
    ("重命名", "Rename"),
    ("重命名空间", "Rename Space"),
    ("重命名会话", "Rename Session"),
    ("重命名 Agent 身份", "Rename Agent Identity"),
    ("重命名 Agent 身份…", "Rename Agent Identity…"),
    ("结束服务端会话", "End Server Session"),
    ("结束服务端会话…", "End Server Session…"),
    ("结束空间…", "End Space…"),
    ("确认结束", "Confirm End"),
    ("关闭视图", "Close View"),
    ("显示终端", "Show Terminal"),
    ("左右分栏", "Split Side by Side"),
    ("上下分栏", "Split Stacked"),
    ("均分", "Equalize"),
    ("均分窗格", "Equalize Panes"),
    ("均分面板", "Equalize Panes"),
    ("加宽面板", "Widen Pane"),
    ("收窄面板", "Narrow Pane"),
    ("增高面板", "Increase Pane Height"),
    ("降低面板", "Decrease Pane Height"),
    ("焦点上移", "Focus Up"),
    ("焦点下移", "Focus Down"),
    ("焦点左移", "Focus Left"),
    ("焦点右移", "Focus Right"),
    ("交换上方", "Swap Up"),
    ("交换下方", "Swap Down"),
    ("交换左侧", "Swap Left"),
    ("交换右侧", "Swap Right"),
    ("向前移动", "Move Earlier"),
    ("向后移动", "Move Later"),
    ("向前移动空间", "Move Space Earlier"),
    ("向后移动空间", "Move Space Later"),
    ("当前对象", "Current Selection"),
    ("复制连接命令", "Copy Connection Command"),
    ("复制空间路径", "Copy Space Path"),
    ("复制 Herdr 空间", "Copy Herdr Space"),
    ("复制 Herdr 位置", "Copy Herdr location"),
    ("发送附件", "Send Attachment"),
    ("发送附件…", "Send Attachment…"),
    ("搜索终端", "Search Terminal"),
    (
        "搜索                       ⌘K",
        "Search                       ⌘K",
    ),
    ("检查会话 / 发送输入", "Inspect Session / Send Input"),
    ("打开独立终端", "Open Standalone Terminal"),
    ("独立终端", "Standalone Terminal"),
    (
        "选择一个会话，或创建新的工作空间。",
        "Select a session, or create a workspace.",
    ),
    (
        "尚无空间，使用上方新建空间。",
        "No spaces yet. Create one above.",
    ),
    ("尚未获取设备快照", "No device snapshot available"),
    (
        "客户端已退出；服务端会话状态请刷新确认。",
        "The client exited. Refresh to check the server session.",
    ),
    (
        "仅从客户端移除此设备，不结束远端会话、不删除文件。已有终端视图保留至手动关闭；Keychain凭据不会自动删除。",
        "Remove this device from the client only. Remote sessions and files remain. Existing terminal views stay open; Keychain credentials are not deleted.",
    ),
    (
        "这会结束服务端对应会话及其进程，不只是关闭本地视图。此操作不可撤销。",
        "This ends the server session and its processes, not just the local view. This cannot be undone.",
    ),
    ("启动本机 Herdr 服务", "Start Local Herdr Service"),
    ("启动本机服务…", "Start Local Service…"),
    ("Herdr CLI 绝对路径", "Absolute Herdr CLI path"),
    (
        "Pi 仍未就绪，可显示终端查看真实输出",
        "Pi is not ready yet. Show the terminal to inspect its output.",
    ),
    (
        "Pi 启动尚未就绪，可检查状态或显示终端",
        "Pi is not ready yet. Check its status or show the terminal.",
    ),
    ("检查 Pi 状态", "Check Pi Status"),
    (
        "Pi 就绪检测超时，请在终端查看启动状态",
        "Pi readiness check timed out. Check its startup status in the terminal.",
    ),
    ("正在检查 Pi 启动状态…", "Checking Pi startup status…"),
    ("正在启动本机服务…", "Starting local service…"),
    ("正在读取本地配置…", "Loading local settings…"),
    ("正在创建…", "Creating…"),
    ("正在执行…", "Running…"),
    ("正在准备附件…", "Preparing attachment…"),
    ("Socket 路径（可留空）", "Socket path (optional)"),
    ("密码", "Password"),
    (
        "密码 / Tailcat token（留空不更改）",
        "Password / Tailcat token (leave blank to keep)",
    ),
    ("Agent 名称", "Agent name"),
    (
        "Agent 类型（terminal 为终端）",
        "Agent kind (terminal for a terminal)",
    ),
    ("参数 JSON 数组", "Arguments (JSON array)"),
    ("Agent 可执行文件", "Agent Executable"),
    (
        "绝对路径（留空自动检测）",
        "Absolute path (leave blank to detect)",
    ),
    ("上级", "Parent"),
    ("主页", "Home"),
    ("前往", "Go"),
    ("上传…", "Upload…"),
    ("下载…", "Download…"),
    ("文件传输", "File Transfer"),
    ("文件传输中…", "Transferring files…"),
    ("取消传输", "Cancel Transfer"),
    ("目录路径", "Directory path"),
    ("本地文件", "Local file"),
    ("本地文件路径", "Local file path"),
    ("本地文件 / 下载目录", "Local file / download directory"),
    ("远端文件 / 上传目录", "Remote file / upload directory"),
    (
        "冲突策略：fail / replace / keep-both",
        "Conflict policy: fail / replace / keep-both",
    ),
    ("隐藏项：显示", "Hidden: Shown"),
    ("隐藏项：隐藏", "Hidden: Hidden"),
    ("此目录为空", "This directory is empty"),
    ("尚未读取目录", "Directory not loaded"),
    ("正在读取目录…", "Reading directory…"),
    (
        "上传已完成，但目标终端已关闭；未向其他终端发送",
        "Upload completed, but the target terminal closed. Nothing was sent to another terminal.",
    ),
    (
        "搜索名称、设备、目录…",
        "Search names, devices, directories…",
    ),
    ("搜索当前终端", "Search Current Terminal"),
    (
        "搜索设备、会话与空间",
        "Search Devices, Sessions and Spaces",
    ),
    (
        "正则表达式（区分大小写）",
        "Regular expression (case-sensitive)",
    ),
    ("正在获取设备快照…", "Fetching device snapshots…"),
    (
        "没有匹配的设备、空间或会话",
        "No matching devices, spaces or sessions",
    ),
    (
        "↑↓ 选择 · Enter 打开 · Esc 关闭",
        "↑↓ Select · Enter Open · Esc Close",
    ),
    ("查询中…", "Querying…"),
    ("上一页", "Previous"),
    ("下一页", "Next"),
    ("已定位并选中匹配文本", "Match located and selected"),
    (
        "当前终端中没有匹配文本",
        "No matching text in this terminal",
    ),
    ("请先打开一个终端", "Open a terminal first"),
    ("请输入搜索内容", "Enter a search query"),
    ("终端已关闭", "The terminal has closed"),
    ("外观", "Appearance"),
    ("字体", "Font"),
    ("字号", "Font Size"),
    ("字重", "Font Weight"),
    ("行距", "Line Spacing"),
    ("细笔画", "Thin Strokes"),
    ("鼠标报告", "Mouse Reporting"),
    ("主题", "Theme"),
    ("语言", "Language"),
    ("界面", "Interface"),
    ("浅色", "Light"),
    ("深色", "Dark"),
    ("跟随系统", "System"),
    ("系统等宽字体", "System monospace font"),
    ("布局", "Layout"),
    ("左右内边距", "Horizontal Padding"),
    ("上下内边距", "Vertical Padding"),
    ("终端外边距", "Terminal Margin"),
    ("非活动分栏不透明度", "Inactive Split Opacity"),
    ("均分网格余量", "Balance Grid Remainder"),
    ("选中即复制", "Copy on Select"),
    (
        "终端文字与左右边缘的距离，单位为逻辑像素，范围 0–64。",
        "Distance between terminal text and the left and right edges, in logical pixels. Range: 0–64.",
    ),
    (
        "终端文字与上下边缘的距离，单位为逻辑像素，范围 0–64。",
        "Distance between terminal text and the top and bottom edges, in logical pixels. Range: 0–64.",
    ),
    (
        "终端区域与分栏边框之间的留白，范围 0–64。",
        "Space between the terminal area and split borders. Range: 0–64.",
    ),
    (
        "范围 0.15–1；1 表示不淡化，只影响非活动分栏。",
        "Range: 0.15–1. Set to 1 for no fading. Only affects inactive splits.",
    ),
    (
        "将不足一格的剩余空间均分到两侧，不改变设置的基础内边距。",
        "Distribute leftover space smaller than a grid cell equally on both sides without changing the configured base padding.",
    ),
    (
        "选中文本后写入系统剪贴板；默认关闭。",
        "Copy selected text to the system clipboard. Off by default.",
    ),
    ("交互", "Interaction"),
    ("提醒", "Alerts"),
    ("通知", "Notifications"),
    ("桌面通知", "Desktop Notifications"),
    ("提示音", "Sound"),
    ("通知授权", "Notification Permission"),
    ("系统权限", "System Permission"),
    ("请求权限", "Request Permission"),
    ("系统设置…", "System Settings…"),
    ("系统已允许通知", "Notifications allowed by the system"),
    ("系统已拒绝通知权限", "Notifications denied by the system"),
    ("尚未请求通知权限", "Notification permission not requested"),
    ("尚未读取授权状态", "Permission status not yet loaded"),
    (
        "已获得临时通知授权",
        "Provisional notification permission granted",
    ),
    ("✓ 已开启", "✓ On"),
    ("已关闭", "Off"),
    ("✓ 启用", "✓ Enabled"),
    ("停用", "Disable"),
    ("已保存：开启 · 不可用", "Saved: On · Unavailable"),
    ("已保存：关闭 · 不可用", "Saved: Off · Unavailable"),
    ("可用 Agents", "Available Agents"),
    ("启动行为", "Launch Behavior"),
    (
        "默认跳过 Agent 权限确认",
        "Skip Agent Permission Prompts by Default",
    ),
    ("自动检测可执行文件", "Detect executable automatically"),
    ("路径…", "Path…"),
    ("上移", "Move Up"),
    ("下移", "Move Down"),
    ("快速新建 Agent", "Quick New Agent"),
    ("快捷键", "Shortcuts"),
    ("键盘快捷键", "Keyboard Shortcuts"),
    (
        "以下为工作台固定快捷键，不能在此修改。",
        "These workbench shortcuts are fixed and cannot be changed here.",
    ),
    ("未绑定", "Unbound"),
    ("还原默认快捷键", "Restore Default Shortcuts"),
    ("快捷键已保存", "Shortcuts saved"),
    ("Agent 快捷键已保存", "Agent shortcut saved"),
    ("已还原 Agent 快捷键", "Agent shortcut restored"),
    ("已还原默认快捷键", "Default shortcuts restored"),
    ("恢复快捷键失败", "Failed to restore shortcuts"),
    ("隐藏空间列表", "Hide Space List"),
    ("优先显示会话", "Prioritize Active Sessions"),
    (
        "跟随系统，或始终使用浅色、深色外观。",
        "Follow the system, or always use a light or dark appearance.",
    ),
    (
        "留空跟随系统；语言标识示例：zh-Hans、en。",
        "Leave blank to follow the system; supported languages: zh-Hans, en.",
    ),
    (
        "留空使用系统等宽字体。",
        "Leave blank to use the system monospace font.",
    ),
    ("允许范围 9–22。", "Allowed range: 9–22."),
    (
        "允许范围 −1–1；0 为常规，0.4 为粗体。",
        "Allowed range: −1–1; 0 is regular, 0.4 is bold.",
    ),
    (
        "行高倍率，允许范围 1–1.4。",
        "Line-height multiplier; allowed range: 1–1.4.",
    ),
    (
        "当前渲染器不支持笔画细化；保留旧设置，不改变渲染。",
        "The renderer does not support thin strokes. The stored setting is preserved without changing rendering.",
    ),
    (
        "向支持鼠标的终端程序发送鼠标事件。",
        "Send mouse events to terminal programs that support them.",
    ),
    (
        "Agent 完成任务或等待处理时发送提醒。",
        "Notify when an Agent finishes or needs attention.",
    ),
    (
        "通知开启时播放提示音。",
        "Play a sound when notifications are enabled.",
    ),
    (
        "若权限被拒绝，请在系统设置中允许通知。应用内开关不会绕过系统授权。",
        "If access is denied, allow notifications in System Settings. App toggles cannot bypass system permissions.",
    ),
    (
        "仅改变侧栏显示，不删除空间或结束会话。",
        "Only changes sidebar visibility. No spaces are deleted or sessions ended.",
    ),
    (
        "优先关注正在进行的会话。",
        "Prioritize sessions that need attention.",
    ),
    (
        "仅在你信任的工作目录中启用；Agent 可能无需再次询问就修改文件或执行命令。",
        "Enable only in trusted working directories. Agents may modify files or run commands without asking again.",
    ),
    (
        "使用 cmd、shift、alt 与按键组合，例如 cmd-shift-t。保存后立即生效；冲突不会覆盖原配置。",
        "Combine cmd, shift and alt with a key, e.g. cmd-shift-t. Changes apply immediately; conflicts do not overwrite existing shortcuts.",
    ),
    ("会话详情", "Session Details"),
    ("修改标签", "Edit Label"),
    ("修改身份", "Edit Identity"),
    ("原始元数据", "Raw Metadata"),
    ("发送文字（不追加回车）", "Send Text (without Return)"),
    ("复制输出", "Copy Output"),
    ("尚未读取输出", "Output not loaded"),
    ("当前会话不是 Agent", "The current session is not an Agent"),
    ("当前会话没有标签标识", "The session has no tab identifier"),
    ("提交 Agent 提示词", "Submit Agent Prompt"),
    ("新的 Agent 身份名称", "New Agent identity name"),
    ("服务端可见输出", "Server-visible Output"),
    (
        "服务端已确认此操作；不会自动重复发送。",
        "The server acknowledged this operation. It will not be resent automatically.",
    ),
    (
        "标签显示名称（支持中文，不改变 Agent 身份）",
        "Tab display label (Unicode supported; does not change Agent identity)",
    ),
    (
        "标签显示名称，可用中文",
        "Tab display label (Unicode supported)",
    ),
    ("正在读取 / 执行…", "Reading / running…"),
    ("直接输出", "Direct Output"),
    ("真实输出", "Actual Output"),
    (
        "要发送的文字（不会自动追加回车）",
        "Text to send (Return is not appended)",
    ),
    ("设备未连接", "Device not connected"),
    ("设备标识缺失", "Missing device identifier"),
    ("详情已关闭", "Details closed"),
    ("请先选择一个服务端会话", "Select a server session first"),
    ("请输入内容", "Enter content"),
    ("输入内容", "Input Content"),
    (
        "Agent 身份名称（用于 CLI 定位，不是显示标签）",
        "Agent identity name (used by CLI, not a display label)",
    ),
    (
        "Agent 身份名称须以小写字母开头，最多32位小写字母、数字、_ 或 -",
        "Agent identity must start with a lowercase letter and contain at most 32 lowercase letters, digits, _ or -.",
    ),
    ("会话标识缺失", "Missing session identifier"),
    ("内容不能包含NUL", "Content cannot contain NUL"),
    ("本机", "This Mac"),
    ("名称不能为空", "Name cannot be empty"),
    (
        "名称不能包含控制字符",
        "Name cannot contain control characters",
    ),
    (
        "请先完成或取消当前空间的名称编辑",
        "Finish or cancel editing the current space name first",
    ),
    (
        "空间或连接已变化，请取消后重新编辑名称",
        "The space or connection changed. Cancel and edit the name again",
    ),
    (
        "另一会话有名称草稿；返回继续编辑，或关闭提示后按 Escape 取消草稿",
        "Another session has a name draft. Return to continue, or dismiss this message and press Escape to discard it",
    ),
    (
        "会话或连接已变化，请取消后重新编辑名称",
        "The session or connection changed. Cancel and edit the name again",
    ),
    ("设备名称不能为空", "Device name cannot be empty"),
    (
        "不能移除固定本机设备",
        "The fixed local device cannot be removed",
    ),
    ("仅能启动本机服务", "Only the local service can be started"),
    (
        "会话已不在当前快照中，请刷新",
        "The session is no longer in the snapshot. Refresh first.",
    ),
    (
        "冲突策略应为 fail、replace 或 keep-both",
        "Conflict policy must be fail, replace or keep-both",
    ),
    ("可执行文件不存在", "Executable does not exist"),
    ("字号应在 9–22 之间", "Font size must be between 9 and 22"),
    (
        "行距应在 1–1.4 之间",
        "Line spacing must be between 1 and 1.4",
    ),
    ("无效字重", "Invalid font weight"),
    ("留白应在 0–64 之间", "Spacing must be between 0 and 64"),
    (
        "非活动分栏不透明度应在 0.15–1 之间",
        "Inactive pane opacity must be between 0.15 and 1",
    ),
    ("快捷键必须是 JSON 对象", "Shortcuts must be a JSON object"),
    ("设备不存在", "Device does not exist"),
    (
        "设备未连接，请先重新连接",
        "Device not connected. Reconnect first.",
    ),
    (
        "该 Agent 已在设置中禁用",
        "This Agent is disabled in settings",
    ),
    (
        "该 Agent 未在此设备的可用目录中，请刷新或检查可执行路径",
        "This Agent is unavailable on the device. Refresh or check its executable path.",
    ),
    (
        "该 Agent 未声明文件附件能力",
        "This Agent does not declare file attachment support",
    ),
    (
        "请先在侧栏选择或新建一个空间",
        "Select or create a space in the sidebar first",
    ),
    ("请先打开终端", "Open a terminal first"),
    (
        "请在侧栏选中与活动终端对应的 Agent，再发送附件",
        "Select the Agent associated with the active terminal before sending an attachment",
    ),
    (
        "请输入有效 Herdr CLI 绝对路径",
        "Enter a valid absolute Herdr CLI path",
    ),
    (
        "请输入有效 SSH target，例如 user@host",
        "Enter a valid SSH target, such as user@host",
    ),
    ("请输入空间名称", "Enter a space name"),
    (
        "请选择 Agent 类型或 terminal",
        "Choose an Agent kind or terminal",
    ),
    ("本地路径须为绝对路径", "The local path must be absolute"),
    ("远端路径不能为空", "Remote path cannot be empty"),
    (
        "附件必须是本地普通文件",
        "Attachment must be a regular local file",
    ),
    ("图片格式无法识别", "Unrecognized image format"),
    (
        "已创建标签但未返回 pane_id；请刷新，勿重复提交",
        "The tab was created, but no pane_id was returned. Refresh; do not submit again.",
    ),
    (
        "此 pane 没有可重命名的 tab，请刷新",
        "This pane has no tab to rename. Refresh first.",
    ),
    ("Agent 名称格式无效", "Invalid Agent name"),
    (
        "Agent 名称需为小写字母开头、最多32位字母数字、_ 或 -",
        "Agent names must start with a lowercase letter and contain up to 32 letters, digits, _ or -.",
    ),
    (
        "语言应为 system、zh-Hans 或 en",
        "Language must be system, zh-Hans or en",
    ),
    ("默认：{value}", "Default: {value}"),
    (
        "默认：{value}；还原仅影响此 Agent。",
        "Default: {value}; resetting affects only this Agent.",
    ),
    ("快捷键必须是对象", "Shortcuts must be an object"),
    ("Agent 快捷键数据损坏", "Agent shortcut data is invalid"),
    ("Agent 类型无效", "Invalid Agent kind"),
    (
        "请输入单组快捷键，例如 cmd-shift-t",
        "Enter one shortcut, such as cmd-shift-t",
    ),
    (
        "请先在 Agent 设置启用 Cursor",
        "Enable Cursor in Agent settings first",
    ),
    (
        "允许只读获取 Cursor 用量？",
        "Allow read-only access to Cursor usage?",
    ),
    (
        "仅读取 Cursor 已有登录凭证并向 Cursor 请求账户用量。不会刷新、修改或导出登录凭证；你可随时断开。",
        "Read Cursor's existing sign-in only to request account usage from Cursor. Sign-in credentials will not be refreshed, modified, or exported. You can disconnect at any time.",
    ),
    ("允许只读获取", "Allow read-only access"),
    ("正在读取…", "Loading…"),
    (
        "用量助手不可用，请重试或检查安装",
        "Usage helper unavailable. Retry or check the installation.",
    ),
    ("尚未连接", "Not connected"),
    ("已禁用", "Disabled"),
    ("暂无用量数据", "No usage data yet"),
    ("命令可用", "Command available"),
    ("未找到命令", "Command not found"),
    ("CLI 命令", "CLI command"),
    ("命令名或可执行文件路径", "Command or executable path"),
    (
        "留空则自动检测。应用或按 Return 保存。",
        "Leave empty for automatic detection. Apply or press Return to save.",
    ),
    ("应用路径", "Apply path"),
    ("用量账户", "Usage account"),
    ("已检测到 Cursor 应用", "Cursor app detected"),
    ("未检测到 Cursor 应用", "Cursor app not detected"),
    (
        "用量跟随此 Agent 现有的本机登录。当前结果见用量菜单。",
        "Usage follows this Agent's existing local sign-in. Check the usage menu for current results.",
    ),
    (
        "该 Agent 暂不支持用量查询，命令仍可独立使用。",
        "Usage is not supported for this Agent. Its command can still be used independently.",
    ),
    (
        "Agent 已禁用 · 不会占用快捷键，也不会后台查询用量。",
        "Agent disabled · no shortcuts or background usage queries.",
    ),
    (
        "只有已启用的 Agent 会出现在快捷键中。",
        "Only enabled Agents appear in shortcuts.",
    ),
    ("刷新策略", "Refresh policy"),
    (
        "已启用 Agent 每 5 分钟刷新用量。Cursor 还需要只读授权。",
        "Usage refreshes every 5 minutes for enabled Agents. Cursor also requires read-only authorization.",
    ),
    ("Agent 用量", "Agent usage"),
    ("本机 Mac", "This Mac"),
    ("刷新用量", "Refresh usage"),
    (
        "请选择本机以查看这台 Mac 的用量。",
        "Select Local to view usage on this Mac.",
    ),
    (
        "请在现有 Agent 设置中启用一个 Agent。",
        "Enable an Agent in the existing Agent settings.",
    ),
    ("Agent 设置", "Agent settings"),
    ("当前窗口用量", "Session usage"),
    ("周期用量", "Weekly usage"),
    ("每月用量", "Monthly usage"),
    ("Fable 每周用量", "Fable weekly usage"),
    ("重置额度", "Reset credits"),
    ("已获得", "Earned"),
    (
        "登录已过期，请打开 Agent 命令行刷新登录。",
        "Sign-in expired. Open the Agent CLI to refresh it.",
    ),
    (
        "OpenCode Go 需要网页会话 cookie；本机没有现成的凭证设置。",
        "OpenCode Go needs a web session cookie; this host has no existing credential setting.",
    ),
    (
        "用量暂不可用。请检查 Agent 登录和方案。",
        "Usage unavailable. Check the Agent sign-in and plan.",
    ),
    (
        "无法读取用量。检查 Agent 后重试。",
        "Unable to read usage. Retry after checking the Agent.",
    ),
    ("本地历史暂不可读。", "Local history could not be read."),
    ("会话", "Sessions"),
    ("正在验证 Cursor 用量…", "Verifying Cursor usage…"),
    ("用量已连接", "Usage connected"),
    ("缓存用量 · 已过期", "Cached usage · out of date"),
    ("断开用量连接", "Disconnect usage"),
    ("取消连接", "Cancel connection"),
    ("重新查询", "Retry usage"),
    ("连接用量…", "Connect usage…"),
    (
        "请在 Agent 设置中连接 Cursor 用量。",
        "Connect Cursor usage in Agent settings.",
    ),
    (
        "Cursor 应用和 CLI 是否可用，不会自动连接账户用量。",
        "Cursor App and CLI availability do not connect account usage.",
    ),
    (
        "Cursor 登录已过期，请打开 Cursor 重新登录后重试。",
        "Cursor sign-in expired. Open Cursor to sign in again, then retry.",
    ),
    ("自动用量", "Auto usage"),
    ("API 用量", "API usage"),
    ("总用量", "Total usage"),
    ("套餐支出", "Plan spend"),
    ("包含额度支出", "Included spend"),
    ("套餐上限", "Plan limit"),
    ("账期", "Billing cycle"),
    ("未知", "Unknown"),
    ("天前", "days ago"),
    ("小时前", "hours ago"),
    ("Oh My Pi", "Oh My Pi"),
    ("OpenCode", "OpenCode"),
    ("方案", "Plan"),
    ("历史数据暂不可读", "History is temporarily unavailable"),
    (
        "账户配额暂不可用；已有历史数据仍显示",
        "Account quota unavailable; existing history is still shown",
    ),
    ("当前窗口", "Current window"),
    ("每周", "Weekly"),
    ("每月", "Monthly"),
    ("Fable 每周", "Fable weekly"),
    ("重置时间", "Resets"),
    ("分钟窗口", "minute window"),
    ("历史会话", "Sessions"),
    ("输入", "Input"),
    ("输出", "Output"),
    ("估算", "Estimated"),
    ("可用重置额度", "Available reset credits"),
    ("累计重置额度", "Total earned reset credits"),
    ("额度到期", "Credits expire"),
    ("已用包含额度", "Included spend"),
    ("账期结束", "Billing cycle ends"),
    ("自动", "Auto"),
    ("总量", "Total"),
    ("方案已用", "Plan spend"),
    ("上次验证", "Last verified"),
    ("分钟前", "minutes ago"),
    (" · 缓存已过期", " · Cached data is out of date"),
    ("断开 Cursor 用量", "Disconnect Cursor usage"),
    ("连接 Cursor 用量…", "Connect Cursor usage…"),
    ("本机 Agent 用量", "Local Agent usage"),
    ("请先登录 Cursor 应用", "Sign in to the Cursor app first"),
    (
        "Cursor 登录已过期，请在 Cursor 重新登录",
        "Cursor sign-in expired. Sign in again in Cursor.",
    ),
    (
        "Cursor 拒绝此登录，请检查账户权限",
        "Cursor rejected this sign-in. Check account access.",
    ),
    (
        "Cursor 登录存储暂不可读，请打开 Cursor 后重试",
        "Cursor sign-in storage is temporarily unreadable. Open Cursor and retry.",
    ),
    (
        "无法连接 Cursor，请检查网络",
        "Cannot reach Cursor. Check your network.",
    ),
    (
        "Cursor 未返回支持的用量字段，数值保持未知",
        "Cursor returned no supported usage fields. Values remain unknown.",
    ),
    (
        "用量助手不可用，请检查安装",
        "Usage helper unavailable. Check the installation.",
    ),
    (
        "Cursor 用量请求失败，请重试",
        "Cursor usage request failed. Retry.",
    ),
    ("约", "About"),
    ("天", "d"),
    ("小时", "h"),
    ("后", " from now"),
    ("天后", "days from now"),
    ("小时后", "hours from now"),
    ("分钟后", "minutes from now"),
    (
        "通知缺少设备或窗格标识",
        "Notification is missing a device or pane identifier",
    ),
    ("应用路径不可用", "Application path unavailable"),
    ("不支持的用量来源", "Unsupported usage provider"),
    (
        "Cursor 用量尚未授权",
        "Cursor usage has not been authorized",
    ),
    ("用量来源不匹配", "Usage provider mismatch"),
    ("无效的用量数据", "Invalid usage data"),
    ("Goose herdr GPUI", "Goose herdr GPUI"),
    ("关闭分栏", "Close Split"),
    ("关闭终端", "Close Terminal"),
    ("关闭文件", "Close Files"),
    ("关闭 Agent", "Close Agent"),
    ("关闭空间", "Close Space"),
    ("此空间没有终端", "This space has no terminals"),
    ("正在恢复空间…", "Restoring space…"),
    ("选择一个会话", "Select a session"),
    ("尚无空间", "No spaces yet"),
    (
        "此 Agent 仍在运行，将被终止。",
        "This agent is still running and will be terminated.",
    ),
    ("此空空间将被移除。", "This empty space will be removed."),
    (
        "该空间中的所有终端和 Agent 将被关闭。",
        "All terminals and agents in this space will be closed.",
    ),
    ("冲突策略", "Conflict policy"),
    ("替换", "Replace"),
    ("保留两者", "Keep Both"),
    ("再检查一次", "Check again"),
    ("未读", "Unread"),
    (
        "跟随系统，或始终使用英语、简体中文。",
        "Follow the system, or always use English or Simplified Chinese.",
    ),
    ("简体中文", "Chinese"),
    ("高级", "Advanced"),
    ("还原此项", "Restore This Shortcut"),
    ("SSH 授权失败", "SSH authorization failed"),
    (
        "无法打开 Goose herdr GPUI 窗口",
        "Couldn't open the Goose herdr GPUI window",
    ),
    ("出了点问题", "Something went wrong"),
    ("全部空间", "All Spaces"),
    ("SSH 目标", "SSH target"),
    ("密码（留空不更改）", "Password (leave blank to keep)"),
    ("Tailcat 令牌", "Tailcat token"),
    ("Tailcat 设备", "Tailcat device"),
    ("默认 Socket 路径", "Default socket path"),
    (
        "可选；留空保留已存密码",
        "Optional; leave blank to keep saved password",
    ),
    ("粘贴 Tailcat 令牌", "Paste a Tailcat token"),
    ("请输入有效的 Tailcat 令牌", "Enter a valid Tailcat token"),
    ("密码包含无效字符", "Password contains invalid characters"),
    ("设备表单已关闭", "Device form is closed"),
    (
        "令牌保存在钥匙串。独立终端和文件功能需要 SSH。",
        "Tokens are stored in Keychain. Standalone terminals and files require SSH.",
    ),
    (
        "支持 user@host、SSH 配置别名或 user@host:port。",
        "Use user@host, an SSH config alias, or user@host:port.",
    ),
    ("留空保留已存令牌", "Leave blank to keep the saved token"),
    ("高级选项", "Advanced options"),
    ("本机 Herdr 连接", "Local Herdr connection"),
    (
        "内置 WireGuard 隧道，无需额外客户端",
        "Built-in WireGuard tunnel; no extra client needed",
    ),
    (
        "使用 OpenSSH 配置、agent、Tailscale SSH 或密码",
        "Use OpenSSH config, agent, Tailscale SSH, or a password",
    ),
    (
        "设备保存失败，凭据恢复失败；请重试保存",
        "Device save failed and credentials could not be restored; retry saving",
    ),
];
const TEMPLATES: &[(&str, &str)] = &[
    ("前往会话 {}", "Go to Session {}"),
    ("前往空间 {}", "Go to Space {}"),
    ("其他会话", "Other Sessions"),
    ("连接失败：{}", "Connection failed: {}"),
    ("控制连接中断：{}", "Control connection interrupted: {}"),
    ("关闭 \"{}\"？", "Close \"{}\"?"),
    ("关闭空间 \"{}\" on {}?", "Close space \"{}\" on {}?"),
    (
        "空间 {} 已恢复，但排序失败：{}；请刷新后重排，不要重复创建",
        "Space {} restored, but ordering failed: {}; refresh and reorder, do not create again",
    ),
    ("已连接 · {}", "Connected · {}"),
    ("事件流断开：{}", "Event stream disconnected: {}"),
    (
        "配置读取失败，原数据未覆盖：{}",
        "Could not load settings; original data preserved: {}",
    ),
    ("设置保存失败：{}", "Could not save settings: {}"),
    ("文件传输：{} / {}", "File transfer: {} / {}"),
    ("传输完成：{}", "Transfer complete: {}"),
    ("◌ {} · 已保留", "◌ {} · Retained"),
    ("{} 个本地视图 · ⌘K 搜索", "{} local views · ⌘K Search"),
    (
        "{} / {} · ↑↓ 选择 · Enter 打开 · Esc 关闭",
        "{} / {} · ↑↓ Select · Enter Open · Esc Close",
    ),
    ("设备 · {} · {}", "Device · {} · {}"),
    ("空间 · {} · {} · {}", "Space · {} · {} · {}"),
    ("详情刷新失败：{}", "Could not refresh details: {}"),
    ("可见输出读取失败：{}", "Could not read visible output: {}"),
    ("操作未完成：{}", "Operation not completed: {}"),
    ("Agent：{}", "Agent: {}"),
    ("状态：{}", "Status: {}"),
    ("目录：{}", "Directory: {}"),
    ("默认：{}", "Default: {}"),
    (
        "默认：{}；还原仅影响此 Agent。",
        "Default: {}; resetting affects only this Agent.",
    ),
    (
        "Agent {} 与系统/编辑快捷键冲突",
        "Agent {} conflicts with a system/editing shortcut",
    ),
    (
        "Agent {} 的快捷键与其他动作冲突",
        "Agent {} shortcut conflicts with another action",
    ),
    (
        "Agent {} 的快捷键必须含 cmd 或 alt",
        "Agent {} shortcut must include cmd or alt",
    ),
    ("{} 与 {} 快捷键冲突", "{} conflicts with shortcut {}"),
    (
        "{} 与文字编辑、终端或系统快捷键 {} 冲突",
        "{} conflicts with text editing, terminal or system shortcut {}",
    ),
    (
        "{}：应用快捷键须含 cmd、alt 或 ctrl-shift，Ctrl 和普通文字保留给终端",
        "{}: app shortcuts must include cmd, alt or ctrl-shift; Ctrl and plain keys are reserved for terminals",
    ),
    ("快捷键缺少 key：{}", "Shortcut is missing key: {}"),
    (
        "快捷键缺少 modifierRaw：{}",
        "Shortcut is missing modifierRaw: {}",
    ),
    ("无效快捷键修饰键：{}", "Invalid shortcut modifiers: {}"),
    ("无效快捷键：{}", "Invalid shortcut: {}"),
    ("未知快捷动作：{}", "Unknown shortcut action: {}"),
];

#[cfg(test)]
mod tests {
    #[test]
    fn language_resolution_and_templates_preserve_user_content() {
        assert_eq!(super::resolve_language("zh-Hans", "en-US"), "zh-Hans");
        assert_eq!(super::resolve_language("en", "zh-Hans-CN"), "en");
        assert_eq!(super::resolve_language("system", "zh-Hans-CN"), "zh-Hans");
        assert_eq!(super::resolve_language("", "en-US"), "en");
        let values = super::capture("设备 · {} · {}", "设备 · 设置 · /用户/终端").unwrap();
        assert_eq!(
            super::substitute("Device · {} · {}", &values),
            "Device · 设置 · /用户/终端"
        );
        assert!(super::capture("状态：{}", "/用户/状态：foo").is_none());
        let mut keys = std::collections::HashSet::new();
        for (key, value) in super::DICTIONARY {
            assert!(keys.insert(key), "duplicate translation: {key}");
            assert!(!value.is_empty());
        }
    }
    #[test]
    fn static_ui_translation_calls_have_dictionary_entries() {
        let files = [
            include_str!("app.rs"),
            include_str!("app_device.rs"),
            include_str!("app_space.rs"),
            include_str!("app_settings.rs"),
            include_str!("app_search.rs"),
            include_str!("app_inspector.rs"),
            include_str!("app_runtime.rs"),
            include_str!("main.rs"),
            include_str!("notifications.rs"),
            include_str!("usage.rs"),
        ];
        for source in files {
            for tail in source.split("tr(").skip(1) {
                let Some(tail) = tail.trim_start().strip_prefix('"') else {
                    continue;
                };
                let Some((key, _)) = tail.split_once('"') else {
                    continue;
                };
                if key.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                    assert!(
                        super::DICTIONARY
                            .iter()
                            .chain(super::TEMPLATES)
                            .any(|(k, _)| *k == key),
                        "missing UI translation: {key}"
                    );
                }
            }
        }
    }
}
