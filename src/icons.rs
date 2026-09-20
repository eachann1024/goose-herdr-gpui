use gpui::{AssetSource, SharedString, Styled, Svg, px, rgb, svg};
use std::borrow::Cow;

pub struct Icons;

const ASSETS: &[(&str, &[u8])] = &[
    (
        "illustrations/empty-space.png",
        include_bytes!("../resources/illustrations/empty-space.png"),
    ),
    (
        "illustrations/workspace.png",
        include_bytes!("../resources/illustrations/workspace.png"),
    ),
    (
        "illustrations/search.png",
        include_bytes!("../resources/illustrations/search.png"),
    ),
    (
        "illustrations/files.png",
        include_bytes!("../resources/illustrations/files.png"),
    ),
    (
        "illustrations/usage.png",
        include_bytes!("../resources/illustrations/usage.png"),
    ),
    (
        "illustrations/connection.png",
        include_bytes!("../resources/illustrations/connection.png"),
    ),
    (
        "icons/threads_sidebar_left_open.svg",
        include_bytes!("../resources/icons/threads_sidebar_left_open.svg"),
    ),
    (
        "icons/plus.svg",
        include_bytes!("../resources/icons/plus.svg"),
    ),
    (
        "icons/chevron_down.svg",
        include_bytes!("../resources/icons/chevron_down.svg"),
    ),
    (
        "icons/chevron_right.svg",
        include_bytes!("../resources/icons/chevron_right.svg"),
    ),
    (
        "icons/ellipsis.svg",
        include_bytes!("../resources/icons/ellipsis.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../resources/icons/close.svg"),
    ),
    (
        "icons/magnifying_glass.svg",
        include_bytes!("../resources/icons/magnifying_glass.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../resources/icons/settings.svg"),
    ),
    (
        "icons/folder.svg",
        include_bytes!("../resources/icons/folder.svg"),
    ),
    (
        "icons/terminal.svg",
        include_bytes!("../resources/icons/terminal.svg"),
    ),
    (
        "icons/split.svg",
        include_bytes!("../resources/icons/split.svg"),
    ),
    (
        "icons/split_alt.svg",
        include_bytes!("../resources/icons/split_alt.svg"),
    ),
    (
        "icons/refresh_title.svg",
        include_bytes!("../resources/icons/refresh_title.svg"),
    ),
    (
        "icons/warning.svg",
        include_bytes!("../resources/icons/warning.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../resources/icons/check.svg"),
    ),
    (
        "icons/bell.svg",
        include_bytes!("../resources/icons/bell.svg"),
    ),
    (
        "icons/bell_fill.svg",
        include_bytes!("../resources/icons/bell_fill.svg"),
    ),
    (
        "icons/bolt_outlined.svg",
        include_bytes!("../resources/icons/bolt_outlined.svg"),
    ),
    (
        "icons/screen.svg",
        include_bytes!("../resources/icons/screen.svg"),
    ),
    (
        "icons/project_rust.svg",
        include_bytes!("../resources/icons/project_rust.svg"),
    ),
    (
        "icons/project_java.svg",
        include_bytes!("../resources/icons/project_java.svg"),
    ),
    (
        "icons/project_python.svg",
        include_bytes!("../resources/icons/project_python.svg"),
    ),
    (
        "icons/project_javascript.svg",
        include_bytes!("../resources/icons/project_javascript.svg"),
    ),
    (
        "icons/project_typescript.svg",
        include_bytes!("../resources/icons/project_typescript.svg"),
    ),
    (
        "icons/project_html.svg",
        include_bytes!("../resources/icons/project_html.svg"),
    ),
    (
        "icons/git_branch.svg",
        include_bytes!("../resources/icons/git_branch.svg"),
    ),
    (
        "icons/arrow_left.svg",
        include_bytes!("../resources/icons/arrow_left.svg"),
    ),
    (
        "icons/copy.svg",
        include_bytes!("../resources/icons/copy.svg"),
    ),
    (
        "icons/arrow_up_right.svg",
        include_bytes!("../resources/icons/arrow_up_right.svg"),
    ),
    (
        "icons/agents/amp.svg",
        include_bytes!("../resources/icons/agents/amp.svg"),
    ),
    (
        "icons/agents/claude.svg",
        include_bytes!("../resources/icons/agents/claude.svg"),
    ),
    (
        "icons/agents/cline.svg",
        include_bytes!("../resources/icons/agents/cline.svg"),
    ),
    (
        "icons/agents/codex.svg",
        include_bytes!("../resources/icons/agents/codex.svg"),
    ),
    (
        "icons/agents/copilot.svg",
        include_bytes!("../resources/icons/agents/copilot.svg"),
    ),
    (
        "icons/agents/cursor.svg",
        include_bytes!("../resources/icons/agents/cursor.svg"),
    ),
    (
        "icons/agents/deepseek.svg",
        include_bytes!("../resources/icons/agents/deepseek.svg"),
    ),
    (
        "icons/agents/devin.svg",
        include_bytes!("../resources/icons/agents/devin.svg"),
    ),
    (
        "icons/agents/gemini.svg",
        include_bytes!("../resources/icons/agents/gemini.svg"),
    ),
    (
        "icons/agents/githubcopilot.svg",
        include_bytes!("../resources/icons/agents/githubcopilot.svg"),
    ),
    (
        "icons/agents/grok.svg",
        include_bytes!("../resources/icons/agents/grok.svg"),
    ),
    (
        "icons/agents/kimi.svg",
        include_bytes!("../resources/icons/agents/kimi.svg"),
    ),
    (
        "icons/agents/kiro.svg",
        include_bytes!("../resources/icons/agents/kiro.svg"),
    ),
    (
        "icons/agents/moonshot.svg",
        include_bytes!("../resources/icons/agents/moonshot.svg"),
    ),
    (
        "icons/agents/openai.svg",
        include_bytes!("../resources/icons/agents/openai.svg"),
    ),
    (
        "icons/agents/opencode.svg",
        include_bytes!("../resources/icons/agents/opencode.svg"),
    ),
    (
        "icons/agents/pi.svg",
        include_bytes!("../resources/icons/agents/pi.svg"),
    ),
    (
        "icons/agents/qwen.svg",
        include_bytes!("../resources/icons/agents/qwen.svg"),
    ),
];

impl AssetSource for Icons {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ASSETS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        gpui_component_assets::Assets.load(path).or(Ok(None))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths: Vec<SharedString> = ASSETS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect();
        paths.extend(gpui_component_assets::Assets.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

const ICON_PATHS: &[(&str, &str)] = &[
    ("sidebar", "icons/threads_sidebar_left_open.svg"),
    ("plus", "icons/plus.svg"),
    ("chevron_down", "icons/chevron_down.svg"),
    ("chevron_right", "icons/chevron_right.svg"),
    ("more", "icons/ellipsis.svg"),
    ("close", "icons/close.svg"),
    ("search", "icons/magnifying_glass.svg"),
    ("settings", "icons/settings.svg"),
    ("folder", "icons/folder.svg"),
    ("terminal", "icons/terminal.svg"),
    ("split", "icons/split.svg"),
    ("split_alt", "icons/split_alt.svg"),
    ("refresh", "icons/refresh_title.svg"),
    ("warning", "icons/warning.svg"),
    ("check", "icons/check.svg"),
    ("bell", "icons/bell.svg"),
    ("bell_fill", "icons/bell_fill.svg"),
    ("usage", "icons/bolt_outlined.svg"),
    ("device", "icons/screen.svg"),
    ("spaces", "icons/copy.svg"),
    ("branch", "icons/git_branch.svg"),
    ("arrow_left", "icons/arrow_left.svg"),
    ("copy", "icons/copy.svg"),
    ("external_link", "icons/arrow_up_right.svg"),
];

pub fn icon(name: &str, color: u32) -> Svg {
    let path = ICON_PATHS
        .iter()
        .find(|(key, _)| *key == name)
        .map_or("icons/terminal.svg", |(_, path)| *path);
    svg()
        .path(path)
        .size(px(16.))
        .flex_shrink_0()
        .text_color(rgb(color))
}

pub fn illustration(name: &str, size: f32) -> gpui::Img {
    gpui::img(gpui::ImageSource::Resource(gpui::Resource::Embedded(
        format!("illustrations/{name}.png").into(),
    )))
    .size(px(size))
    .flex_shrink_0()
}

const PROJECT_TYPE_PATHS: &[(&str, &str)] = &[
    ("rust", "icons/project_rust.svg"),
    ("gpui", "icons/project_rust.svg"),
    ("java", "icons/project_java.svg"),
    ("python", "icons/project_python.svg"),
    ("javascript", "icons/project_javascript.svg"),
    ("typescript", "icons/project_typescript.svg"),
    ("html", "icons/project_html.svg"),
    ("pi", "icons/agents/pi.svg"),
    ("folder", "icons/folder.svg"),
];

pub fn project_type_icon(kind: &str, color: u32) -> Svg {
    let path = PROJECT_TYPE_PATHS
        .iter()
        .find(|(key, _)| *key == kind)
        .map_or("icons/folder.svg", |(_, path)| *path);
    svg()
        .path(path)
        .size(px(16.))
        .flex_shrink_0()
        .text_color(rgb(color))
}

const AGENT_PATHS: &[(&str, &str)] = &[
    ("amp", "icons/agents/amp.svg"),
    ("claude", "icons/agents/claude.svg"),
    ("cline", "icons/agents/cline.svg"),
    ("codex", "icons/agents/codex.svg"),
    ("copilot", "icons/agents/copilot.svg"),
    ("cursor", "icons/agents/cursor.svg"),
    ("deepseek", "icons/agents/deepseek.svg"),
    ("devin", "icons/agents/devin.svg"),
    ("gemini", "icons/agents/gemini.svg"),
    ("githubcopilot", "icons/agents/githubcopilot.svg"),
    ("grok", "icons/agents/grok.svg"),
    ("kimi", "icons/agents/kimi.svg"),
    ("kiro", "icons/agents/kiro.svg"),
    ("moonshot", "icons/agents/moonshot.svg"),
    ("openai", "icons/agents/openai.svg"),
    ("opencode", "icons/agents/opencode.svg"),
    ("pi", "icons/agents/pi.svg"),
    ("qwen", "icons/agents/qwen.svg"),
];

pub fn agent_icon(kind: &str, color: u32) -> Svg {
    let path = AGENT_PATHS
        .iter()
        .find(|(key, _)| *key == kind)
        .map_or("icons/terminal.svg", |(_, path)| *path);
    svg()
        .path(path)
        .size(px(16.))
        .flex_shrink_0()
        .text_color(rgb(color))
}

pub fn agent_mark(kind: Option<&str>, size: f32, color: u32) -> Svg {
    agent_icon(kind.unwrap_or("terminal"), color).size(px(size))
}

pub fn app_mark(size: f32) -> gpui::Img {
    static APP_ICON: std::sync::OnceLock<std::sync::Arc<gpui::Image>> = std::sync::OnceLock::new();
    let image = APP_ICON.get_or_init(|| {
        std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../resources/icons/app/herdrm-icon-1024.png").to_vec(),
        ))
    });
    gpui::img(image.clone()).size(px(size)).flex_shrink_0()
}

pub fn pane_kind(
    extra: &std::collections::BTreeMap<String, serde_json::Value>,
    agent: Option<&str>,
) -> Option<String> {
    let explicit = extra
        .get("agent_kind")
        .or_else(|| extra.get("kind"))
        .and_then(serde_json::Value::as_str);
    explicit
        .or_else(|| agent.filter(|kind| AGENT_PATHS.iter().any(|(name, _)| name == kind)))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_kind_does_not_treat_session_names_as_brands() {
        let mut extra = std::collections::BTreeMap::new();
        assert_eq!(pane_kind(&extra, Some("my-claude-session")), None);
        assert_eq!(pane_kind(&extra, Some("claude")).as_deref(), Some("claude"));
        extra.insert("agent_kind".into(), serde_json::json!("codex"));
        assert_eq!(pane_kind(&extra, Some("session")).as_deref(), Some("codex"));
        extra.insert("agent_kind".into(), serde_json::json!("unknown-agent"));
        assert_eq!(
            pane_kind(&extra, Some("claude")).as_deref(),
            Some("unknown-agent")
        );
    }

    #[test]
    fn every_icon_mapping_has_embedded_svg() {
        for (_, path) in ICON_PATHS
            .iter()
            .chain(AGENT_PATHS)
            .chain(PROJECT_TYPE_PATHS)
        {
            let data = Icons.load(path).unwrap().unwrap();
            assert!(
                std::str::from_utf8(&data).unwrap().contains("<svg"),
                "{path}"
            );
        }
        assert!(Icons.load("icons/terminal.svg").unwrap().is_some());
        assert!(Icons.load("icons/missing.svg").unwrap().is_none());
        assert!(Icons.list("").unwrap().len() >= ASSETS.len());
        assert!(Icons.load("icons/arrow-down.svg").unwrap().is_some());
    }

    #[test]
    fn illustrations_are_embedded_bounded_pngs() {
        for name in [
            "workspace",
            "search",
            "files",
            "usage",
            "connection",
            "empty-space",
        ] {
            let bytes = Icons
                .load(&format!("illustrations/{name}.png"))
                .unwrap()
                .unwrap();
            assert!(
                bytes.len() >= 33 && bytes.len() <= 8 * 1024 * 1024,
                "{name}"
            );
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{name}");
            for dimension in [&bytes[16..20], &bytes[20..24]] {
                assert!(
                    (64..=2048).contains(&u32::from_be_bytes(dimension.try_into().unwrap())),
                    "{name}"
                );
            }
        }
    }
}
