//! One local executable inventory for the New Agent surface and usage capabilities.
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    fs,
    io::Read,
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub(super) struct InstalledAgent {
    pub kind: String,
    pub path: PathBuf,
}
fn executable(path: &Path) -> bool {
    path.is_file()
        && CString::new(path.as_os_str().as_bytes())
            .is_ok_and(|p| unsafe { libc::access(p.as_ptr(), libc::X_OK) == 0 })
}
fn capture_path() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let shell = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && executable(p))
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"));
    let mut random = [0u8; 16];
    fs::File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut random)
        .ok()?;
    let suffix = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let path = PathBuf::from(format!("/tmp/ghg-path-{suffix}"));
    let _file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .ok()?;
    struct Remove(PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _remove = Remove(path.clone());
    let deadline = Instant::now() + Duration::from_secs(5);
    for interactive in [true, false] {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut command = Command::new(&shell);
        if interactive {
            command.arg("-i");
        }
        command
            .args([
                "-l",
                "-c",
                "printf '%s' \"$PATH\" > \"$HERDRM_SHELL_PATH_CAPTURE_FILE\"",
            ])
            .env("HERDRM_SHELL_PATH_CAPTURE_FILE", &path)
            .env("HOME", &home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let Ok(mut child) = command.spawn() else {
            continue;
        };
        let start = Instant::now();
        let timeout = if interactive {
            remaining.min(Duration::from_secs(3))
        } else {
            remaining
        };
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        if let Ok(bytes) = fs::read(&path) {
                            if bytes.len() <= 1024 * 1024 {
                                if let Ok(value) = String::from_utf8(bytes) {
                                    if !value.is_empty() && !value.contains('\0') {
                                        return Some(value);
                                    }
                                }
                            }
                        }
                    }
                    break;
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                Ok(None) => {}
            }
            if start.elapsed() >= timeout {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
    None
}
pub(crate) fn search_directories() -> &'static Vec<PathBuf> {
    static PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    PATHS.get_or_init(|| {
        let mut paths = Vec::new();
        let mut seen = HashSet::new();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut add = |p: PathBuf| {
            if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
                paths.push(p)
            }
        };
        if let Some(path) = capture_path() {
            for p in std::env::split_paths(&path) {
                add(p)
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            for p in std::env::split_paths(&path) {
                add(p)
            }
        }
        for p in [
            ".local/bin",
            ".bun/bin",
            ".cargo/bin",
            ".local/share/mise/shims",
            ".volta/bin",
        ] {
            add(home.join(p));
        }
        for p in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ] {
            add(p.into());
        }
        paths
    })
}
pub(crate) fn find_executable(query: &str) -> Option<PathBuf> {
    let query = query.trim();
    if query.is_empty() || query.contains('\0') {
        return None;
    }
    if query.contains('/') || query == "~" {
        let path = if query == "~" {
            PathBuf::from(std::env::var_os("HOME")?)
        } else if let Some(p) = query.strip_prefix("~/") {
            PathBuf::from(std::env::var_os("HOME")?).join(p)
        } else {
            PathBuf::from(query)
        };
        return executable(&path).then_some(path);
    }
    if query.starts_with('-') || query.chars().any(char::is_whitespace) {
        return None;
    }
    search_directories()
        .iter()
        .map(|p| p.join(query))
        .find(|p| executable(p))
}
pub(super) fn validate_override(query: &str) -> anyhow::Result<()> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(());
    }
    let path = Path::new(query);
    if path.is_absolute() {
        anyhow::ensure!(executable(path), "可执行文件不存在或没有执行权限");
    } else {
        // ponytail: command names are resolved by the existing background catalog, never a shell on the UI thread.
        anyhow::ensure!(
            !query.starts_with('-')
                && query
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b)),
            "请输入命令名或可执行文件的绝对路径，不要包含参数"
        );
    }
    Ok(())
}
pub(super) fn installed_agents(
    advertised: &[String],
    overrides: &HashMap<String, String>,
) -> Vec<InstalledAgent> {
    let mut seen = HashSet::new();
    advertised
        .iter()
        .map(String::as_str)
        .chain(["omp"])
        .filter(|kind| seen.insert((*kind).to_owned()))
        .filter_map(|kind| {
            let default = if kind == "cursor" {
                "cursor-agent"
            } else {
                kind
            };
            let query = overrides
                .get(kind)
                .map(String::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(default);
            find_executable(query).map(|path| InstalledAgent {
                kind: kind.into(),
                path,
            })
        })
        .collect()
}

impl super::AppView {
    pub(super) fn refresh_local_catalog(&mut self) {
        let overrides = self.settings.agent_binary_overrides.clone();
        let mut kinds: Vec<String> = super::known_agents()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        kinds.extend(self.settings.agent_kind_order.iter().cloned());
        if let Some(state) = self.states.get(&crate::herdr::Device::local().id) {
            kinds.extend(state.manifests.iter().filter_map(|m| {
                m.get("agent")
                    .or_else(|| m.get("kind"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            }));
        }
        let tx = self.tx.clone();
        thread::spawn(move || {
            let installed = installed_agents(&kinds, &overrides);
            let _ = tx.send(super::Worker::LocalCatalog(overrides, installed));
        });
    }

    pub(super) fn apply_local_catalog(
        &mut self,
        overrides: HashMap<String, String>,
        installed: Vec<InstalledAgent>,
    ) -> bool {
        if overrides != self.settings.agent_binary_overrides {
            return false;
        }
        let state = self
            .states
            .entry(crate::herdr::Device::local().id)
            .or_insert_with(|| super::DeviceState {
                connection: None,
                snapshot: None,
                status: "未连接".into(),
                generation: 0,
                loading: false,
                event_cancel: None,
                event_generation: 0,
                refresh_pending: false,
                manifests: Vec::new(),
                event_panes: Vec::new(),
                catalog: Vec::new(),
                catalog_paths: HashMap::new(),
            });
        state.catalog = installed.iter().map(|agent| agent.kind.clone()).collect();
        state.catalog_paths = installed
            .into_iter()
            .map(|agent| (agent.kind, agent.path.display().to_string()))
            .collect();
        super::publish_installed_agent_kinds(state.catalog.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::{executable, find_executable, validate_override};
    use std::path::Path;
    #[test]
    fn paths_are_not_commands() {
        assert!(find_executable("x; echo unsafe").is_none());
        assert!(find_executable("\0").is_none());
        assert!(executable(Path::new("/bin/sh")));
        assert!(!executable(Path::new("/tmp")));
        assert!(validate_override("cursor-agent").is_ok());
        assert!(validate_override("/bin/sh").is_ok());
        assert!(validate_override("").is_ok());
        for query in [
            "echo;whoami",
            "-x",
            "../agent",
            "agent --unsafe",
            "/tmp",
            "\0",
        ] {
            assert!(validate_override(query).is_err());
        }
    }
}
