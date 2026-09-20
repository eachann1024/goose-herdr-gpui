use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Invalid(String),
    Rpc { code: String, message: String },
    UnknownOutcome(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Rpc { code, message } => write!(f, "{code}: {message}"),
            Self::UnknownOutcome(s) => write!(f, "结果未知：{s}；请刷新状态，不要重复提交"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}
pub fn validate(value: &str) -> Result<()> {
    if value.is_empty() || value.contains('\0') {
        Err(Error::Invalid("值不能为空或含 NUL".into()))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    #[serde(
        rename = "socketPath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub socket_path: Option<String>,
    #[serde(rename = "osID", default, skip_serializing_if = "Option::is_none")]
    pub os_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Local,
    Ssh { target: String },
    Tailcat,
}
impl Serialize for DeviceKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Local => json!({"local":{}}),
            Self::Ssh { target } => json!({"ssh":{"target":target}}),
            Self::Tailcat => json!({"tailcat":{}}),
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for DeviceKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if v.as_object().map(|o| o.len()) != Some(1) {
            return Err(serde::de::Error::custom("invalid Device.Kind"));
        }
        if v.get("local").is_some() {
            Ok(Self::Local)
        } else if v.get("tailcat").is_some() {
            Ok(Self::Tailcat)
        } else if let Some(t) = v.pointer("/ssh/target").and_then(Value::as_str) {
            Ok(Self::Ssh { target: t.into() })
        } else {
            Err(serde::de::Error::custom("invalid Device.Kind"))
        }
    }
}
impl Device {
    pub fn local() -> Self {
        Self {
            id: "00000000-0000-0000-0000-000000000001".into(),
            name: "Local".into(),
            kind: DeviceKind::Local,
            socket_path: None,
            os_id: Some("macos".into()),
        }
    }
    pub fn is_local(&self) -> bool {
        matches!(self.kind, DeviceKind::Local)
    }
    pub fn is_named_session(&self) -> bool {
        self.is_local() && self.socket_path.is_some() && self.id != Self::local().id
    }
    pub fn ssh_target(&self) -> Option<&str> {
        if let DeviceKind::Ssh { target } = &self.kind {
            Some(target)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub number: u64,
    pub cwd: Option<String>,
    pub agent_status: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tab {
    pub tab_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(
        default,
        rename = "custom_label",
        alias = "customLabel",
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_label: Option<String>,
    pub number: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pane {
    pub pane_id: String,
    pub terminal_id: Option<String>,
    pub workspace_id: String,
    pub tab_id: Option<String>,
    pub agent: Option<String>,
    pub agent_status: Option<String>,
    pub terminal_title: Option<String>,
    pub cwd: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Agent {
    pub pane_id: String,
    pub terminal_id: Option<String>,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: Option<String>,
    pub agent_status: Option<String>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub terminal_title: Option<String>,
    pub cwd: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    #[serde(default, deserialize_with = "null_vec")]
    pub tabs: Vec<Tab>,
    #[serde(default, deserialize_with = "null_vec")]
    pub panes: Vec<Pane>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
fn null_vec<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}
#[derive(Clone, Debug, Deserialize)]
pub struct Ping {
    pub version: String,
    pub protocol: u32,
}
#[derive(Clone, Debug)]
pub struct Client {
    pub socket_path: PathBuf,
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
// ponytail: cap one wire frame at 32 MiB; raise only with daemon contract evidence.
const MAX_LINE: u64 = 32 * 1024 * 1024;
fn line(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    let n = reader.take(MAX_LINE + 1).read_until(b'\n', &mut b)?;
    if n == 0 {
        return Err(Error::Invalid("连接结束，未收到响应".into()));
    }
    if n as u64 > MAX_LINE || b.last() != Some(&b'\n') {
        return Err(Error::Invalid("响应超长或不完整".into()));
    }
    Ok(b)
}
fn decode(b: &[u8]) -> Result<Value> {
    let v: Value = serde_json::from_slice(b)?;
    if let Some(e) = v.get("error") {
        return Err(Error::Rpc {
            code: e["code"].as_str().unwrap_or("unknown").into(),
            message: e["message"].as_str().unwrap_or("unknown error").into(),
        });
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| Error::Invalid("响应缺少 result/error".into()))
}
impl Client {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.call(method, params, false)
    }
    pub fn mutation(&self, method: &str, params: Value) -> Result<Value> {
        self.call(method, params, true)
    }
    fn call(&self, method: &str, params: Value, mutating: bool) -> Result<Value> {
        validate(method)?;
        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        let mut b = serde_json::to_vec(
            &json!({"id":format!("gpui-{}",NEXT_ID.fetch_add(1,Ordering::Relaxed)),"method":method,"params":params}),
        )?;
        b.push(b'\n');
        let result = (|| {
            stream.write_all(&b)?;
            decode(&line(&mut BufReader::new(stream))?)
        })();
        match result {
            Err(e @ Error::Rpc { .. }) => Err(e),
            Err(e) if mutating => Err(Error::UnknownOutcome(e.to_string())),
            r => r,
        }
    }
    pub fn ping(&self) -> Result<Ping> {
        let p: Ping = serde_json::from_value(self.request("ping", json!({}))?)?;
        if p.protocol < 17 {
            return Err(Error::Invalid(format!(
                "Herdr 协议 {} 低于最低版本 17",
                p.protocol
            )));
        }
        Ok(p)
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(serde_json::from_value(
            self.request("session.snapshot", json!({}))?["snapshot"].clone(),
        )?)
    }
    pub fn agents(&self) -> Result<Value> {
        self.request("agent.list", json!({}))
    }
    pub fn workspaces(&self) -> Result<Value> {
        self.request("workspace.list", json!({}))
    }
    pub fn agent_manifests(&self) -> Result<Value> {
        self.request("server.agent_manifests", json!({}))
    }
    pub fn create_workspace(&self, label: Option<&str>, cwd: Option<&str>) -> Result<Value> {
        let mut p = json!({"focus":false});
        if let Some(s) = label {
            validate(s)?;
            p["label"] = s.into();
        }
        if let Some(s) = cwd {
            validate(s)?;
            p["cwd"] = s.into();
        }
        self.mutation("workspace.create", p)
    }
    pub fn create_tab(
        &self,
        workspace: Option<&str>,
        cwd: Option<&str>,
        label: Option<&str>,
    ) -> Result<Value> {
        let mut p = json!({"focus":false});
        for (k, v) in [("workspace_id", workspace), ("cwd", cwd), ("label", label)] {
            if let Some(v) = v {
                validate(v)?;
                p[k] = v.into();
            }
        }
        self.mutation("tab.create", p)
    }
    pub fn rename_workspace(&self, id: &str, label: &str) -> Result<Value> {
        validate(id)?;
        validate(label)?;
        self.mutation("workspace.rename", json!({"workspace_id":id,"label":label}))
    }
    pub fn rename_tab(&self, id: &str, label: &str) -> Result<Value> {
        validate(id)?;
        validate(label)?;
        self.mutation("tab.rename", json!({"tab_id":id,"label":label}))
    }
    pub fn rename_agent(&self, target: &str, name: &str) -> Result<Value> {
        validate(target)?;
        if name.len() > 32
            || !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            || !name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
        {
            return Err(Error::Invalid(
                "Agent 名称须为小写字母开头的 1–32 位字母、数字、_ 或 -".into(),
            ));
        }
        self.mutation("agent.rename", json!({"target":target,"name":name}))
    }
    pub fn move_tab(&self, id: &str, index: usize) -> Result<Value> {
        validate(id)?;
        self.mutation("tab.move", json!({"tab_id":id,"insert_index":index}))
    }
    pub fn move_workspaces(&self, ids: &[String], before: Option<&str>) -> Result<Value> {
        for id in ids {
            validate(id)?;
        }
        let mut p = json!({"workspace_ids":ids});
        if let Some(b) = before {
            validate(b)?;
            p["before_workspace_id"] = b.into();
        }
        self.mutation("workspace.move_block", p)
    }
    pub fn start_agent(
        &self,
        name: &str,
        kind: &str,
        pane: &str,
        args: &[String],
    ) -> Result<Value> {
        for s in [name, kind, pane] {
            validate(s)?;
        }
        for s in args {
            if s.contains('\0') {
                return Err(Error::Invalid("参数包含 NUL".into()));
            }
        }
        self.mutation(
            "agent.start",
            json!({"name":name,"kind":kind,"pane_id":pane,"args":args}),
        )
    }
    pub fn wait_for_started_agent(
        &self,
        kind: &str,
        pane: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<bool> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
            let catalog = self.agents().unwrap_or(Value::Null);
            let info = catalog["agents"]
                .as_array()
                .and_then(|a| a.iter().find(|a| a["pane_id"] == pane));
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            if info.is_some_and(|i| i["interactive_ready"] == true) {
                return Ok(true);
            }
            if kind != "pi"
                && info.is_some_and(|i| i["launch_pending"] == false && i["agent"].is_string())
            {
                return Ok(true);
            }
            if !info.is_some_and(|i| i["interactive_ready"].is_boolean()) {
                if let Ok(screen) = self.read_pane(pane) {
                    if let Some(text) = screen.pointer("/read/text").and_then(Value::as_str) {
                        if looks_started(text, kind) {
                            return Ok(!cancel.load(Ordering::Relaxed));
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Ok(false)
    }
    pub fn start_agent_when_ready(
        &self,
        name: &str,
        kind: &str,
        pane: &str,
        args: &[String],
    ) -> Result<Value> {
        let pinned = self.get_pane(pane).unwrap_or(Value::Null);
        let identity = pinned
            .pointer("/pane/terminal_id")
            .or_else(|| pinned.get("terminal_id"))
            .cloned();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match self.start_agent(name, kind, pane, args) {
                Err(Error::Rpc { code, message })
                    if code == "agent_pane_busy"
                        && identity.is_some()
                        && std::time::Instant::now() < deadline =>
                {
                    let current = self.get_pane(pane).unwrap_or(Value::Null);
                    let id = current
                        .pointer("/pane/terminal_id")
                        .or_else(|| current.get("terminal_id"));
                    if id != identity.as_ref() {
                        return Err(Error::Rpc { code, message });
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                r => return r,
            }
        }
    }
    pub fn prompt(&self, target: &str, text: &str) -> Result<Value> {
        validate(target)?;
        validate(text)?;
        self.mutation("agent.prompt", json!({"target":target,"text":text}))
    }
    pub fn read_pane(&self, id: &str) -> Result<Value> {
        validate(id)?;
        self.request(
            "pane.read",
            json!({"pane_id":id,"source":"visible","format":"ansi"}),
        )
    }
    pub fn get_pane(&self, id: &str) -> Result<Value> {
        validate(id)?;
        self.request("pane.get", json!({"pane_id":id}))
    }
    pub fn send_input(&self, id: &str, text: &str) -> Result<Value> {
        validate(id)?;
        validate(text)?;
        self.mutation("pane.send_input", json!({"pane_id":id,"text":text}))
    }
    pub fn send_keys(&self, id: &str, keys: &[String]) -> Result<Value> {
        validate(id)?;
        self.mutation("pane.send_input", json!({"pane_id":id,"keys":keys}))
    }
    pub fn close_pane(&self, id: &str) -> Result<Value> {
        validate(id)?;
        self.mutation("pane.close", json!({"pane_id":id}))
    }
    pub fn close_workspace(&self, id: &str) -> Result<Value> {
        validate(id)?;
        self.mutation("workspace.close", json!({"workspace_id":id}))
    }
    pub fn events(&self, pane_ids: &[String]) -> Result<EventStream> {
        let mut ids = pane_ids.to_vec();
        ids.sort();
        ids.dedup();
        loop {
            let mut stream = UnixStream::connect(&self.socket_path)?;
            stream.set_read_timeout(Some(Duration::from_secs(15)))?;
            let mut subs: Vec<Value> = EVENTS.iter().map(|t| json!({"type":t})).collect();
            subs.extend(
                ids.iter()
                    .map(|id| json!({"type":"pane.agent_status_changed","pane_id":id})),
            );
            let mut b = serde_json::to_vec(
                &json!({"id":"events","method":"events.subscribe","params":{"subscriptions":subs}}),
            )?;
            b.push(b'\n');
            stream.write_all(&b)?;
            let cancel = stream.try_clone()?;
            let mut reader = BufReader::new(stream);
            match decode(&line(&mut reader)?) {
                Err(Error::Rpc { code, .. }) if code == "pane_not_found" && !ids.is_empty() => {
                    ids.clear();
                    continue;
                }
                Err(e) => return Err(e),
                Ok(_) => {
                    reader.get_ref().set_read_timeout(None)?;
                    return Ok(EventStream {
                        reader,
                        cancel,
                        started: false,
                    });
                }
            }
        }
    }
}
fn looks_started(text: &str, kind: &str) -> bool {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            plain.push(c);
        }
    }
    let lines: Vec<_> = plain
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let command = |s: &str| s.trim_start_matches(['❯', '➜', '$', '%']).trim().to_owned();
    if lines
        .last()
        .is_some_and(|s| command(s).eq_ignore_ascii_case(kind))
    {
        return false;
    }
    lines.iter().any(|s| {
        let c = command(s);
        !c.is_empty() && !c.starts_with('/') && !c.starts_with('~')
    })
}
const EVENTS: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.metadata_updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.focused",
    "workspace.closed",
    "worktree.created",
    "worktree.opened",
    "worktree.removed",
    "tab.created",
    "tab.renamed",
    "tab.moved",
    "tab.focused",
    "tab.closed",
    "pane.created",
    "pane.updated",
    "pane.moved",
    "pane.focused",
    "pane.closed",
    "pane.exited",
    "pane.agent_detected",
    "layout.updated",
];
pub fn event_kind(event: &Value) -> String {
    let raw = event["event"]
        .as_str()
        .or_else(|| event.pointer("/event/type").and_then(Value::as_str))
        .or_else(|| event["type"].as_str())
        .or_else(|| event["kind"].as_str())
        .unwrap_or("unknown");
    for kind in EVENTS.iter().copied().chain([
        "pane.agent_status_changed",
        "pane.scroll_changed",
        "pane.output_matched",
    ]) {
        if kind.replace('.', "_") == raw {
            return kind.into();
        }
    }
    raw.into()
}
pub struct EventStream {
    started: bool,
    reader: BufReader<UnixStream>,
    cancel: UnixStream,
}
impl EventStream {
    pub fn cancellation_handle(&self) -> Result<UnixStream> {
        Ok(self.cancel.try_clone()?)
    }
    pub fn next(&mut self) -> Result<Value> {
        if !self.started {
            self.started = true;
            return Ok(json!({"event":"subscription.started"}));
        }
        loop {
            let b = line(&mut self.reader)?;
            if b.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            return Ok(serde_json::from_slice(&b)?);
        }
    }
}
impl Drop for EventStream {
    fn drop(&mut self) {
        let _ = self.cancel.shutdown(std::net::Shutdown::Both);
    }
}
fn named_session_id(name: &str) -> String {
    fn fnv(seed: u64, s: &str) -> u64 {
        s.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
        })
    }
    let low = fnv(14_695_981_039_346_656_037, &format!("herdr-session:{name}"));
    let high = fnv(low ^ 0x9E37_79B9_7F4A_7C15, name);
    let h = format!("{high:016X}{low:016X}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}
pub fn discover_named_sessions(home: &Path) -> Result<Vec<Device>> {
    let root = home.join(".config/herdr/sessions");
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut devices = Vec::new();
    for e in std::fs::read_dir(root)? {
        let e = e?;
        let socket = e.path().join("herdr.sock");
        use std::os::unix::fs::FileTypeExt;
        if std::fs::metadata(&socket).is_ok_and(|m| m.file_type().is_socket()) {
            let name = e.file_name().to_string_lossy().into_owned();
            devices.push(Device {
                id: named_session_id(&name),
                name,
                kind: DeviceKind::Local,
                socket_path: Some(socket.to_string_lossy().into_owned()),
                os_id: Some("macos".into()),
            });
        }
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn swift_device_roundtrip() {
        let v = json!({"id":"id","name":"服务器","kind":{"ssh":{"target":"me@host"}},"socketPath":"/a"});
        let d: Device = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(d.ssh_target(), Some("me@host"));
        assert_eq!(serde_json::to_value(d).unwrap(), v);
    }
    #[test]
    fn readiness_and_optional_snapshot() {
        assert!(!looks_started("\u{1b}[32m❯ pi\u{1b}[0m", "pi"));
        assert!(!looks_started("~\n❯", "pi"));
        assert!(looks_started("Pi coding agent\nSelect a model", "pi"));
        let snapshot: Snapshot =
            serde_json::from_value(json!({"agents":[],"workspaces":[],"tabs":null,"panes":null}))
                .unwrap();
        assert!(snapshot.tabs.is_empty() && snapshot.panes.is_empty());
    }
    #[test]
    fn ack_keeps_buffer() {
        let mut r = BufReader::new(&b"{\"result\":{}}\n{\"event\":\"pane.closed\"}\n"[..]);
        assert!(decode(&line(&mut r).unwrap()).is_ok());
        assert_eq!(
            serde_json::from_slice::<Value>(&line(&mut r).unwrap()).unwrap()["event"],
            "pane.closed"
        );
    }
}
