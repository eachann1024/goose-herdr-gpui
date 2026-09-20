use crate::icons;
use gpui::{AnyElement, Image, ImageFormat, IntoElement, ParentElement, Styled, StyledImage, px};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const SIPS_TIMEOUT: Duration = Duration::from_secs(2);
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "svg", "ico", "icns"];
const ICON_STEMS: &[&str] = &[
    "favicon",
    "public/favicon",
    "app/favicon",
    "app/icon",
    "src/favicon",
    "src/app/icon",
    "assets/favicon",
    "assets/icon",
    "static/favicon",
    "logo",
    "public/logo",
    "public/icon",
    "src-tauri/icons/icon",
    "app-icon",
    "icon",
    "AppIcon",
    "Resources/AppIcon",
    "Resources/icon",
];
const ICON_DIRECTORIES: &[&str] = &[
    "Resources/AppIcon",
    "Assets.xcassets/AppIcon.appiconset",
    "Resources/Assets.xcassets/AppIcon.appiconset",
    "Assets/AppIcon.appiconset",
    "AppIcon.appiconset",
    "resources/icons/app",
];

#[derive(Clone, Debug)]
pub enum ProjectIcon {
    Image(Arc<Image>),
    Type(&'static str),
}

impl ProjectIcon {
    fn content_size(&self, size: f32) -> f32 {
        // Default SVG artwork is 12px; images use 14px for optical balance in the same 16px slot.
        match self {
            Self::Image(_) => size * 14. / 16.,
            Self::Type("pi") => size * 12. / 16. * 24. / 22.,
            Self::Type(_) => size,
        }
    }

    /// Rendering only uses already-resolved data; call `resolve` off the UI thread.
    pub fn mark(&self, size: f32, color: u32) -> AnyElement {
        let content_size = self.content_size(size);
        let content = match self {
            Self::Image(image) => gpui::img(image.clone())
                .size(px(content_size))
                .object_fit(gpui::ObjectFit::Contain)
                .flex_shrink_0()
                .into_any_element(),
            Self::Type(kind) => icons::project_type_icon(kind, color)
                .size(px(content_size))
                .into_any_element(),
        };
        gpui::div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(content)
            .into_any_element()
    }
}

/// Resolves an absolute, local project directory without executing project code.
pub fn resolve(path: &str) -> ProjectIcon {
    let Some(directory) = local_directory(path) else {
        return ProjectIcon::Type("folder");
    };
    let root = project_root(&directory);
    find_icon(&root).unwrap_or_else(|| ProjectIcon::Type(project_type(&root)))
}

fn local_directory(path: &str) -> Option<PathBuf> {
    let path = if path == "~" {
        std::env::var_os("HOME").map(PathBuf::from)
    } else if let Some(rest) = path.strip_prefix("~/") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))
    } else {
        Some(PathBuf::from(path))
    }?;
    path.is_absolute()
        .then(|| fs::canonicalize(path).ok())
        .flatten()
        .filter(|path| fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()))
}

fn project_root(directory: &Path) -> PathBuf {
    let mut current = directory;
    while let Some(parent) = current.parent() {
        if current.join(".git").exists() {
            return current.to_owned();
        }
        current = parent;
    }
    directory.to_owned()
}

fn find_icon(root: &Path) -> Option<ProjectIcon> {
    for stem in ICON_STEMS {
        for extension in IMAGE_EXTENSIONS {
            if let Some(icon) = load_image(root, &root.join(format!("{stem}.{extension}"))) {
                return Some(icon);
            }
        }
    }
    for relative in ICON_DIRECTORIES {
        let Some(directory) = safe_directory(root, relative) else {
            continue;
        };
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut candidates = entries
            .take(64)
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        candidates.sort();
        for candidate in candidates {
            if let Some(icon) = load_image(root, &candidate) {
                return Some(icon);
            }
        }
    }
    None
}

fn safe_directory(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = fs::canonicalize(root.join(relative)).ok()?;
    (path.starts_with(root) && fs::metadata(&path).ok()?.is_dir()).then_some(path)
}

fn safe_file(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = fs::canonicalize(root.join(relative)).ok()?;
    (path.starts_with(root) && fs::metadata(&path).ok()?.is_file()).then_some(path)
}

fn load_image(root: &Path, candidate: &Path) -> Option<ProjectIcon> {
    let candidate = fs::canonicalize(candidate).ok()?;
    if !candidate.starts_with(root) || !candidate.is_file() || !has_image_extension(&candidate) {
        return None;
    }
    let metadata = fs::metadata(&candidate).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_BYTES {
        return None;
    }
    // Native ImageIO validates/decodes every supported source to a bounded PNG, including ICO and ICNS.
    let bytes = thumbnail(&candidate)?;
    Some(ProjectIcon::Image(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes,
    ))))
}

fn thumbnail(source: &Path) -> Option<Vec<u8>> {
    let directory = private_temp_dir()?;
    let output = directory.join("icon.png");
    let child = Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "--resampleHeightWidthMax", "64"])
        .arg(source)
        .arg("--out")
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        let _ = fs::remove_dir_all(directory);
        return None;
    };
    let deadline = Instant::now() + SIPS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = status
        .filter(|status| status.success())
        .and_then(|_| fs::read(&output).ok());
    let _ = fs::remove_dir_all(directory);
    bytes.filter(|bytes| valid_png(bytes))
}

fn private_temp_dir() -> Option<PathBuf> {
    use std::{
        ffi::{CStr, CString, OsStr},
        os::unix::ffi::OsStrExt,
    };
    let template = std::env::temp_dir()
        .join("goose-herdr-icon-XXXXXX")
        .into_os_string()
        .into_string()
        .ok()?;
    let mut template = CString::new(template).ok()?.into_bytes_with_nul();
    // mkdtemp atomically creates a mode-0700 directory; `sips` never writes through an attacker symlink.
    let path = unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) };
    (!path.is_null())
        .then(|| unsafe { PathBuf::from(OsStr::from_bytes(CStr::from_ptr(path).to_bytes())) })
}

fn valid_png(bytes: &[u8]) -> bool {
    bytes.len() <= MAX_BYTES as usize
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes.len() >= 24
        && (1..=4096).contains(&u32::from_be_bytes(bytes[16..20].try_into().unwrap()))
        && (1..=4096).contains(&u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
}

fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn project_type(root: &Path) -> &'static str {
    let package = read_manifest(root, "package.json");
    if package
        .as_ref()
        .is_some_and(|package| is_pi_package(package))
    {
        return "pi";
    }
    if let Some(cargo) = read_manifest(root, "Cargo.toml") {
        return if cargo.contains("gpui") {
            "gpui"
        } else {
            "rust"
        };
    }
    if ["pom.xml", "build.gradle", "build.gradle.kts"]
        .iter()
        .any(|path| safe_file(root, path).is_some())
    {
        return "java";
    }
    if ["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"]
        .iter()
        .any(|path| safe_file(root, path).is_some())
    {
        return "python";
    }
    if let Some(package) = package {
        return if package.contains("typescript") {
            "typescript"
        } else {
            "javascript"
        };
    }
    if ["index.html", "vite.config.js", "vite.config.ts"]
        .iter()
        .any(|path| safe_file(root, path).is_some())
    {
        return "html";
    }
    "folder"
}

fn is_pi_package(package: &str) -> bool {
    let Ok(package) = serde_json::from_str::<serde_json::Value>(package) else {
        return false;
    };
    let non_empty_array = |key| {
        package
            .get("pi")
            .and_then(|pi| pi.get(key))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty())
    };
    non_empty_array("extensions")
        || non_empty_array("themes")
        || package
            .get("keywords")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|keywords| {
                keywords
                    .iter()
                    .any(|keyword| keyword.as_str() == Some("pi-package"))
            })
        || [
            "@mariozechner/pi-coding-agent",
            "@mariozechner/pi-ai",
            "@mariozechner/pi-tui",
        ]
        .iter()
        .any(|key| {
            package
                .get("dependencies")
                .and_then(|dependencies| dependencies.get(key))
                .is_some()
        })
}

fn read_manifest(root: &Path, relative: &str) -> Option<String> {
    let path = safe_file(root, relative)?;
    (fs::metadata(&path).ok()?.len() <= 1024 * 1024)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn project_artwork_matches_default_icon_size() {
        let image = ProjectIcon::Image(Arc::new(Image::from_bytes(ImageFormat::Png, Vec::new())));
        for size in [12., 16.] {
            assert_eq!(image.content_size(size), size * 14. / 16.);
            assert!(
                (ProjectIcon::Type("pi").content_size(size) * 22. / 24. - size * 0.75).abs()
                    < 0.001
            );
            for kind in ["javascript", "folder", "rust"] {
                assert_eq!(ProjectIcon::Type(kind).content_size(size), size);
            }
        }
    }

    fn fixture(files: &[(&str, &str)]) -> PathBuf {
        let root = private_temp_dir().unwrap();
        for (path, contents) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        fs::canonicalize(root).unwrap()
    }

    #[test]
    fn recognizes_manifest_types() {
        let root = fixture(&[("package.json", r#"{"pi":{"extensions":["footer.ts"]}}"#)]);
        assert_eq!(project_type(&root), "pi");
        let _ = fs::remove_dir_all(root);
        for (file, expected) in [
            ("Cargo.toml", "rust"),
            ("pom.xml", "java"),
            ("pyproject.toml", "python"),
            ("package.json", "javascript"),
            ("index.html", "html"),
        ] {
            let root = fixture(&[(file, "{}")]);
            assert_eq!(project_type(&root), expected);
            let _ = fs::remove_dir_all(root);
        }
        let root = fixture(&[("Cargo.toml", "gpui = \"0.2\"")]);
        assert_eq!(project_type(&root), "gpui");
        let _ = fs::remove_dir_all(root);
        let root = fixture(&[("package.json", r#"{"devDependencies":{"typescript":"5"}}"#)]);
        assert_eq!(project_type(&root), "typescript");
        let _ = fs::remove_dir_all(root);
        assert!(is_pi_package(r#"{"keywords":["pi-package"]}"#));
        assert!(!is_pi_package(
            r#"{"pi":{"extensions":[]},"description":"pi-package"}"#
        ));
    }

    #[test]
    fn loads_logo_and_rejects_outside_symlink_and_bad_image() {
        let root = fixture(&[("public/logo.png", "not an image")]);
        assert!(matches!(
            resolve(root.to_str().unwrap()),
            ProjectIcon::Type("folder")
        ));
        fs::write(
            root.join("public/logo.png"),
            include_bytes!("../resources/icons/app/herdrm-icon-1024.png"),
        )
        .unwrap();
        assert!(matches!(
            resolve(root.to_str().unwrap()),
            ProjectIcon::Image(_)
        ));
        fs::write(root.join("public/logo.png"), "not an image").unwrap();
        assert!(find_icon(&root).is_none());
        let outside = root.with_extension("outside.png");
        fs::write(&outside, b"not an image").unwrap();
        fs::remove_file(root.join("public/logo.png")).unwrap();
        symlink(&outside, root.join("public/logo.png")).unwrap();
        assert!(load_image(&root, &root.join("public/logo.png")).is_none());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }
}
