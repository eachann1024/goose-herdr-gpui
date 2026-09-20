use crate::herdr::Device;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::{CString, c_void},
    fs,
    io::Write,
    path::PathBuf,
};
pub(crate) type CF = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CFRelease(v: CF);
    pub(crate) fn CFStringCreateWithCString(a: CF, s: *const i8, e: u32) -> CF;
    fn CFStringGetCString(s: CF, b: *mut i8, n: isize, e: u32) -> bool;
    fn CFStringGetLength(s: CF) -> isize;
    fn CFGetTypeID(v: CF) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFBooleanGetValue(v: CF) -> bool;
    fn CFNumberGetValue(v: CF, t: isize, out: *mut c_void) -> bool;
    fn CFNumberCreate(a: CF, t: isize, v: *const c_void) -> CF;
    pub(crate) fn CFDataCreate(a: CF, b: *const u8, n: isize) -> CF;
    pub(crate) fn CFDataGetLength(v: CF) -> isize;
    pub(crate) fn CFDataGetBytePtr(v: CF) -> *const u8;
    fn CFArrayGetCount(v: CF) -> isize;
    fn CFArrayGetValueAtIndex(v: CF, i: isize) -> CF;
    fn CFArrayCreate(a: CF, v: *const CF, n: isize, c: CF) -> CF;
    fn CFDictionaryGetCount(v: CF) -> isize;
    fn CFDictionaryGetKeysAndValues(v: CF, k: *mut CF, val: *mut CF);
    pub(crate) fn CFDictionaryCreate(
        a: CF,
        k: *const CF,
        v: *const CF,
        n: isize,
        kc: CF,
        vc: CF,
    ) -> CF;
    fn CFPreferencesCopyAppValue(k: CF, app: CF) -> CF;
    fn CFPreferencesSetAppValue(k: CF, v: CF, app: CF);
    fn CFPreferencesAppSynchronize(app: CF) -> bool;
    static kCFTypeArrayCallBacks: u8;
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    pub(crate) static kCFBooleanTrue: CF;
    pub(crate) static kCFBooleanFalse: CF;
}
pub(crate) fn cfstr(s: &str) -> Result<CF> {
    let c = CString::new(s)?;
    Ok(unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), 0x08000100) })
}
pub(crate) unsafe fn cfbytes(v: CF) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(v), CFDataGetLength(v) as usize).to_vec() }
}
unsafe fn value(v: CF) -> Result<Value> {
    unsafe {
        let id = CFGetTypeID(v);
        Ok(if id == CFStringGetTypeID() {
            let mut b = vec![0u8; CFStringGetLength(v) as usize * 4 + 1];
            if !CFStringGetCString(v, b.as_mut_ptr().cast(), b.len() as isize, 0x08000100) {
                bail!("偏好字符串编码失败")
            };
            Value::String(String::from_utf8(
                b.into_iter().take_while(|b| *b != 0).collect(),
            )?)
        } else if id == CFBooleanGetTypeID() {
            json!(CFBooleanGetValue(v))
        } else if id == CFNumberGetTypeID() {
            let mut n = 0f64;
            CFNumberGetValue(v, 13, (&mut n as *mut f64).cast());
            json!(n)
        } else if id == CFDataGetTypeID() {
            serde_json::from_slice(&cfbytes(v)).context("偏好JSON数据损坏")?
        } else if id == CFArrayGetTypeID() {
            Value::Array(
                (0..CFArrayGetCount(v))
                    .map(|i| value(CFArrayGetValueAtIndex(v, i)))
                    .collect::<Result<_>>()?,
            )
        } else if id == CFDictionaryGetTypeID() {
            let n = CFDictionaryGetCount(v) as usize;
            let mut k = vec![std::ptr::null(); n];
            let mut vals = k.clone();
            CFDictionaryGetKeysAndValues(v, k.as_mut_ptr(), vals.as_mut_ptr());
            let mut map = serde_json::Map::new();
            for (k, v) in k.into_iter().zip(vals) {
                let key = value(k)?.as_str().context("非法偏好键")?.to_owned();
                map.insert(key, value(v)?);
            }
            Value::Object(map)
        } else {
            bail!("不支持的偏好类型")
        })
    }
}
fn cfvalue(v: &Value, owned: &mut Vec<CF>) -> Result<CF> {
    let p = unsafe {
        match v {
            Value::String(s) => cfstr(s)?,
            Value::Bool(b) => return Ok(if *b { kCFBooleanTrue } else { kCFBooleanFalse }),
            Value::Number(n) => {
                let n = n.as_f64().context("非法数字")?;
                CFNumberCreate(std::ptr::null(), 13, (&n as *const f64).cast())
            }
            Value::Array(a) => {
                let a = a
                    .iter()
                    .map(|v| cfvalue(v, owned))
                    .collect::<Result<Vec<_>>>()?;
                CFArrayCreate(
                    std::ptr::null(),
                    a.as_ptr(),
                    a.len() as isize,
                    std::ptr::addr_of!(kCFTypeArrayCallBacks).cast(),
                )
            }
            Value::Object(m) => {
                let mut k = vec![];
                let mut vs = vec![];
                for (key, v) in m {
                    k.push(cfvalue(&json!(key), owned)?);
                    vs.push(cfvalue(v, owned)?);
                }
                CFDictionaryCreate(
                    std::ptr::null(),
                    k.as_ptr(),
                    vs.as_ptr(),
                    k.len() as isize,
                    std::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),
                    std::ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast(),
                )
            }
            Value::Null => bail!("偏好值不能为null"),
        }
    };
    owned.push(p);
    Ok(p)
}
pub fn read_preference(key: &str) -> Result<Option<Value>> {
    let k = cfstr(key)?;
    let app = cfstr("dev.eachann.goose-herdr")?;
    unsafe {
        let v = CFPreferencesCopyAppValue(k, app);
        CFRelease(k);
        CFRelease(app);
        if v.is_null() {
            return Ok(None);
        }
        let result = value(v);
        CFRelease(v);
        result.map(Some)
    }
}
pub fn write_preference(key: &str, v: Option<&Value>) -> Result<()> {
    let mut owned = vec![];
    let p = match v {
        None => std::ptr::null(),
        Some(v)
            if matches!(
                key,
                "spaces.retained"
                    | "spaces.recentFolders"
                    | "session.selectedPane"
                    | "session.selectedSpace"
                    | "app.keyboardShortcuts"
                    | "app.keyboardShortcuts.agentKinds"
            ) =>
        {
            let data = serde_json::to_vec(v)?;
            let p = unsafe { CFDataCreate(std::ptr::null(), data.as_ptr(), data.len() as isize) };
            owned.push(p);
            p
        }
        Some(v) => cfvalue(v, &mut owned)?,
    };
    let k = cfstr(key)?;
    let app = cfstr("dev.eachann.goose-herdr")?;
    unsafe {
        CFPreferencesSetAppValue(k, p, app);
        let ok = CFPreferencesAppSynchronize(app);
        CFRelease(k);
        CFRelease(app);
        for p in owned.into_iter().rev() {
            CFRelease(p)
        }
        if !ok {
            bail!("偏好保存失败")
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Settings {
    pub theme: String,
    pub language: String,
    pub font_name: String,
    pub font_size: f64,
    pub font_weight: f64,
    pub line_spacing: f64,
    pub thin_strokes: bool,
    pub mouse_reporting: bool,
    pub terminal_padding_x: f64,
    pub terminal_padding_y: f64,
    pub terminal_padding_balance: bool,
    pub terminal_margin: f64,
    pub unfocused_split_opacity: f64,
    pub copy_on_select: bool,
    pub notifications_enabled: bool,
    pub notifications_sound: bool,
    pub spaces_hidden: bool,
    pub priority_sessions: bool,
    pub agent_bypass_default: bool,
    pub agent_binary_overrides: HashMap<String, String>,
    pub disabled_agent_kinds: Vec<String>,
    pub agent_kind_order: Vec<String>,
    pub shortcuts: Value,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            language: "".into(),
            font_name: "".into(),
            font_size: 12.5,
            font_weight: 0.,
            line_spacing: 1.,
            thin_strokes: true,
            mouse_reporting: true,
            terminal_padding_x: 8.,
            terminal_padding_y: 8.,
            terminal_padding_balance: false,
            terminal_margin: 0.,
            unfocused_split_opacity: 1.,
            copy_on_select: false,
            notifications_enabled: true,
            notifications_sound: true,
            spaces_hidden: false,
            priority_sessions: false,
            agent_bypass_default: true,
            agent_binary_overrides: HashMap::new(),
            disabled_agent_kinds: vec![],
            agent_kind_order: vec![],
            shortcuts: json!({}),
        }
    }
}
impl Settings {
    pub fn load() -> Result<Self> {
        let mut s = Self::default();
        macro_rules! get {
            ($field:ident,$key:literal) => {
                if let Some(v) = read_preference($key)? {
                    s.$field = serde_json::from_value(v)
                        .with_context(|| format!("偏好格式无效：{}", $key))?;
                }
            };
        }
        get!(theme, "app.theme");
        get!(language, "app.language");
        get!(font_name, "terminal.fontName");
        get!(font_size, "terminal.fontSize");
        get!(font_weight, "terminal.fontWeight");
        get!(line_spacing, "terminal.lineSpacing");
        get!(thin_strokes, "terminal.thinStrokes");
        get!(mouse_reporting, "terminal.mouseReporting");
        get!(terminal_padding_x, "terminal.paddingX");
        get!(terminal_padding_y, "terminal.paddingY");
        get!(terminal_padding_balance, "terminal.paddingBalance");
        get!(terminal_margin, "terminal.margin");
        get!(unfocused_split_opacity, "terminal.unfocusedSplitOpacity");
        get!(copy_on_select, "terminal.copyOnSelect");
        get!(notifications_enabled, "notifications.enabled");
        get!(notifications_sound, "notifications.sound");
        get!(spaces_hidden, "sidebar.spacesHidden");
        get!(priority_sessions, "sidebar.prioritySessions");
        get!(agent_bypass_default, "agent.bypassDefault");
        get!(agent_binary_overrides, "agent.binaryOverrides");
        get!(disabled_agent_kinds, "agents.disabledKinds");
        get!(agent_kind_order, "agents.kindOrder");
        get!(shortcuts, "app.keyboardShortcuts");
        s.validate_terminal_layout()?;
        Ok(s)
    }
    fn validate_terminal_layout(&self) -> Result<()> {
        for (key, value, minimum, maximum) in [
            ("terminal.paddingX", self.terminal_padding_x, 0., 64.),
            ("terminal.paddingY", self.terminal_padding_y, 0., 64.),
            ("terminal.margin", self.terminal_margin, 0., 64.),
            (
                "terminal.unfocusedSplitOpacity",
                self.unfocused_split_opacity,
                0.15,
                1.,
            ),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                bail!("偏好值超出允许范围：{key} ({minimum}–{maximum})")
            }
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        self.validate_terminal_layout()?;
        if !["system", "light", "dark"].contains(&self.theme.as_str())
            || !self.font_size.is_finite()
            || !(9.0..=22.0).contains(&self.font_size)
            || !self.line_spacing.is_finite()
            || !(1.0..=1.4).contains(&self.line_spacing)
            || !self.font_weight.is_finite()
        {
            bail!("设置值超出允许范围")
        };
        macro_rules! put {
            ($field:ident,$key:literal) => {
                write_preference($key, Some(&json!(self.$field)))?;
            };
        }
        put!(theme, "app.theme");
        put!(language, "app.language");
        put!(font_name, "terminal.fontName");
        put!(font_size, "terminal.fontSize");
        put!(font_weight, "terminal.fontWeight");
        put!(line_spacing, "terminal.lineSpacing");
        put!(thin_strokes, "terminal.thinStrokes");
        put!(mouse_reporting, "terminal.mouseReporting");
        put!(terminal_padding_x, "terminal.paddingX");
        put!(terminal_padding_y, "terminal.paddingY");
        put!(terminal_padding_balance, "terminal.paddingBalance");
        put!(terminal_margin, "terminal.margin");
        put!(unfocused_split_opacity, "terminal.unfocusedSplitOpacity");
        put!(copy_on_select, "terminal.copyOnSelect");
        put!(notifications_enabled, "notifications.enabled");
        put!(notifications_sound, "notifications.sound");
        put!(spaces_hidden, "sidebar.spacesHidden");
        put!(priority_sessions, "sidebar.prioritySessions");
        put!(agent_bypass_default, "agent.bypassDefault");
        put!(agent_binary_overrides, "agent.binaryOverrides");
        put!(disabled_agent_kinds, "agents.disabledKinds");
        put!(agent_kind_order, "agents.kindOrder");
        put!(shortcuts, "app.keyboardShortcuts");
        Ok(())
    }
}
fn devices_path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("找不到HOME")?)
            .join("Library/Application Support/HerdrM/devices.json"),
    )
}
pub fn load_devices() -> Result<Vec<Device>> {
    let p = devices_path()?;
    if !p.exists() {
        return Ok(vec![Device::local()]);
    }
    let mut devices: Vec<Device> =
        serde_json::from_slice(&fs::read(p)?).context("设备数据损坏，原文件已保留")?;
    devices.retain(|d| !d.is_local());
    devices.insert(0, Device::local());
    Ok(devices)
}
pub fn save_devices(devices: &[Device]) -> Result<()> {
    let p = devices_path()?;
    if p.exists() {
        let _: Vec<Device> =
            serde_json::from_slice(&fs::read(&p)?).context("拒绝覆盖损坏的设备数据")?;
    }
    fs::create_dir_all(p.parent().unwrap())?;
    let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(devices)?)?;
        f.sync_all()?;
        fs::rename(&tmp, &p)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_layout_defaults_and_ranges() {
        let mut settings = Settings::default();
        assert_eq!(settings.terminal_padding_x, 8.);
        assert_eq!(settings.terminal_padding_y, 8.);
        assert_eq!(settings.terminal_margin, 0.);
        assert_eq!(settings.unfocused_split_opacity, 1.);
        assert!(!settings.terminal_padding_balance);
        assert!(!settings.copy_on_select);
        assert!(settings.validate_terminal_layout().is_ok());
        for field in 0..4 {
            let (minimum, maximum) = if field == 3 { (0.15, 1.) } else { (0., 64.) };
            for (value, valid) in [
                (minimum, true),
                (maximum, true),
                (minimum - 0.01, false),
                (maximum + 0.01, false),
                (f64::NAN, false),
                (f64::INFINITY, false),
            ] {
                settings = Settings::default();
                match field {
                    0 => settings.terminal_padding_x = value,
                    1 => settings.terminal_padding_y = value,
                    2 => settings.terminal_margin = value,
                    _ => settings.unfocused_split_opacity = value,
                }
                assert_eq!(settings.validate_terminal_layout().is_ok(), valid);
            }
        }
    }
    #[test]
    fn cf_values_round_trip_without_user_preferences() {
        let source =
            json!({"theme":"dark","font":12.5,"flags":[true,false],"names":["中文","Agent"]});
        let mut owned = vec![];
        let raw = cfvalue(&source, &mut owned).unwrap();
        assert_eq!(unsafe { value(raw).unwrap() }, source);
        unsafe {
            for raw in owned.into_iter().rev() {
                CFRelease(raw);
            }
        }
    }
}
