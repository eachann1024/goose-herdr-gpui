//! Native permission and click routing, without assuming a permission grant.
use anyhow::{Result, bail};
use std::{
    ffi::{CStr, CString, c_char},
    sync::{
        Mutex, Once,
        atomic::{AtomicI32, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Unknown,
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Done,
    Blocked,
}
#[derive(Clone, Debug)]
pub struct NotificationRoute {
    pub device_id: String,
    pub pane_id: String,
}
static PERMISSION: AtomicI32 = AtomicI32::new(-1);
static ROUTES: Mutex<Vec<NotificationRoute>> = Mutex::new(Vec::new());
static INIT: Once = Once::new();
unsafe extern "C" {
    fn goose_notifications_init(callback: extern "C" fn(*const c_char, *const c_char));
    fn goose_notifications_permission(request: bool, callback: extern "C" fn(i32));
    fn goose_notifications_post(
        title: *const c_char,
        body: *const c_char,
        device: *const c_char,
        pane: *const c_char,
    );
    fn goose_notifications_sound(blocked: bool);
}
extern "C" fn permission_callback(value: i32) {
    PERMISSION.store(value, Ordering::Relaxed);
}
extern "C" fn route_callback(device: *const c_char, pane: *const c_char) {
    if device.is_null() || pane.is_null() {
        return;
    }
    let route = unsafe {
        NotificationRoute {
            device_id: CStr::from_ptr(device).to_string_lossy().into_owned(),
            pane_id: CStr::from_ptr(pane).to_string_lossy().into_owned(),
        }
    };
    if let Ok(mut routes) = ROUTES.lock() {
        routes.push(route);
    }
}
pub fn init() {
    INIT.call_once(|| {
        crate::credentials::purge_authorizations();
        unsafe {
            goose_notifications_init(route_callback);
        }
    });
    query_permission();
}
pub fn permission() -> Permission {
    match PERMISSION.load(Ordering::Relaxed) {
        0 => Permission::NotDetermined,
        1 => Permission::Denied,
        2 => Permission::Authorized,
        3 => Permission::Provisional,
        _ => Permission::Unknown,
    }
}
pub fn query_permission() {
    unsafe {
        goose_notifications_permission(false, permission_callback);
    }
}
pub fn request_permission() {
    unsafe {
        goose_notifications_permission(true, permission_callback);
    }
}
pub fn take_routes() -> Vec<NotificationRoute> {
    ROUTES
        .lock()
        .map(|mut r| std::mem::take(&mut *r))
        .unwrap_or_default()
}
pub fn enabled() -> bool {
    preference("notifications.enabled")
}
pub fn sound_enabled() -> bool {
    preference("notifications.sound")
}
fn preference(key: &str) -> bool {
    crate::settings::read_preference(key)
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}
pub fn set_enabled(value: bool) -> Result<()> {
    crate::settings::write_preference(
        "notifications.enabled",
        Some(&serde_json::Value::Bool(value)),
    )
}
pub fn set_sound_enabled(value: bool) -> Result<()> {
    crate::settings::write_preference("notifications.sound", Some(&serde_json::Value::Bool(value)))
}
pub fn post(
    title: &str,
    agent: &str,
    status: AgentStatus,
    device_id: &str,
    device_name: &str,
    pane_id: &str,
    space_name: &str,
) -> Result<()> {
    if device_id.is_empty() || pane_id.is_empty() {
        bail!(crate::i18n::tr("通知缺少设备或窗格标识"));
    }
    let event = if status == AgentStatus::Blocked {
        crate::i18n::tr("需要你的输入")
    } else {
        crate::i18n::tr("已完成")
    };
    let body = CString::new(format!("{agent} {event} · {space_name} · {device_name}"))?;
    let title = CString::new(title)?;
    let device = CString::new(device_id)?;
    let pane = CString::new(pane_id)?;
    if sound_enabled() {
        unsafe {
            goose_notifications_sound(status == AgentStatus::Blocked);
        }
    }
    if enabled()
        && matches!(
            permission(),
            Permission::Authorized | Permission::Provisional
        )
    {
        unsafe {
            goose_notifications_post(
                title.as_ptr(),
                body.as_ptr(),
                device.as_ptr(),
                pane.as_ptr(),
            );
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_preserve_device_and_pane() {
        let device = CString::new("device-1").unwrap();
        let pane = CString::new("%7").unwrap();
        route_callback(device.as_ptr(), pane.as_ptr());
        let routes = take_routes();
        assert_eq!(routes[0].device_id, "device-1");
        assert_eq!(routes[0].pane_id, "%7");
        assert!(take_routes().is_empty());
    }
}
