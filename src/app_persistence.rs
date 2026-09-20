use super::*;
use crate::herdr::{Client, Workspace};
use std::sync::{LazyLock, Mutex};
static DISMISSED: LazyLock<Mutex<std::collections::HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));
pub(super) fn dismiss_space(device: &str, workspace: &str) -> Result<()> {
    DISMISSED
        .lock()
        .map_err(|_| anyhow::anyhow!("空间状态锁不可用"))?
        .insert((device.to_ascii_uppercase(), workspace.into()));
    forget_retained(device, workspace)
}
static RETAINED_WRITE: Mutex<()> = Mutex::new(());
static RECENT_WRITE: Mutex<()> = Mutex::new(());
fn retained() -> Result<Vec<Value>> {
    let v = settings::read_preference("spaces.retained")?.unwrap_or_else(|| json!([]));
    v.as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("保留空间数据损坏，未覆盖原数据"))
}
fn belongs(v: &Value, device: &str, workspace: &str) -> bool {
    v["deviceID"]
        .as_str()
        .is_some_and(|d| d.eq_ignore_ascii_case(device))
        && v["workspaceID"] == workspace
}
pub(super) fn retained_workspaces(device: &str) -> Result<Vec<Workspace>> {
    let mut all = retained()?;
    all.retain(|v| {
        v["deviceID"]
            .as_str()
            .is_some_and(|d| d.eq_ignore_ascii_case(device))
    });
    all.sort_by_key(|v| v["sortIndex"].as_u64().unwrap_or(0));
    all.into_iter()
        .map(|v| {
            let id = v["workspaceID"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("保留空间缺少标识"))?;
            let mut w = Workspace {
                workspace_id: id.into(),
                label: v["label"].as_str().unwrap_or(id).into(),
                cwd: v["cwd"].as_str().map(str::to_owned),
                number: v["sortIndex"].as_u64().unwrap_or(0),
                ..Workspace::default()
            };
            w.extra.insert("retained".into(), true.into());
            Ok(w)
        })
        .collect()
}
pub(super) fn merge_retained_workspaces(device: &str, snapshot: &mut Snapshot) -> Result<()> {
    merge_workspaces(snapshot, retained_workspaces(device)?);
    Ok(())
}
fn merge_workspaces(snapshot: &mut Snapshot, retained: Vec<Workspace>) {
    for workspace in retained {
        if live_workspace_covers(snapshot, &workspace) {
            continue;
        }
        let index = (workspace.number as usize).min(snapshot.workspaces.len());
        snapshot.workspaces.insert(index, workspace);
    }
}

fn live_workspace_covers(snapshot: &Snapshot, workspace: &Workspace) -> bool {
    snapshot.workspaces.iter().any(|current| {
        current.extra.get("retained") != Some(&Value::Bool(true))
            && (current.workspace_id == workspace.workspace_id
                || (!workspace.label.is_empty()
                    && current.label == workspace.label
                    && current.cwd == workspace.cwd))
    })
}

pub(super) fn preserve_emptied_workspaces(previous: Option<&Snapshot>, next: &mut Snapshot) {
    if let Some(previous) = previous {
        for (index, workspace) in previous.workspaces.iter().enumerate() {
            if workspace.extra.get("retained") == Some(&Value::Bool(true)) {
                continue;
            }
            if next
                .workspaces
                .iter()
                .any(|item| item.workspace_id == workspace.workspace_id)
            {
                continue;
            }
            if previous
                .panes
                .iter()
                .any(|pane| pane.workspace_id == workspace.workspace_id)
            {
                continue;
            }
            let mut kept = workspace.clone();
            if !kept
                .cwd
                .as_deref()
                .map(str::trim)
                .is_some_and(|path| !path.is_empty())
            {
                kept.cwd = super::space_path(previous, &workspace.workspace_id);
            }
            let index = index.min(next.workspaces.len());
            next.workspaces.insert(index, kept);
        }
    }
    let drop = next
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace.extra.get("retained") == Some(&Value::Bool(true))
                && snapshot_has_live_duplicate(next, workspace)
        })
        .map(|workspace| workspace.workspace_id.clone())
        .collect::<Vec<_>>();
    next.workspaces
        .retain(|workspace| !drop.contains(&workspace.workspace_id));
}

fn snapshot_has_live_duplicate(snapshot: &Snapshot, workspace: &Workspace) -> bool {
    snapshot.workspaces.iter().any(|current| {
        current.workspace_id != workspace.workspace_id
            && current.extra.get("retained") != Some(&Value::Bool(true))
            && current.label == workspace.label
            && !current.label.is_empty()
            && current.cwd == workspace.cwd
    })
}
pub(super) fn retain_disappeared(
    device: &str,
    previous: Option<&Snapshot>,
    next: &Snapshot,
) -> Result<()> {
    let _lock = RETAINED_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("空间保存锁不可用"))?;
    let original = retained()?;
    let mut all = original.clone();
    all.retain(|r| {
        !next.workspaces.iter().any(|w| {
            w.extra.get("retained") != Some(&Value::Bool(true))
                && (belongs(r, device, &w.workspace_id)
                    || (r["deviceID"]
                        .as_str()
                        .is_some_and(|d| d.eq_ignore_ascii_case(device))
                        && r["label"].as_str() == Some(w.label.as_str())
                        && r["cwd"].as_str() == w.cwd.as_deref()))
        })
    });
    if let Some(previous) = previous {
        for (index, w) in previous.workspaces.iter().enumerate() {
            if DISMISSED
                .lock()
                .map_err(|_| anyhow::anyhow!("空间状态锁不可用"))?
                .contains(&(device.to_ascii_uppercase(), w.workspace_id.clone()))
            {
                continue;
            }
            if w.extra.get("retained") == Some(&Value::Bool(true)) {
                continue;
            }
            if !previous
                .panes
                .iter()
                .any(|pane| pane.workspace_id == w.workspace_id)
            {
                continue;
            }
            if !next
                .workspaces
                .iter()
                .any(|n| n.workspace_id == w.workspace_id)
                && !all.iter().any(|r| belongs(r, device, &w.workspace_id))
            {
                let cwd = super::space_path(previous, &w.workspace_id).or_else(|| w.cwd.clone());
                all.push(json!({"deviceID":device,"workspaceID":w.workspace_id,"label":w.label,"cwd":cwd,"sortIndex":index}));
            }
        }
    }
    if all != original {
        settings::write_preference("spaces.retained", Some(&Value::Array(all)))?;
    }
    Ok(())
}
pub(super) fn forget_retained(device: &str, workspace: &str) -> Result<()> {
    let _lock = RETAINED_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("空间保存锁不可用"))?;
    let mut all = retained()?;
    all.retain(|r| !belongs(r, device, workspace));
    settings::write_preference("spaces.retained", Some(&Value::Array(all)))
}

pub(super) fn retain_empty_space(
    device: &str,
    cwd: Option<&str>,
    label: &str,
    sort_index: u64,
) -> Result<String> {
    let _lock = RETAINED_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("空间保存锁不可用"))?;
    let mut all = retained()?;
    let used: std::collections::HashSet<String> = all
        .iter()
        .filter_map(|entry| entry["workspaceID"].as_str().map(str::to_owned))
        .collect();
    let mut n = used.len().saturating_add(1);
    let id = loop {
        let candidate = format!("empty-{n}");
        if !used.contains(&candidate) {
            break candidate;
        }
        n += 1;
    };
    let mut entry = json!({
        "deviceID": device,
        "workspaceID": id,
        "label": label,
        "sortIndex": sort_index
    });
    if let Some(cwd) = cwd {
        entry["cwd"] = cwd.into();
    }
    all.push(entry);
    settings::write_preference("spaces.retained", Some(&Value::Array(all)))?;
    Ok(id)
}

fn recent_folder_list() -> Result<Vec<Value>> {
    let v = settings::read_preference("spaces.recentFolders")?.unwrap_or_else(|| json!([]));
    v.as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("最近文件夹数据损坏，未覆盖原数据"))
}

fn folder_key(path: &str) -> String {
    path.trim().trim_end_matches('/').to_owned()
}

fn remember_recent_folder_locked(device: &str, cwd: &str, label: &str) -> Result<()> {
    let cwd = folder_key(cwd);
    if cwd.is_empty() {
        return Ok(());
    }
    let mut all = recent_folder_list()?;
    all.retain(|entry| {
        !(entry["deviceID"]
            .as_str()
            .is_some_and(|id| id.eq_ignore_ascii_case(device))
            && entry["cwd"].as_str().map(folder_key).as_deref() == Some(cwd.as_str()))
    });
    all.insert(
        0,
        json!({
            "deviceID": device,
            "cwd": cwd,
            "label": label
        }),
    );
    all.truncate(50);
    settings::write_preference("spaces.recentFolders", Some(&Value::Array(all)))
}

pub(super) fn remember_recent_folder(device: &str, cwd: &str, label: &str) -> Result<()> {
    let _lock = RECENT_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("最近文件夹锁不可用"))?;
    remember_recent_folder_locked(device, cwd, label)
}

pub(super) fn recent_folders(device: &str) -> Result<Vec<(String, String)>> {
    let all = recent_folder_list()?;
    Ok(all
        .into_iter()
        .filter(|entry| {
            entry["deviceID"]
                .as_str()
                .is_some_and(|id| id.eq_ignore_ascii_case(device))
        })
        .filter_map(|entry| {
            let cwd = entry["cwd"]
                .as_str()
                .map(folder_key)
                .filter(|path| !path.is_empty())?;
            let label = entry["label"]
                .as_str()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    std::path::Path::new(&cwd)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(cwd.as_str())
                        .to_owned()
                });
            Some((cwd, label))
        })
        .collect())
}

pub(super) fn rename_retained(device: &str, workspace: &str, label: &str) -> Result<()> {
    let _lock = RETAINED_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("空间保存锁不可用"))?;
    let mut all = retained()?;
    let retained = all
        .iter_mut()
        .find(|entry| belongs(entry, device, workspace))
        .ok_or_else(|| anyhow::anyhow!("保留空间不存在"))?;
    retained["label"] = label.into();
    settings::write_preference("spaces.retained", Some(&Value::Array(all)))
}
fn is_workspace_not_found(error: &crate::herdr::Error) -> bool {
    matches!(error, crate::herdr::Error::Rpc { code, .. } if code == "workspace_not_found")
}

fn create_tab_in_workspace(client: &Client, workspace: &str) -> crate::herdr::Result<Value> {
    client
        .mutation(
            "tab.create",
            json!({"focus": false, "workspace_id": workspace}),
        )
        .map(stamp_created_pane)
}

/// Never retry creation after an uncertain response, and never auto-close a successfully created pane.
pub(super) fn revive_retained(client: &Client, device: &str, workspace: &str) -> Result<Value> {
    revive_retained_excluding(client, device, workspace, None)
}

fn revive_retained_excluding(
    client: &Client,
    device: &str,
    workspace: &str,
    exclude: Option<&str>,
) -> Result<Value> {
    let old = retained()?
        .into_iter()
        .find(|v| belongs(v, device, workspace))
        .ok_or_else(|| anyhow::anyhow!("保留空间不存在"))?;
    let mut snapshot = client.snapshot()?;
    merge_retained_workspaces(device, &mut snapshot)?;
    let before = snapshot
        .workspaces
        .iter()
        .skip_while(|current| current.workspace_id != workspace)
        .skip(1)
        .find(|current| {
            current.extra.get("retained") != Some(&Value::Bool(true))
                && exclude != Some(current.workspace_id.as_str())
        })
        .map(|current| current.workspace_id.clone());
    if let Some(live) =
        live_workspace_for_restore(&snapshot, workspace, old["cwd"].as_str(), exclude)
    {
        match create_tab_in_workspace(client, &live) {
            Ok(value) => {
                forget_retained(device, workspace)?;
                return Ok(value);
            }
            Err(error) if is_workspace_not_found(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    let result = client.create_workspace(old["label"].as_str(), old["cwd"].as_str())?;
    let id = result
        .pointer("/workspace/workspace_id")
        .or_else(|| result.get("workspace_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("创建已提交，但响应缺少空间标识；请刷新，勿重复创建"))?;
    forget_retained(device, workspace)?;
    if let Err(error) = client.move_workspaces(&[id.to_owned()], before.as_deref()) {
        bail!("空间 {id} 已恢复，但排序失败：{error}；请刷新后重排，不要重复创建");
    }
    Ok(stamp_created_pane(result))
}

fn stamp_created_pane(mut value: Value) -> Value {
    if value.get("created_pane").is_none() {
        value["created_pane"] = value
            .pointer("/root_pane/pane_id")
            .cloned()
            .unwrap_or(Value::Null);
    }
    value
}

fn live_workspace_for_restore(
    snapshot: &Snapshot,
    workspace: &str,
    cwd: Option<&str>,
    exclude: Option<&str>,
) -> Option<String> {
    let cwd = cwd.map(str::trim).filter(|path| !path.is_empty());
    snapshot.workspaces.iter().find_map(|current| {
        (exclude != Some(current.workspace_id.as_str())
            && current.extra.get("retained") != Some(&Value::Bool(true))
            && (current.workspace_id == workspace
                || cwd.is_some_and(|path| {
                    current
                        .cwd
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|value| value == path)
                })))
        .then(|| current.workspace_id.clone())
    })
}

fn remember_workspace(
    device: &str,
    workspace: &str,
    label: Option<&str>,
    cwd: Option<&str>,
) -> Result<()> {
    let _lock = RETAINED_WRITE
        .lock()
        .map_err(|_| anyhow::anyhow!("空间保存锁不可用"))?;
    let mut all = retained()?;
    if all.iter().any(|entry| belongs(entry, device, workspace)) {
        if let Some(cwd) = cwd.filter(|value| !value.is_empty()) {
            if let Some(entry) = all
                .iter_mut()
                .find(|entry| belongs(entry, device, workspace))
            {
                if entry["cwd"]
                    .as_str()
                    .map(str::trim)
                    .is_none_or(|value| value.is_empty())
                {
                    entry["cwd"] = cwd.into();
                    settings::write_preference("spaces.retained", Some(&Value::Array(all)))?;
                }
            }
        }
        return Ok(());
    }
    let mut entry = json!({
        "deviceID": device,
        "workspaceID": workspace,
        "label": label.filter(|value| !value.is_empty()).unwrap_or(workspace),
        "sortIndex": all.len() as u64
    });
    if let Some(cwd) = cwd.filter(|value| !value.is_empty()) {
        entry["cwd"] = cwd.into();
    }
    all.push(entry);
    settings::write_preference("spaces.retained", Some(&Value::Array(all)))
}

pub(super) fn open_tab_in_workspace(
    client: &Client,
    device: &str,
    workspace: &str,
    revive: bool,
    label: Option<&str>,
    cwd: Option<&str>,
) -> Result<Value> {
    let mut exclude = None;
    if !revive {
        match create_tab_in_workspace(client, workspace) {
            Ok(value) => return Ok(value),
            Err(error) if is_workspace_not_found(&error) => {
                exclude = Some(workspace.to_owned());
            }
            Err(error) => return Err(error.into()),
        }
    }
    let snapshot = client.snapshot()?;
    if let Some(live) = live_workspace_for_restore(&snapshot, workspace, cwd, exclude.as_deref()) {
        match create_tab_in_workspace(client, &live) {
            Ok(value) => {
                let _ = forget_retained(device, workspace);
                return Ok(value);
            }
            Err(error) if is_workspace_not_found(&error) => exclude = Some(live),
            Err(error) => return Err(error.into()),
        }
    }
    remember_workspace(device, workspace, label, cwd)?;
    revive_retained_excluding(client, device, workspace, exclude.as_deref())
}
impl AppView {
    pub(super) fn restore_selection(&mut self) -> Result<()> {
        if let Some(id) =
            settings::read_preference("device.filter")?.and_then(|v| v.as_str().map(str::to_owned))
        {
            if let Some(index) = self
                .devices
                .iter()
                .position(|d| d.id.eq_ignore_ascii_case(&id))
            {
                self.selected_device = index;
            }
        }
        let device = self.device().id;
        if !self.settings.spaces_hidden {
            if let Some(v) = settings::read_preference("session.selectedSpace")? {
                if v["deviceID"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&device))
                {
                    self.workspace = v["workspaceID"].as_str().map(str::to_owned);
                }
            }
        }
        if let Some(v) = settings::read_preference("session.selectedPane")? {
            if v["deviceID"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&device))
            {
                self.selected_pane = v["paneID"].as_str().map(str::to_owned);
            }
        }
        Ok(())
    }
    pub(super) fn persist_selection(&self) -> Result<()> {
        let device = self.device().id;
        settings::write_preference("device.filter", Some(&json!(device)))?;
        let space = self
            .workspace
            .as_ref()
            .filter(|_| !self.settings.spaces_hidden)
            .map(|id| json!({"deviceID":device,"workspaceID":id}));
        let pane = self
            .selected_pane
            .as_ref()
            .map(|id| json!({"deviceID":device,"paneID":id}));
        settings::write_preference("session.selectedSpace", space.as_ref())?;
        settings::write_preference("session.selectedPane", pane.as_ref())
    }
}
#[cfg(test)]
mod tests {
    use super::{
        belongs, live_workspace_for_restore, merge_workspaces, preserve_emptied_workspaces,
    };
    use crate::herdr::Snapshot;
    use serde_json::json;
    #[test]
    fn identity_is_device_scoped() {
        let v = json!({"deviceID":"ABC","workspaceID":"w1"});
        assert!(belongs(&v, "abc", "w1"));
        assert!(!belongs(&v, "other", "w1"));
        assert!(!belongs(&v, "abc", "w2"));
    }
    #[test]
    fn merge_keeps_retained_at_its_saved_index() {
        let mut snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id":"live"}, {"workspace_id":"tail"}]
        }))
        .unwrap();
        let retained = |id: &str, number| crate::herdr::Workspace {
            workspace_id: id.into(),
            number,
            extra: [("retained".into(), true.into())].into_iter().collect(),
            ..Default::default()
        };
        merge_workspaces(
            &mut snapshot,
            vec![
                retained("kept", 0),
                retained("live", 0),
                retained("saved", 1),
            ],
        );
        assert_eq!(
            snapshot
                .workspaces
                .iter()
                .map(|workspace| workspace.workspace_id.as_str())
                .collect::<Vec<_>>(),
            ["kept", "saved", "live", "tail"]
        );
    }

    #[test]
    fn emptied_space_stays_live_and_keeps_its_directory() {
        let previous: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id":"harmony","label":"harmony"}],
            "panes": [{"pane_id":"1","workspace_id":"other"}],
            "agents": [{"pane_id":"a","workspace_id":"harmony","tab_id":"t","cwd":"/tmp/harmony"}]
        }))
        .unwrap();
        let mut next: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id":"other","label":"other"}]
        }))
        .unwrap();
        preserve_emptied_workspaces(Some(&previous), &mut next);
        assert_eq!(
            next.workspaces
                .iter()
                .map(|workspace| workspace.workspace_id.as_str())
                .collect::<Vec<_>>(),
            ["harmony", "other"]
        );
        assert_ne!(next.workspaces[0].extra.get("retained"), Some(&json!(true)));
        assert_eq!(next.workspaces[0].cwd.as_deref(), Some("/tmp/harmony"));
    }

    #[test]
    fn restore_reuses_the_live_space_for_the_same_directory() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [
                {"workspace_id":"ghost","label":"harmony","cwd":"/tmp/old","retained":true},
                {"workspace_id":"live","label":"harmony","cwd":" /tmp/harmony "}
            ]
        }))
        .unwrap();
        assert_eq!(
            live_workspace_for_restore(&snapshot, "wAZ", Some("/tmp/harmony"), None).as_deref(),
            Some("live")
        );
        assert_eq!(
            live_workspace_for_restore(&snapshot, "live", None, None).as_deref(),
            Some("live")
        );
        assert_eq!(
            live_workspace_for_restore(&snapshot, "wAZ", None, None),
            None
        );
    }

    #[test]
    fn restore_does_not_retry_a_workspace_the_server_already_rejected() {
        let snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [
                {"workspace_id":"wB2","label":"pi-jev-route","cwd":"/tmp/pi-jev-route"}
            ]
        }))
        .unwrap();
        assert_eq!(
            live_workspace_for_restore(&snapshot, "wB2", Some("/tmp/pi-jev-route"), None)
                .as_deref(),
            Some("wB2")
        );
        assert_eq!(
            live_workspace_for_restore(&snapshot, "wB2", Some("/tmp/pi-jev-route"), Some("wB2")),
            None
        );
    }

    #[test]
    fn retained_ghost_does_not_duplicate_a_live_space_with_the_same_label() {
        let mut snapshot: Snapshot = serde_json::from_value(json!({
            "workspaces": [{"workspace_id":"live","label":"harmony","cwd":"/tmp/harmony"}]
        }))
        .unwrap();
        let mut ghost = crate::herdr::Workspace {
            workspace_id: "ghost".into(),
            label: "harmony".into(),
            cwd: Some("/tmp/harmony".into()),
            extra: [("retained".into(), true.into())].into_iter().collect(),
            ..Default::default()
        };
        ghost.number = 0;
        merge_workspaces(&mut snapshot, vec![ghost.clone()]);
        assert_eq!(snapshot.workspaces.len(), 1);
        snapshot.workspaces.push(ghost);
        preserve_emptied_workspaces(None, &mut snapshot);
        assert_eq!(snapshot.workspaces.len(), 1);
        assert_eq!(snapshot.workspaces[0].workspace_id, "live");
    }
}
