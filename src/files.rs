use crate::{credentials, herdr::Device};
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    RegularFile,
    SymbolicLink,
    Other,
}
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: Option<u64>,
    pub modified: Option<u64>,
    pub is_package: bool,
    pub is_hidden: bool,
}
#[derive(Clone, Debug)]
pub struct DirectoryListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictPolicy {
    Fail,
    Replace,
    KeepBoth,
}
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Upload,
    Download,
}
#[derive(Clone, Copy, Debug)]
pub struct TransferProgress {
    pub completed_bytes: u64,
    pub total_bytes: u64,
}
impl TransferProgress {
    pub fn fraction_completed(&self) -> f64 {
        if self.total_bytes == 0 {
            0.
        } else {
            (self.completed_bytes as f64 / self.total_bytes as f64).min(1.)
        }
    }
}
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed)
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!("传输已取消")
        }
        Ok(())
    }
}
fn read_diagnostics(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        result.extend_from_slice(&chunk[..n]);
        if result.len() > 16384 {
            result.drain(..result.len() - 16384);
        }
    }
    Ok(result)
}
fn quote(s: &str) -> Result<String> {
    if s.contains('\0') {
        bail!("路径含有非法字符")
    }
    Ok(format!("'{}'", s.replace('\'', "'\\''")))
}
fn ssh_target(s: &str) -> Result<String> {
    crate::transport::destination(s).map_err(Into::into)
}

fn ssh(device: &Device, script: &str) -> Result<(Command, credentials::SshAuthentication)> {
    let target = device
        .ssh_target()
        .context("Files工作区需要本地设备或SSH；Tailcat请改用SSH")?;
    let auth = credentials::ssh_authentication(&device.id)?;
    let mut c = Command::new("/usr/bin/ssh");
    c.args(&auth.arguments)
        .args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=3",
        ])
        .arg(ssh_target(target)?)
        .arg(format!("exec /bin/sh -c {}", quote(script)?))
        .envs(auth.environment.iter().cloned());
    Ok((c, auth))
}
fn run(device: &Device, script: &str, cancel: &CancelToken) -> Result<Vec<u8>> {
    let (mut cmd, _auth) = ssh(device, script)?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut v = vec![];
        out.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut v)
            .map(|_| v)
    });
    let errors = std::thread::spawn(move || read_diagnostics(err));
    let start = Instant::now();
    let status = loop {
        if cancel.is_cancelled() || start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = errors.join();
            cancel.check()?;
            bail!("SSH操作超时")
        };
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let out = reader
        .join()
        .map_err(|_| anyhow::anyhow!("读取SSH输出失败"))??;
    let err = errors
        .join()
        .map_err(|_| anyhow::anyhow!("读取SSH错误失败"))??;
    if !status.success() {
        bail!("SSH文件操作失败：{}", String::from_utf8_lossy(&err))
    }
    if out.len() > 64 * 1024 * 1024 {
        bail!("目录数据过大")
    }
    Ok(out)
}
fn absolute(device: &Device, requested: &str) -> Result<String> {
    let s = requested.trim();
    let s = if s.is_empty() { "~" } else { s };
    quote(s)?;
    let expanded = if s == "~" || s.starts_with("~/") {
        let home = if device.is_local() {
            std::env::var("HOME")?
        } else {
            String::from_utf8(run(
                device,
                "printf '%s' \"$HOME\"",
                &CancelToken::default(),
            )?)?
        };
        format!("{}{}", home, &s[1..])
    } else {
        s.to_owned()
    };
    if !expanded.starts_with('/') {
        bail!("路径必须为绝对路径或以~开头")
    };
    let mut normalized = PathBuf::from("/");
    for part in Path::new(&expanded).components() {
        match part {
            std::path::Component::Normal(v) => normalized.push(v),
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => {}
        }
    }
    Ok(normalized.to_string_lossy().into_owned())
}
fn is_package(name: &str, kind: FileKind) -> bool {
    kind == FileKind::Directory
        && Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "app" | "bundle" | "framework" | "plugin" | "kext" | "appex" | "pkg"
                )
            })
}

fn is_hidden(name: &str, meta: &std::fs::Metadata) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::darwin::fs::MetadataExt;
        const UF_HIDDEN: u32 = 0x8000;
        if meta.st_flags() & UF_HIDDEN != 0 {
            return true;
        }
    }
    let _ = meta;
    false
}

/// `/` has no parent. `~` stays at `~`.
pub fn parent(path: &str) -> Option<String> {
    let mut trimmed = path.trim().to_owned();
    while trimmed.len() > 1 && trimmed.ends_with('/') {
        trimmed.pop();
    }
    if trimmed.is_empty() || trimmed == "/" {
        return None;
    }
    if trimmed == "~" {
        return Some("~".into());
    }
    let parent = Path::new(&trimmed).parent()?;
    if parent.as_os_str().is_empty() {
        return None;
    }
    Some(parent.to_string_lossy().into_owned())
}

/// Reloading a directory should keep a still-present selection.
pub fn keep_selection(entries: &[FileEntry], selected: Option<&str>) -> Option<String> {
    selected
        .filter(|path| entries.iter().any(|entry| entry.path == *path))
        .map(str::to_owned)
}

pub fn list_directory(device: &Device, path: &str, hidden: bool) -> Result<DirectoryListing> {
    let path = absolute(device, path)?;
    let mut entries = if device.is_local() {
        fs::read_dir(&path)?
            .map(|e| {
                let e = e?;
                let m = fs::symlink_metadata(e.path())?;
                let kind = if m.is_symlink() {
                    FileKind::SymbolicLink
                } else if m.is_dir() {
                    FileKind::Directory
                } else if m.is_file() {
                    FileKind::RegularFile
                } else {
                    FileKind::Other
                };
                let name = e.file_name().to_string_lossy().into_owned();
                Ok(FileEntry {
                    is_hidden: is_hidden(&name, &m),
                    is_package: is_package(&name, kind),
                    name,
                    path: e.path().to_string_lossy().into_owned(),
                    kind,
                    size: (kind == FileKind::RegularFile).then_some(m.len()),
                    modified: m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        let script = format!(
            "cd {} || exit 1\nif stat -f '%HT' . >/dev/null 2>&1; then\nfind . ! -name . -prune -exec /bin/sh -c 'for item do type=$(stat -f \"%HT\" \"$item\") || continue; size=$(stat -f \"%z\" \"$item\") || continue; modified=$(stat -f \"%m\" \"$item\") || continue; name=${{item#./}}; printf \"%s\\0%s\\0%s\\0%s\\0\" \"$name\" \"$type\" \"$size\" \"$modified\"; done' sh {{}} +\nelse\nfind . ! -name . -prune -exec /bin/sh -c 'for item do type=$(stat -c \"%F\" -- \"$item\") || continue; size=$(stat -c \"%s\" -- \"$item\") || continue; modified=$(stat -c \"%Y\" -- \"$item\") || continue; name=${{item#./}}; printf \"%s\\0%s\\0%s\\0%s\\0\" \"$name\" \"$type\" \"$size\" \"$modified\"; done' sh {{}} +\nfi",
            quote(&path)?
        );
        parse_listing(&run(device, &script, &CancelToken::default())?, &path)?
    };
    entries.retain(|e| hidden || !e.is_hidden);
    entries.sort_by(|a, b| {
        (a.kind != FileKind::Directory, a.name.to_lowercase())
            .cmp(&(b.kind != FileKind::Directory, b.name.to_lowercase()))
    });
    Ok(DirectoryListing { path, entries })
}
fn parse_listing(data: &[u8], dir: &str) -> Result<Vec<FileEntry>> {
    if data.is_empty() {
        return Ok(vec![]);
    }
    let fields = data
        .strip_suffix(&[0])
        .context("远端目录数据缺少分隔符")?
        .split(|b| *b == 0)
        .collect::<Vec<_>>();
    if fields.len() % 4 != 0 {
        bail!("远端目录数据损坏")
    };
    fields
        .chunks_exact(4)
        .map(|f| {
            let name = std::str::from_utf8(f[0])?.to_owned();
            if name.is_empty() || name == "." || name == ".." || name.contains('/') {
                bail!("远端文件名无效")
            };
            let kind = match std::str::from_utf8(f[1])?.to_lowercase().as_str() {
                "directory" => FileKind::Directory,
                "regular file" | "regular empty file" => FileKind::RegularFile,
                "symbolic link" => FileKind::SymbolicLink,
                _ => FileKind::Other,
            };
            let size = std::str::from_utf8(f[2])?.parse()?;
            Ok(FileEntry {
                path: Path::new(dir).join(&name).to_string_lossy().into_owned(),
                is_hidden: name.starts_with('.'),
                is_package: is_package(&name, kind),
                name,
                kind,
                size: (kind == FileKind::RegularFile).then_some(size),
                modified: Some(std::str::from_utf8(f[3])?.parse()?),
            })
        })
        .collect()
}
fn keep_name(name: &str, n: usize) -> String {
    let p = Path::new(name);
    match (p.file_stem(), p.extension()) {
        (Some(stem), Some(ext)) => {
            format!("{} {n}.{}", stem.to_string_lossy(), ext.to_string_lossy())
        }
        _ => format!("{name} {n}"),
    }
}
fn destination(
    device: Option<&Device>,
    directory: &Path,
    name: &str,
    policy: ConflictPolicy,
    cancel: &CancelToken,
) -> Result<PathBuf> {
    for n in 1..=10000 {
        cancel.check()?;
        let p = directory.join(if n == 1 {
            name.to_owned()
        } else {
            keep_name(name, n)
        });
        let exists = if let Some(device) = device {
            run(
                device,
                &format!(
                    "if [ -e {0} ] || [ -L {0} ]; then printf 1; else printf 0; fi",
                    quote(&p.to_string_lossy())?
                ),
                cancel,
            )? == b"1"
        } else {
            fs::symlink_metadata(&p).is_ok()
        };
        if !exists || policy == ConflictPolicy::Replace {
            return Ok(p);
        }
        if policy == ConflictPolicy::Fail {
            bail!("目标文件已存在")
        }
    }
    bail!("无法找到可用文件名")
}
fn temporary(dest: &Path) -> Result<PathBuf> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(dest.with_file_name(format!(
        ".{}.herdrm-{}.part",
        dest.file_name()
            .context("目标文件名无效")?
            .to_string_lossy(),
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )))
}
fn install(tmp: &Path, dest: &Path, policy: ConflictPolicy) -> Result<()> {
    if policy == ConflictPolicy::Replace {
        fs::rename(tmp, dest)?
    } else {
        fs::hard_link(tmp, dest).context("目标已存在或无法安装文件")?;
        fs::remove_file(tmp)?;
    }
    Ok(())
}
fn copy(
    source: &Path,
    dest: &Path,
    policy: ConflictPolicy,
    cancel: &CancelToken,
    progress: &impl Fn(TransferProgress),
) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(source)?;
    if !meta.is_file() {
        bail!("只能传输普通文件")
    };
    let tmp = temporary(dest)?;
    let result = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut input = File::open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut count = 0;
        loop {
            cancel.check()?;
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n])?;
            count += n as u64;
            progress(TransferProgress {
                completed_bytes: count,
                total_bytes: meta.len(),
            });
        }
        output.sync_all()?;
        cancel.check()?;
        install(&tmp, dest, policy)?;
        Ok(dest.to_owned())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
/// Upload: local_path is source file; remote_path is destination directory.
/// Download: local_path is destination directory; remote_path is source file.
pub fn transfer(
    device: &Device,
    local_path: &Path,
    remote_path: &str,
    direction: Direction,
    policy: ConflictPolicy,
    cancel: &CancelToken,
    progress: impl Fn(TransferProgress),
) -> Result<PathBuf> {
    cancel.check()?;
    let remote = PathBuf::from(absolute(device, remote_path)?);
    let (source, dest) = match direction {
        Direction::Upload => {
            let name = local_path
                .file_name()
                .context("源文件名无效")?
                .to_str()
                .context("文件名编码无效")?;
            (
                local_path.to_owned(),
                destination(
                    (!device.is_local()).then_some(device),
                    &remote,
                    name,
                    policy,
                    cancel,
                )?,
            )
        }
        Direction::Download => {
            let name = remote
                .file_name()
                .context("源文件名无效")?
                .to_str()
                .context("文件名编码无效")?;
            (
                remote.clone(),
                destination(None, local_path, name, policy, cancel)?,
            )
        }
    };
    if device.is_local() {
        return copy(&source, &dest, policy, cancel, &progress);
    }
    let tmp = temporary(&dest)?;
    let total = match direction {
        Direction::Upload => {
            let m = fs::symlink_metadata(&source)?;
            if !m.is_file() {
                bail!("只能上传普通文件")
            };
            m.len()
        }
        Direction::Download => String::from_utf8(run(
            device,
            &format!(
                "[ -f {0} ] && [ ! -L {0} ] && wc -c < {0}",
                quote(&source.to_string_lossy())?
            ),
            cancel,
        )?)?
        .trim()
        .parse::<u64>()?,
    };
    let script = match direction {
        Direction::Upload => format!(
            "umask 077\ntmp={}\ndest={}\ntrap 'rm -f \"$tmp\"' EXIT HUP INT TERM\n(set -C; cat > \"$tmp\") && chmod 600 \"$tmp\" && {}\nstatus=$?\n[ \"$status\" -eq 0 ] && trap - EXIT HUP INT TERM\nexit \"$status\"",
            quote(&tmp.to_string_lossy())?,
            quote(&dest.to_string_lossy())?,
            if policy == ConflictPolicy::Replace {
                "[ ! -d \"$dest\" ] && mv -f \"$tmp\" \"$dest\""
            } else {
                "ln \"$tmp\" \"$dest\" && rm -f \"$tmp\""
            }
        ),
        Direction::Download => format!("cat {}", quote(&source.to_string_lossy())?),
    };
    let (mut cmd, _auth) = ssh(device, &script)?;
    use std::os::unix::fs::OpenOptionsExt;
    let monitor: File;
    match direction {
        Direction::Upload => {
            let f = File::open(&source)?;
            monitor = f.try_clone()?;
            cmd.stdin(Stdio::from(f)).stdout(Stdio::null());
        }
        Direction::Download => {
            let f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            monitor = f.try_clone()?;
            cmd.stdin(Stdio::null()).stdout(Stdio::from(f));
        }
    };
    let result = (|| {
        let mut child = cmd.stderr(Stdio::piped()).spawn()?;
        let err = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || read_diagnostics(err));
        let mut cancelled = false;
        let status = loop {
            if cancel.is_cancelled() {
                cancelled = true;
                let _ = child.kill();
                break child.wait()?;
            }
            if let Some(s) = child.try_wait()? {
                break s;
            }
            use std::os::fd::AsRawFd;
            let offset = unsafe { libc::lseek(monitor.as_raw_fd(), 0, libc::SEEK_CUR) };
            progress(TransferProgress {
                completed_bytes: offset.max(0) as u64,
                total_bytes: total,
            });
            std::thread::sleep(Duration::from_millis(100));
        };
        let err = reader
            .join()
            .map_err(|_| anyhow::anyhow!("读取SSH错误失败"))??;
        if cancelled {
            bail!("传输已取消；远端最终状态请刷新确认")
        };
        if !status.success() {
            bail!(
                "传输失败，远端最终状态请刷新确认：{}",
                String::from_utf8_lossy(&err)
            )
        };
        if matches!(direction, Direction::Download) {
            monitor.sync_all()?;
            cancel.check()?;
            if fs::metadata(&tmp)?.len() != total {
                bail!("下载大小与源文件不一致，原文件已保留")
            };
            install(&tmp, &dest, policy)?;
        }
        progress(TransferProgress {
            completed_bytes: total,
            total_bytes: total,
        });
        Ok(dest.clone())
    })();
    if matches!(direction, Direction::Download) && result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_and_conflicting_copy_preserves_destination() {
        let root =
            std::env::temp_dir().join(format!("herdr-files-selfcheck-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        let dest = root.join("dest");
        fs::write(&source, b"new").unwrap();
        fs::write(&dest, b"old").unwrap();
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(copy(&source, &dest, ConflictPolicy::Replace, &cancel, &|_| {}).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        assert!(
            copy(
                &source,
                &dest,
                ConflictPolicy::Fail,
                &CancelToken::default(),
                &|_| {}
            )
            .is_err()
        );
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        copy(
            &source,
            &dest,
            ConflictPolicy::Replace,
            &CancelToken::default(),
            &|_| {},
        )
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn listing_and_safety() {
        let local = Device::local();
        assert_eq!(
            attachment_action(&[], "claude_code", &local, true),
            AttachmentAction::NativeClipboard
        );
        assert_eq!(
            attachment_action(
                &[serde_json::json!({"agent":"other","capabilities":{}})],
                "claude",
                &local,
                true
            ),
            AttachmentAction::Unsupported
        );
        assert_eq!(
            attachment_action(
                &[
                    serde_json::json!({"agent":"custom","aliases":["my_agent"],"capabilities":{"attachments":{"file_path":"shell_quoted"}}})
                ],
                "MY-AGENT",
                &local,
                false
            ),
            AttachmentAction::DevicePaths
        );
        assert_eq!(quote("a'b").unwrap(), "'a'\\''b'");
        assert!(quote("a\0b").is_err());
        assert_eq!(keep_name("a.txt", 2), "a 2.txt");
        assert!(ssh_target("-oProxyCommand=x").is_err());
        let e = parse_listing(b"a\0regular file\012\01700000000\0", "/").unwrap();
        assert_eq!(e[0].size, Some(12));
        assert!(!e[0].is_package);
        assert!(parse_listing(b"../x\0regular file\01\01\0", "/").is_err());
        assert_eq!(parent("/"), None);
        assert_eq!(parent("/Users"), Some("/".into()));
        assert_eq!(parent("~"), Some("~".into()));
        let app = parse_listing(b"Foo.app\0directory\00\01\0", "/").unwrap();
        assert!(app[0].is_package);
        assert_eq!(
            keep_selection(&e, Some(&e[0].path)),
            Some(e[0].path.clone())
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentAction {
    Unsupported,
    NativeClipboard,
    DevicePaths,
}
pub fn attachment_action(
    manifests: &[serde_json::Value],
    kind: &str,
    device: &Device,
    all_images: bool,
) -> AttachmentAction {
    let key = |s: &str| s.to_lowercase().replace('_', "-");
    let capability = manifests
        .iter()
        .find(|m| {
            m.get("agent")
                .and_then(|v| v.as_str())
                .is_some_and(|s| key(s) == key(kind))
                || m.get("aliases")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| {
                        a.iter()
                            .any(|s| s.as_str().is_some_and(|s| key(s) == key(kind)))
                    })
        })
        .and_then(|m| m.pointer("/capabilities/attachments"));
    let fallback = !manifests
        .iter()
        .any(|m| m.get("capabilities").is_some_and(|v| !v.is_null()))
        && [
            "claude",
            "claude-code",
            "codex",
            "codex-cli",
            "openai-codex",
            "copilot",
            "github-copilot",
            "ghcs",
            "cursor",
            "cursor-agent",
            "gemini",
            "grok",
            "grok-build",
            "opencode",
            "open-code",
            "pi",
        ]
        .contains(&key(kind).as_str());
    let native = fallback
        || capability
            .and_then(|c| c.get("native_clipboard_image_data"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    if all_images && device.is_local() && native {
        return AttachmentAction::NativeClipboard;
    }
    let supports = |field: &str| {
        fallback
            || capability
                .and_then(|c| c.get(field))
                .and_then(|v| v.as_str())
                == Some("shell_quoted")
    };
    if supports("file_path") || (all_images && supports("image_path")) {
        AttachmentAction::DevicePaths
    } else {
        AttachmentAction::Unsupported
    }
}
/// Returns text for the terminal composer, never submits a command.
pub fn attachment_text(device: &Device, paths: &[PathBuf], cancel: &CancelToken) -> Result<String> {
    if paths.is_empty() {
        bail!("未选择附件")
    }
    if !device.is_local() && device.ssh_target().is_none() {
        bail!("Tailcat暂不支持附件上传，请改用SSH")
    }
    // Validate the complete selection before starting any upload.
    for path in paths {
        let m = fs::symlink_metadata(path)?;
        if !m.is_file() || m.len() > 50 * 1024 * 1024 {
            bail!("附件必须为不超过50MB的普通文件")
        }
    }
    let mut delivered = vec![];
    for path in paths {
        cancel.check()?;
        let remote = if device.is_local() {
            fs::canonicalize(path)?.to_string_lossy().into_owned()
        } else {
            upload_attachment(device, path, cancel)?
        };
        delivered.push(quote(&remote)?);
    }
    Ok(delivered.join(" ") + " ")
}
fn upload_attachment(device: &Device, path: &Path, cancel: &CancelToken) -> Result<String> {
    let size = fs::metadata(path)?.len();
    let generated = temporary(Path::new("attachment"))?
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace(".attachment.herdrm-", "")
        .replace(".part", "");
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let suffix = if !extension.is_empty()
        && extension.len() <= 16
        && extension.bytes().all(|c| c.is_ascii_alphanumeric())
    {
        format!(".{extension}")
    } else {
        String::new()
    };
    let name = format!("{generated}{suffix}");
    let script = format!(
        "umask 077\ndir=\"${{XDG_CACHE_HOME:-$HOME/.cache}}/herdrm/attachments\"\nmkdir -p \"$dir\" && chmod 700 \"$dir\" || exit 1\nfind \"$dir\" -type f -mtime +7 -delete 2>/dev/null || true\ntmp=\"$dir/.{name}.part\"\ndest=\"$dir/{name}\"\ntrap 'rm -f \"$tmp\"' EXIT HUP INT TERM\nif (set -C; cat > \"$tmp\") && chmod 600 \"$tmp\" && ln \"$tmp\" \"$dest\" && rm -f \"$tmp\"; then trap - EXIT HUP INT TERM; printf '%s\\n' \"$dest\"; else exit 1; fi"
    );
    let (mut cmd, _auth) = ssh(device, &script)?;
    let mut child = cmd
        .stdin(Stdio::from(File::open(path)?))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut v = vec![];
        out.read_to_end(&mut v).map(|_| v)
    });
    let stderr = std::thread::spawn(move || read_diagnostics(err));
    let start = Instant::now();
    let deadline = Duration::from_secs((30 + size / 262144).min(600));
    let mut interrupted = false;
    let status = loop {
        if cancel.is_cancelled() || start.elapsed() > deadline {
            interrupted = true;
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(s) = child.try_wait()? {
            break s;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("附件输出读取失败"))??;
    let errors = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("附件错误读取失败"))??;
    if interrupted {
        bail!("附件上传已取消或超时，远端结果可能未知")
    }
    if !status.success() {
        bail!("附件上传失败：{}", String::from_utf8_lossy(&errors))
    }
    let output = String::from_utf8(output)?;
    let remote = output.lines().last().context("远端未返回附件路径")?;
    if !remote.starts_with('/')
        || !remote.ends_with(&format!("/herdrm/attachments/{name}"))
        || remote.contains('\0')
    {
        bail!("远端返回的附件路径无效")
    }
    Ok(remote.into())
}
