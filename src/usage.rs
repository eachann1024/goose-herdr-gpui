//! Read-only usage probes. Never expose helper stderr or credential-bearing errors.
use anyhow::{Result, bail};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime},
};

pub const PROVIDERS: [&str; 6] = ["claude", "codex", "kimi", "grok", "opencode", "cursor"];
pub const CURSOR_AUTHORIZATION_KEY: &str = "usage.cursor.readOnlyAuthorized";
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used_percent: f64,
    pub window_minutes: f64,
    pub resets_at: Option<f64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub available_count: u64,
    pub total_earned_count: Option<u64>,
    pub next_expires_at: Option<f64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub sessions: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorSummary {
    pub auto_percent: Option<f64>,
    pub api_percent: Option<f64>,
    pub total_percent: Option<f64>,
    #[serde(rename = "planSpentUSD")]
    pub plan_spent_usd: Option<f64>,
    #[serde(rename = "includedSpentUSD")]
    pub included_spent_usd: Option<f64>,
    #[serde(rename = "planLimitUSD")]
    pub plan_limit_usd: Option<f64>,
    pub cycle_start: Option<f64>,
    pub cycle_end: Option<f64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub failure_kind: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub provider: String,
    pub status: String,
    pub session: Option<Window>,
    pub weekly: Option<Window>,
    pub monthly: Option<Window>,
    pub fable_weekly: Option<Window>,
    pub rate_limit_reset_credits: Option<Credits>,
    pub plan_type: Option<String>,
    pub cursor: Option<CursorSummary>,
    pub history: Option<History>,
    pub history_error: Option<bool>,
    pub usage_metadata: Option<Metadata>,
}
struct Completion {
    provider: String,
    generation: u64,
    snapshot: Option<UsageSnapshot>,
}
pub struct UsageService {
    pub snapshots: BTreeMap<String, UsageSnapshot>,
    pub failures: BTreeSet<String>,
    pub fetching: BTreeSet<String>,
    pub cursor_failure: Option<String>,
    pub cursor_updated_at: Option<SystemTime>,
    enabled: BTreeSet<String>,
    visible: BTreeSet<String>,
    generation: u64,
    processes: BTreeMap<String, Arc<Mutex<Child>>>,
    sender: mpsc::Sender<Completion>,
    receiver: mpsc::Receiver<Completion>,
    last_refresh: Instant,
}
impl Default for UsageService {
    fn default() -> Self {
        Self::new()
    }
}
impl UsageService {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            snapshots: BTreeMap::new(),
            failures: BTreeSet::new(),
            fetching: BTreeSet::new(),
            cursor_failure: None,
            cursor_updated_at: None,
            enabled: BTreeSet::new(),
            visible: BTreeSet::new(),
            generation: 0,
            processes: BTreeMap::new(),
            sender,
            receiver,
            last_refresh: Instant::now(),
        }
    }
    pub fn cursor_authorized() -> bool {
        crate::settings::read_preference(CURSOR_AUTHORIZATION_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
    pub fn set_cursor_authorized(&mut self, authorized: bool) -> Result<()> {
        crate::settings::write_preference(
            CURSOR_AUTHORIZATION_KEY,
            Some(&serde_json::Value::Bool(authorized)),
        )?;
        if !authorized {
            let mut enabled = self.enabled.clone();
            enabled.remove("cursor");
            self.replace(enabled);
        }
        Ok(())
    }
    /// Catalog is local, enabled agent kinds. Cursor is independent of CLI installation.
    pub fn configure(&mut self, catalog: &[String], disabled: &BTreeSet<String>) -> Result<()> {
        if disabled.contains("cursor") && Self::cursor_authorized() {
            self.set_cursor_authorized(false)?;
        }
        let mut visible: BTreeSet<_> = catalog
            .iter()
            .filter(|s| PROVIDERS.contains(&s.as_str()) && !disabled.contains(*s))
            .cloned()
            .collect();
        if Self::cursor_authorized() && !disabled.contains("cursor") {
            visible.insert("cursor".into());
        }
        let mut enabled = visible.clone();
        if !Self::cursor_authorized() {
            enabled.remove("cursor");
        }
        if visible != self.visible || enabled != self.enabled {
            self.replace(enabled);
            self.visible = visible;
            self.refresh();
        }
        Ok(())
    }
    pub fn shows(&self, provider: &str) -> bool {
        self.visible.contains(provider)
    }
    fn replace(&mut self, enabled: BTreeSet<String>) {
        self.generation = self.generation.wrapping_add(1);
        for child in self.processes.values() {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
            }
        }
        self.enabled = enabled;
        self.visible.clear();
        self.snapshots.clear();
        self.failures.clear();
        self.fetching.clear();
        self.cursor_failure = None;
        self.cursor_updated_at = None;
    }
    /// Called from the UI's background tick; performs no blocking network/credential reads.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(done) = self.receiver.try_recv() {
            self.processes.remove(&done.provider);
            if done.generation != self.generation
                || !self.enabled.contains(&done.provider)
                || (done.provider == "cursor" && !Self::cursor_authorized())
            {
                self.refresh();
                continue;
            }
            changed = true;
            self.fetching.remove(&done.provider);
            if let Some(snapshot) = done.snapshot {
                self.failures.remove(&done.provider);
                if done.provider == "cursor" && snapshot.status != "ok" {
                    self.cursor_failure = Some(
                        snapshot
                            .usage_metadata
                            .and_then(|m| m.failure_kind)
                            .unwrap_or_else(|| "request-failed".into()),
                    );
                } else {
                    if done.provider == "cursor" {
                        self.cursor_failure = None;
                        self.cursor_updated_at = Some(SystemTime::now());
                    }
                    self.snapshots.insert(done.provider, snapshot);
                }
            } else {
                if done.provider == "cursor" {
                    self.cursor_failure = Some("helper-unavailable".into());
                }
                self.failures.insert(done.provider);
            }
        }
        if self.last_refresh.elapsed() >= Duration::from_secs(300) {
            self.refresh();
            changed = true;
        }
        changed
    }
    pub fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        for provider in self.enabled.clone() {
            if self.processes.contains_key(&provider) {
                continue;
            }
            if provider == "cursor" && !Self::cursor_authorized() {
                continue;
            }
            match launch(&provider) {
                Ok((child, stdout)) => {
                    let child = Arc::new(Mutex::new(child));
                    self.processes.insert(provider.clone(), child.clone());
                    self.fetching.insert(provider.clone());
                    let sender = self.sender.clone();
                    let generation = self.generation;
                    thread::spawn(move || {
                        let reader = thread::spawn(move || {
                            let mut data = Vec::new();
                            stdout
                                .take(1024 * 1024 + 1)
                                .read_to_end(&mut data)
                                .map(|_| data)
                        });
                        let started = Instant::now();
                        let success = loop {
                            let Ok(mut process) = child.lock() else {
                                break false;
                            };
                            match process.try_wait() {
                                Ok(Some(status)) => break status.success(),
                                Err(_) => {
                                    let _ = process.kill();
                                    let _ = process.wait();
                                    break false;
                                }
                                _ => {}
                            }
                            if started.elapsed() > Duration::from_secs(60) {
                                let _ = process.kill();
                                let _ = process.wait();
                                break false;
                            }
                            drop(process);
                            thread::sleep(Duration::from_millis(100));
                        };
                        let snapshot = reader
                            .join()
                            .ok()
                            .and_then(|r| r.ok())
                            .filter(|b| success && b.len() <= 1024 * 1024)
                            .and_then(|data| parse_snapshot(&provider, &data).ok());
                        let _ = sender.send(Completion {
                            provider,
                            generation,
                            snapshot,
                        });
                    });
                }
                Err(_) => {
                    if provider == "cursor" {
                        self.cursor_failure = Some("helper-unavailable".into());
                    }
                    self.failures.insert(provider);
                }
            }
        }
    }
}
impl Drop for UsageService {
    fn drop(&mut self) {
        self.replace(BTreeSet::new());
    }
}
fn helper_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("GOOSE_USAGE_HELPER") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe()?;
    Ok(exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!(crate::i18n::tr("应用路径不可用")))?
        .join("../Resources/UsageHelper"))
}
fn launch(provider: &str) -> Result<(Child, std::process::ChildStdout)> {
    if !PROVIDERS.contains(&provider) {
        bail!(crate::i18n::tr("不支持的用量来源"));
    }
    if provider == "cursor" && !UsageService::cursor_authorized() {
        bail!(crate::i18n::tr("Cursor 用量尚未授权"));
    }
    let root = helper_root()?;
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    let mut command = Command::new(root.join("runtime").join(arch).join("node"));
    command
        .args(["--use-env-proxy"])
        .arg(root.join("main.mjs"))
        .arg(provider)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "en_US.UTF-8");
    for key in [
        "HOME",
        "KIMI_CODE_HOME",
        "GROK_HOME",
        "CODEX_HOME",
        "OPENCODE_DB",
        "XDG_DATA_HOME",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
    ] {
        if let Some(v) = std::env::var_os(key) {
            command.env(key, v);
        }
    }
    if provider == "cursor" {
        command.env("GOOSE_CURSOR_USAGE_AUTHORIZED", "1");
    }
    let mut child = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    Ok((child, stdout))
}
fn parse_snapshot(provider: &str, data: &[u8]) -> Result<UsageSnapshot> {
    let snapshot: UsageSnapshot = serde_json::from_slice(data)?;
    if snapshot.provider != provider {
        bail!(crate::i18n::tr("用量来源不匹配"));
    }
    for window in [
        &snapshot.session,
        &snapshot.weekly,
        &snapshot.monthly,
        &snapshot.fable_weekly,
    ]
    .into_iter()
    .flatten()
    {
        if !window.used_percent.is_finite()
            || window.used_percent < 0.0
            || !window.window_minutes.is_finite()
            || window.window_minutes < 0.0
        {
            bail!(crate::i18n::tr("无效的用量数据"));
        }
    }
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_boundary() {
        assert!(parse_snapshot("cursor", br#"{"provider":"codex","status":"ok"}"#).is_err());
        assert!(parse_snapshot("claude", br#"{"provider":"claude","status":"ok","session":{"usedPercent":-1,"windowMinutes":5}}"#).is_err());
        let unavailable = parse_snapshot("opencode", br#"{"provider":"opencode","status":"unavailable","history":{"sessions":1,"inputTokens":2,"outputTokens":3}}"#).unwrap();
        assert!(unavailable.session.is_none());
        assert_eq!(unavailable.history.unwrap().sessions, 1);
    }
}
