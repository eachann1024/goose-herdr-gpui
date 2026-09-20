use crate::settings::{
    CF, CFDataCreate, CFDictionaryCreate, CFRelease, cfbytes, cfstr, kCFBooleanTrue,
};
use anyhow::{Context, Result, bail};
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecClass: CF;
    static kSecClassGenericPassword: CF;
    static kSecAttrService: CF;
    static kSecAttrAccount: CF;
    static kSecValueData: CF;
    static kSecReturnData: CF;
    static kSecMatchLimit: CF;
    static kSecMatchLimitOne: CF;
    static kSecAttrAccessible: CF;
    static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: CF;
    static kSecAttrAccessibleAfterFirstUnlock: CF;
    fn SecItemCopyMatching(q: CF, out: *mut CF) -> i32;
    fn SecItemAdd(q: CF, out: *mut CF) -> i32;
    fn SecItemUpdate(q: CF, a: CF) -> i32;
    fn SecItemDelete(q: CF) -> i32;
}
#[derive(Clone, Copy)]
pub enum SecretKind {
    SshPassword,
    TailcatToken,
}
impl SecretKind {
    fn service(self) -> &'static str {
        match self {
            Self::SshPassword => "dev.bybee.herdrm.ssh-password",
            Self::TailcatToken => "dev.bybee.herdrm.tailcat-token",
        }
    }
}
struct Query {
    keys: Vec<CF>,
    values: Vec<CF>,
    owned: Vec<CF>,
}
impl Query {
    fn new(service: &str, account: &str) -> Result<Self> {
        let s = cfstr(service)?;
        let a = cfstr(account)?;
        unsafe {
            Ok(Self {
                keys: vec![kSecClass, kSecAttrService, kSecAttrAccount],
                values: vec![kSecClassGenericPassword, s, a],
                owned: vec![s, a],
            })
        }
    }
    fn add(&mut self, k: CF, v: CF) {
        self.keys.push(k);
        self.values.push(v);
    }
    fn dict(&self) -> CF {
        unsafe {
            CFDictionaryCreate(
                std::ptr::null(),
                self.keys.as_ptr(),
                self.values.as_ptr(),
                self.keys.len() as isize,
                std::ptr::null(),
                std::ptr::null(),
            )
        }
    }
}
impl Drop for Query {
    fn drop(&mut self) {
        for p in &self.owned {
            unsafe { CFRelease(*p) }
        }
    }
}
fn account(id: &str) -> Result<String> {
    if id.len() != 36
        || !id.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
    {
        bail!("凭据设备标识无效")
    }
    Ok(id.to_ascii_uppercase())
}
fn read(service: &str, id: &str) -> Result<Option<String>> {
    let mut q = Query::new(service, &account(id)?)?;
    unsafe {
        q.add(kSecReturnData, kCFBooleanTrue);
        q.add(kSecMatchLimit, kSecMatchLimitOne);
        let d = q.dict();
        let mut out = std::ptr::null();
        let status = SecItemCopyMatching(d, &mut out);
        CFRelease(d);
        if status == -25300 {
            return Ok(None);
        }
        if status != 0 {
            bail!("Keychain读取失败（{}）", status)
        }
        let data = cfbytes(out);
        CFRelease(out);
        Ok(Some(String::from_utf8(data).context("凭据编码无效")?))
    }
}
fn write(service: &str, id: &str, secret: &str, tailcat: bool) -> Result<()> {
    let q = Query::new(service, &account(id)?)?;
    unsafe {
        let d = q.dict();
        let data = CFDataCreate(std::ptr::null(), secret.as_ptr(), secret.len() as isize);
        let keys = [kSecValueData, kSecAttrAccessible];
        let values = [
            data,
            if tailcat {
                kSecAttrAccessibleAfterFirstUnlock
            } else {
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            },
        ];
        let attrs = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            2,
            std::ptr::null(),
            std::ptr::null(),
        );
        let mut status = SecItemUpdate(d, attrs);
        CFRelease(d);
        CFRelease(attrs);
        if status == -25300 {
            let mut q = q;
            q.add(kSecValueData, data);
            q.add(kSecAttrAccessible, values[1]);
            let d = q.dict();
            status = SecItemAdd(d, std::ptr::null_mut());
            CFRelease(d);
        }
        CFRelease(data);
        if status != 0 {
            bail!("Keychain保存失败（{}）", status)
        }
    }
    Ok(())
}
fn remove(service: &str, id: &str) -> Result<()> {
    let q = Query::new(service, &account(id)?)?;
    unsafe {
        let d = q.dict();
        let status = SecItemDelete(d);
        CFRelease(d);
        if status != 0 && status != -25300 {
            bail!("Keychain删除失败（{}）", status)
        }
    }
    Ok(())
}
pub fn get(kind: SecretKind, id: &str) -> Result<Option<String>> {
    let existing = read(kind.service(), id)?;
    if existing.is_some() || !matches!(kind, SecretKind::SshPassword) {
        return Ok(existing);
    }
    let legacy = std::path::PathBuf::from(std::env::var_os("HOME").context("找不到HOME")?)
        .join("Library/Application Support/HerdrM/SSHCredentials/passwords")
        .join(account(id)?);
    let password = match std::fs::read_to_string(&legacy) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    write(kind.service(), id, &password, false)?;
    if read(kind.service(), id)?.as_deref() != Some(password.as_str()) {
        bail!("Keychain迁移校验失败，原凭据已保留")
    }
    std::fs::remove_file(legacy)?;
    Ok(Some(password))
}
pub fn set(kind: SecretKind, id: &str, secret: &str) -> Result<()> {
    if secret.is_empty() {
        return delete(kind, id);
    }
    write(
        kind.service(),
        id,
        secret,
        matches!(kind, SecretKind::TailcatToken),
    )
}
pub fn delete(kind: SecretKind, id: &str) -> Result<()> {
    remove(kind.service(), id)?;
    if matches!(kind, SecretKind::SshPassword) {
        let path = std::path::PathBuf::from(std::env::var_os("HOME").context("找不到HOME")?)
            .join("Library/Application Support/HerdrM/SSHCredentials/passwords")
            .join(account(id)?);
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
const AUTH: &str = "dev.bybee.herdrm.ssh-authorization";

/// Drops grants stranded by a crash between creation and askpass consumption.
/// Call from the main process only, never from the SSH_ASKPASS child.
pub fn purge_authorizations() {
    let Ok(service) = cfstr(AUTH) else {
        return;
    };
    unsafe {
        let keys = [kSecClass, kSecAttrService];
        let values = [kSecClassGenericPassword, service];
        let query = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            2,
            std::ptr::null(),
            std::ptr::null(),
        );
        let _ = SecItemDelete(query);
        CFRelease(query);
        CFRelease(service);
    }
}
pub struct SshAuthentication {
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    authorization: Option<String>,
}
impl std::fmt::Debug for SshAuthentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SshAuthentication([redacted])")
    }
}
impl Drop for SshAuthentication {
    fn drop(&mut self) {
        if let Some(id) = &self.authorization {
            let _ = remove(AUTH, id);
        }
    }
}
pub fn ssh_authentication(id: &str) -> Result<SshAuthentication> {
    if get(SecretKind::SshPassword, id)?.is_none() {
        return Ok(SshAuthentication {
            arguments: vec!["-o".into(), "BatchMode=yes".into()],
            environment: vec![],
            authorization: None,
        });
    }
    let mut bytes = [0u8; 16];
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let h = bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    let auth = format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    );
    write(AUTH, &auth, &account(id)?, false)?;
    Ok(SshAuthentication {
        arguments: vec![
            "-o".into(),
            "BatchMode=no".into(),
            "-o".into(),
            "NumberOfPasswordPrompts=1".into(),
        ],
        environment: vec![
            (
                "SSH_ASKPASS".into(),
                std::env::current_exe()?.to_string_lossy().into_owned(),
            ),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            ("HERDRM_SSH_ASKPASS".into(), "1".into()),
            ("HERDRM_SSH_AUTHORIZATION_ID".into(), auth.clone()),
        ],
        authorization: Some(auth),
    })
}
/// Call before creating windows. The password is emitted only to OpenSSH's askpass pipe.
pub fn handle_askpass() -> Result<bool> {
    if std::env::var("HERDRM_SSH_ASKPASS").as_deref() != Ok("1") {
        return Ok(false);
    }
    let id = std::env::var("HERDRM_SSH_AUTHORIZATION_ID")?;
    let device = read(AUTH, &id)?.context("SSH授权已失效")?;
    remove(AUTH, &id)?;
    let password = get(SecretKind::SshPassword, &device)?.context("未保存SSH密码")?;
    use std::io::Write;
    std::io::stdout().lock().write_all(password.as_bytes())?;
    Ok(true)
}
