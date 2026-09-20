use crate::{
    herdr::{Client, Device, DeviceKind, Error, Result, validate},
    terminal::CommandSpec,
};
use std::{
    ffi::OsString,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| Error::Invalid("无法确定用户主目录".into()))
}
pub(crate) fn destination(target: &str) -> Result<String> {
    validate(target)?;
    let invalid = || Error::Invalid("无效 SSH 目标".into());
    if target.starts_with('-') || target.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid());
    }
    let authority = target.strip_prefix("ssh://").unwrap_or(target);
    if authority.contains(['/', '?', '#']) {
        return Err(invalid());
    }
    let host = if let Some((user, host)) = authority.rsplit_once('@') {
        if user.is_empty() || user.contains('@') {
            return Err(invalid());
        }
        host
    } else {
        authority
    };
    let port = if host.starts_with('[') {
        let (ip, suffix) = host[1..].split_once(']').ok_or_else(invalid)?;
        ip.parse::<std::net::Ipv6Addr>().map_err(|_| invalid())?;
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':').ok_or_else(invalid)?)
        }
    } else if host.matches(':').count() > 1 {
        host.parse::<std::net::Ipv6Addr>().map_err(|_| invalid())?;
        if target.starts_with("ssh://") {
            return Err(invalid());
        }
        None
    } else {
        let (name, port) = host
            .split_once(':')
            .map_or((host, None), |(h, p)| (h, Some(p)));
        if name.is_empty() || name.starts_with('-') || name.contains(['[', ']']) {
            return Err(invalid());
        }
        port
    };
    if let Some(port) = port {
        if !port.bytes().all(|c| c.is_ascii_digit()) || !port.parse::<u16>().is_ok_and(|p| p > 0) {
            return Err(invalid());
        }
        if !target.starts_with("ssh://") {
            return Ok(format!("ssh://{target}"));
        }
    }
    Ok(target.to_owned())
}

fn map_err(e: impl std::fmt::Display) -> Error {
    Error::Invalid(e.to_string())
}
struct Resources {
    child: Mutex<Option<Child>>,
    directory: Option<PathBuf>,
    _authentication: Option<crate::credentials::SshAuthentication>,
    _tailcat: Option<crate::tailcat::TailcatBridge>,
}
impl Drop for Resources {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            if let Some(mut p) = child.take() {
                let _ = p.kill();
                let _ = p.wait();
            }
        }
        if let Some(d) = &self.directory {
            let _ = fs::remove_dir_all(d);
        }
    }
}
#[derive(Clone)]
pub struct Connection {
    pub client: Client,
    pub version: String,
    _resources: Arc<Resources>,
}
#[derive(Clone, Debug)]
pub enum AttachTarget {
    Agent(String),
    Terminal(String),
}
const REMOTE_PATH: &str = "for d in \"$HOME\"/.nvm/versions/node/*/bin; do [ -d \"$d\" ] && PATH=\"$d:$PATH\"; done; export PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.grok/bin:/opt/homebrew/bin:/usr/local/bin:$PATH\"";
// Set only for a new process; an existing Pi keeps the environment it started with.
const REMOTE_PI_IMAGE_PROTOCOL: &str =
    "if [ \"${PI_IMAGE_PROTOCOL+x}\" != x ]; then export PI_IMAGE_PROTOCOL=kitty; fi";
fn ssh_base(target: &str, auth: &crate::credentials::SshAuthentication) -> Result<Command> {
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
        .arg(destination(target)?)
        .envs(auth.environment.iter().cloned());
    Ok(c)
}
fn output_timeout(mut cmd: Command, timeout: Duration) -> Result<String> {
    let mut p = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut out = p
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("未建立输出管道".into()))?;
    let read = thread::spawn(move || {
        let mut b = Vec::new();
        out.by_ref()
            .take(1024 * 1024)
            .read_to_end(&mut b)
            .map(|_| b)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = p.try_wait()? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = p.kill();
            let _ = p.wait();
            return Err(Error::Invalid("SSH 请求超时".into()));
        }
        thread::sleep(Duration::from_millis(30));
    };
    let bytes = read
        .join()
        .map_err(|_| Error::Invalid("SSH 输出线程结束".into()))??;
    if !status.success() {
        return Err(Error::Invalid(format!(
            "SSH 失败（{status}）；请检查认证、主机密钥和远端 Herdr"
        )));
    }
    String::from_utf8(bytes).map_err(map_err)
}
pub fn connect(device: &Device) -> Result<Connection> {
    let mut resources = Resources {
        child: Mutex::new(None),
        directory: None,
        _authentication: None,
        _tailcat: None,
    };
    let socket = match &device.kind {
        DeviceKind::Local => device
            .socket_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or(home()?.join(".config/herdr/herdr.sock")),
        DeviceKind::Ssh { target } => {
            let auth = crate::credentials::ssh_authentication(&device.id).map_err(map_err)?;
            let mut probe = ssh_base(target, &auth)?;
            probe.arg("printf '%s' \"$HOME\"");
            let remote_home = output_timeout(probe, Duration::from_secs(15))?;
            if !remote_home.starts_with('/') || remote_home.contains('\n') {
                return Err(Error::Invalid("SSH 返回无效主目录".into()));
            }
            drop(auth);
            let auth = crate::credentials::ssh_authentication(&device.id).map_err(map_err)?;
            let remote = device
                .socket_path
                .clone()
                .unwrap_or(format!("{remote_home}/.config/herdr/herdr.sock"));
            validate(&remote)?;
            if remote.contains(':') {
                return Err(Error::Invalid("SSH socket 路径不能含冒号".into()));
            }
            let dir = PathBuf::from(format!(
                "/tmp/ghg-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir)?;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
            resources.directory = Some(dir.clone());
            let local = dir.join("s");
            let mut c = Command::new("/usr/bin/ssh");
            c.arg("-N")
                .args(&auth.arguments)
                .args([
                    "-o",
                    "StrictHostKeyChecking=accept-new",
                    "-o",
                    "ConnectTimeout=10",
                    "-o",
                    "ExitOnForwardFailure=yes",
                    "-o",
                    "StreamLocalBindUnlink=yes",
                    "-o",
                    "ServerAliveInterval=15",
                    "-o",
                    "ServerAliveCountMax=3",
                    "-L",
                ])
                .arg(format!("{}:{remote}", local.display()))
                .arg(destination(target)?)
                .envs(auth.environment.iter().cloned())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            let mut child = c.spawn()?;
            let stderr = child.stderr.take();
            *resources.child.get_mut().unwrap() = Some(child);
            resources._authentication = Some(auth);
            let mut diagnostics = stderr.map(|err| {
                thread::spawn(move || {
                    let mut bytes = Vec::new();
                    let _ = err.take(16 * 1024).read_to_end(&mut bytes);
                    bytes
                })
            });
            let start = Instant::now();
            loop {
                if UnixStream::connect(&local).is_ok() {
                    break;
                }
                if let Some(p) = resources.child.get_mut().unwrap() {
                    if let Some(status) = p.try_wait()? {
                        let detail = diagnostics
                            .take()
                            .and_then(|t| t.join().ok())
                            .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_default();
                        return Err(Error::Invalid(if detail.is_empty() {
                            format!("SSH 转发退出：{status}")
                        } else {
                            format!("SSH 转发退出：{status}：{detail}")
                        }));
                    }
                }
                if start.elapsed() > Duration::from_secs(12) {
                    let detail = diagnostics
                        .take()
                        .and_then(|t| t.join().ok())
                        .map(|b| String::from_utf8_lossy(&b).trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_default();
                    return Err(Error::Invalid(if detail.is_empty() {
                        "SSH socket 转发超时".into()
                    } else {
                        format!("SSH socket 转发超时：{detail}")
                    }));
                }
                thread::sleep(Duration::from_millis(50));
            }
            local
        }
        DeviceKind::Tailcat => {
            let token =
                crate::credentials::get(crate::credentials::SecretKind::TailcatToken, &device.id)
                    .map_err(map_err)?
                    .ok_or_else(|| Error::Invalid("尚未设置 Tailcat token".into()))?;
            let bridge = crate::tailcat::TailcatBridge::connect(&token).map_err(map_err)?;
            let socket = bridge.socket_path().to_path_buf();
            resources._tailcat = Some(bridge);
            socket
        }
    };
    let client = Client::new(socket);
    let ping = client.ping()?;
    Ok(Connection {
        client,
        version: ping.version,
        _resources: Arc::new(resources),
    })
}
fn binary_selection(version: &str) -> Result<String> {
    if version.is_empty() || !version.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(Error::Invalid("服务端版本不可用于安全匹配 CLI".into()));
    }
    Ok(format!(
        "hb=''; oldifs=$IFS; IFS=:; for d in $PATH; do c=\"$d/herdr\"; [ -x \"$c\" ] || continue; [ \"$(\"$c\" --version 2>/dev/null | awk '{{print $NF}}')\" = '{version}' ] && {{ hb=\"$c\"; break; }}; done; IFS=$oldifs; [ -n \"$hb\" ] || {{ printf '%s\\n' '找不到与服务端版本 {version} 匹配的 Herdr CLI' >&2; exit 1; }}"
    ))
}
fn spec(program: &str, args: Vec<String>) -> CommandSpec {
    CommandSpec {
        program: program.into(),
        args: args.into_iter().map(OsString::from).collect(),
        cwd: None,
        env: vec![],
        authentication: None,
    }
}
pub fn attach_command(
    device: &Device,
    connection: &Connection,
    target: AttachTarget,
    takeover: bool,
) -> Result<CommandSpec> {
    let (kind, id) = match target {
        AttachTarget::Agent(id) => ("agent", id),
        AttachTarget::Terminal(id) => ("terminal", id),
    };
    validate(&id)?;
    let script = format!(
        "{}; exec \"$hb\" {kind} attach {}{}",
        binary_selection(&connection.version)?,
        shell_quote(&id),
        if takeover { " --takeover" } else { "" }
    );
    match &device.kind {
        DeviceKind::Local | DeviceKind::Tailcat => {
            let mut s = spec("/bin/sh", vec!["-c".into(), script]);
            // Herdr recognizes this Kitty-compatible profile for its direct-graphics bridge.
            // Ordinary shells retain xterm-256color and our own TERM_PROGRAM name.
            s.env.push(("TERM".into(), "xterm-kitty".into()));
            // Discovery and execution must share PATH, including login-shell shims.
            let path = std::env::join_paths(crate::app::app_catalog::search_directories())
                .map_err(map_err)?;
            s.env.push(("PATH".into(), path));
            s.env.push((
                "HERDR_SOCKET_PATH".into(),
                connection.client.socket_path.as_os_str().into(),
            ));
            Ok(s)
        }
        DeviceKind::Ssh { target } => {
            let mut s = spec(
                "/usr/bin/ssh",
                vec![
                    "-tt".into(),
                    "-o".into(),
                    "StrictHostKeyChecking=accept-new".into(),
                    "-o".into(),
                    "ConnectTimeout=10".into(),
                    "-o".into(),
                    "ServerAliveInterval=15".into(),
                    "-o".into(),
                    "ServerAliveCountMax=3".into(),
                    destination(target)?.into(),
                    format!(
                        "exec /bin/sh -c {}",
                        shell_quote(&format!(
                            "{REMOTE_PATH}; {REMOTE_PI_IMAGE_PROTOCOL}; {}; {script}",
                            device
                                .socket_path
                                .as_ref()
                                .map(|p| format!("export HERDR_SOCKET_PATH={}", shell_quote(p)))
                                .unwrap_or_else(|| ":".into())
                        ))
                    ),
                ],
            );
            let auth =
                Arc::new(crate::credentials::ssh_authentication(&device.id).map_err(map_err)?);
            let mut args: Vec<OsString> = auth.arguments.iter().map(OsString::from).collect();
            args.extend(s.args);
            s.args = args;
            s.env
                .extend(auth.environment.iter().map(|(k, v)| (k.into(), v.into())));
            s.authentication = Some(auth);
            Ok(s)
        }
    }
}
pub fn shell_command(device: &Device, cwd: Option<&Path>) -> Result<CommandSpec> {
    match &device.kind {
        DeviceKind::Tailcat => Err(Error::Invalid(
            "Tailcat 仅提供 Herdr socket，独立终端需要 SSH".into(),
        )),
        DeviceKind::Local => {
            let prelude = if let Some(cwd) = cwd {
                let s = cwd
                    .to_str()
                    .ok_or_else(|| Error::Invalid("目录不是 UTF-8".into()))?;
                validate(s)?;
                format!("cd {} 2>/dev/null || cd \"$HOME\"", shell_quote(s))
            } else {
                "cd \"$HOME\"".into()
            };
            Ok(spec(
                "/bin/sh",
                vec![
                    "-c".into(),
                    format!("{prelude}; exec \"${{SHELL:-/bin/zsh}}\" -l"),
                ],
            ))
        }
        DeviceKind::Ssh { target } => {
            let auth =
                Arc::new(crate::credentials::ssh_authentication(&device.id).map_err(map_err)?);
            let mut args = auth.arguments.clone();
            args.extend([
                "-tt".into(),
                "-o".into(),
                "StrictHostKeyChecking=accept-new".into(),
                "-o".into(),
                "ConnectTimeout=10".into(),
                "-o".into(),
                "ServerAliveInterval=15".into(),
                destination(target)?.into(),
                format!(
                    "exec /bin/sh -c {}",
                    shell_quote(&format!(
                        "{REMOTE_PI_IMAGE_PROTOCOL}; exec \"${{SHELL:-/bin/zsh}}\" -l"
                    ))
                ),
            ]);
            let mut s = spec("/usr/bin/ssh", args);
            s.env
                .extend(auth.environment.iter().map(|(k, v)| (k.into(), v.into())));
            s.authentication = Some(auth);
            Ok(s)
        }
    }
}
/// Explicit user action only. Never called by reconnect; never owns or kills the daemon.
pub fn start_local_server(device: &Device, binary: &Path) -> Result<()> {
    if !device.is_local() {
        return Err(Error::Invalid("只能启动本机 Herdr".into()));
    }
    if !binary.is_absolute() || !binary.is_file() {
        return Err(Error::Invalid("请选择有效的 Herdr CLI 绝对路径".into()));
    }
    let socket = device
        .socket_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(home()?.join(".config/herdr/herdr.sock"));
    if UnixStream::connect(&socket).is_ok() {
        return Ok(());
    }
    if socket.exists() {
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(50));
            match UnixStream::connect(&socket) {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    let mut c = Command::new(binary);
    c.arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, _) in std::env::vars_os() {
        let key = k.to_string_lossy();
        if key.starts_with("DYLD_")
            || [
                "__CFBundleIdentifier",
                "XPC_SERVICE_NAME",
                "XPC_FLAGS",
                "OSLogRateLimit",
                "MallocNanoZone",
                "OS_ACTIVITY_DT_MODE",
            ]
            .contains(&key.as_ref())
        {
            c.env_remove(k);
        }
    }
    c.env("HERDR_SOCKET_PATH", &socket);
    if std::env::var_os("PI_IMAGE_PROTOCOL").is_none() {
        c.env("PI_IMAGE_PROTOCOL", "kitty");
    }
    if std::env::var_os("SHELL").is_none() {
        c.env("SHELL", "/bin/zsh");
    }
    let mut child = c.spawn()?;
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    let start = Instant::now();
    let mut exited = None;
    loop {
        if UnixStream::connect(&socket).is_ok() {
            return Ok(());
        }
        if exited.is_none() {
            if let Ok(status) = rx.try_recv() {
                exited = Some((Instant::now(), status));
            }
        }
        if let Some((at, status)) = &exited {
            if at.elapsed() > Duration::from_millis(500) {
                return Err(Error::Invalid(format!(
                    "Herdr server 已退出但 socket 未就绪：{status:?}"
                )));
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(Error::Invalid(
                "等待 Herdr socket 超时；共享 daemon 未被终止".into(),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn socket_snapshot_and_attach_use_discovered_path() {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixListener,
        };
        let dir = std::env::temp_dir().join(format!("ghg-wire-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("s");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            for (method, result) in [
                ("ping", serde_json::json!({"version":"0.7.4","protocol":17})),
                (
                    "session.snapshot",
                    serde_json::json!({"snapshot":{
                        "agents":[], "workspaces":[{"workspace_id":"w1","label":"工作","number":1}],
                        "tabs":[{"tab_id":"t1","workspace_id":"w1","label":"终端"}],
                        "panes":[{"pane_id":"p1","terminal_id":"term1","workspace_id":"w1","tab_id":"t1"}]
                    }}),
                ),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method);
                writeln!(
                    stream,
                    "{}",
                    serde_json::json!({"id":request["id"], "result":result})
                )
                .unwrap();
            }
        });
        let mut device = Device::local();
        device.socket_path = Some(socket.to_str().unwrap().into());
        let connection = connect(&device).unwrap();
        let snapshot = connection.client.snapshot().unwrap();
        assert_eq!(snapshot.panes[0].terminal_id.as_deref(), Some("term1"));
        let command = attach_command(
            &device,
            &connection,
            AttachTarget::Terminal("term1".into()),
            false,
        )
        .unwrap();
        assert!(
            command.args[1]
                .to_string_lossy()
                .contains("terminal attach 'term1'")
        );
        assert!(!command.args[1].to_string_lossy().contains("--takeover"));
        let takeover = attach_command(
            &device,
            &connection,
            AttachTarget::Terminal("term1".into()),
            true,
        )
        .unwrap();
        assert!(takeover.args[1].to_string_lossy().contains("--takeover"));
        let expected = std::env::join_paths(crate::app::app_catalog::search_directories()).unwrap();
        assert!(command.env.contains(&("PATH".into(), expected)));
        assert!(
            command
                .env
                .contains(&("HERDR_SOCKET_PATH".into(), socket.into_os_string()))
        );
        server.join().unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ssh_destinations_support_aliases_ports_and_ipv6() {
        for target in [
            "my-server",
            "user@host",
            "::1",
            "user@2001:db8::1",
            "ssh://user@host:22",
        ] {
            assert_eq!(destination(target).unwrap(), target);
        }
        assert_eq!(
            destination("user@host:2222").unwrap(),
            "ssh://user@host:2222"
        );
        assert_eq!(
            destination("user@[::1]:2222").unwrap(),
            "ssh://user@[::1]:2222"
        );
        for target in [
            "",
            "-oProxyCommand=x",
            "host:0",
            "host:65536",
            "host:no",
            "host:",
            "user@",
            "@host",
            "ssh://",
            "host path",
            "host\n",
            "[invalid]:22",
            "ssh://host/path",
        ] {
            assert!(destination(target).is_err(), "accepted {target:?}");
        }
    }
    #[test]
    fn remote_image_protocol_defaults_without_overriding_a_choice() {
        assert!(REMOTE_PI_IMAGE_PROTOCOL.contains("${PI_IMAGE_PROTOCOL+x}"));
        assert!(REMOTE_PI_IMAGE_PROTOCOL.contains("export PI_IMAGE_PROTOCOL=kitty"));
    }
    #[test]
    fn hostile_quote() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert!(destination("-oProxyCommand=x").is_err());
        assert!(binary_selection("1;rm").is_err());
    }
}
