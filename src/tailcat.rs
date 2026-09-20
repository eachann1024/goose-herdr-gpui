//! Embedded WireGuard/DERP bridge. Credentials never enter argv or the environment.
use anyhow::{Context, Result, bail};
use std::{
    ffi::CString,
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

unsafe extern "C" {
    fn herdr_tailcat_start(token: *const libc::c_char, path: *const libc::c_char) -> libc::c_int;
    fn herdr_tailcat_has_error(path: *const libc::c_char) -> libc::c_int;
    fn herdr_tailcat_stop(path: *const libc::c_char);
}

static NEXT_BRIDGE: AtomicU64 = AtomicU64::new(0);

/// Keep this guard alive for RPC, events and terminal sessions using its socket.
#[derive(Debug)]
pub struct TailcatBridge {
    path: PathBuf,
    c_path: CString,
    directory: PathBuf,
}

impl TailcatBridge {
    /// Starts local listeners; the caller must complete RPC handshake before marking connected.
    pub fn connect(token: &str) -> Result<Self> {
        if token.trim().is_empty() || token.len() > 64 * 1024 {
            bail!("Tailcat 连接令牌为空或过长");
        }
        let token = CString::new(token).context("Tailcat 令牌包含无效字符")?;
        let directory = loop {
            let n = NEXT_BRIDGE.fetch_add(1, Ordering::Relaxed);
            let candidate = PathBuf::from(format!("/tmp/herdr-gpui-tc-{}-{n}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e).context("无法创建 Tailcat 私有套接字目录"),
            }
        };
        let path = directory.join("rpc.sock");
        let c_path = CString::new(path.as_os_str().as_encoded_bytes())?;
        let bridge = Self {
            path,
            c_path,
            directory,
        };
        if unsafe { herdr_tailcat_start(token.as_ptr(), bridge.c_path.as_ptr()) } == 0 {
            // Do not relay runtime errors: malformed-token errors can contain credentials.
            bail!("Tailcat 隧道启动失败，请检查连接令牌");
        }
        Ok(bridge)
    }

    pub fn socket_path(&self) -> &Path {
        &self.path
    }

    pub fn recent_error(&self) -> Option<String> {
        (unsafe { herdr_tailcat_has_error(self.c_path.as_ptr()) } != 0)
            .then(|| "Tailcat 远端连接失败，请检查令牌和网络".to_owned())
    }
}

impl Drop for TailcatBridge {
    fn drop(&mut self) {
        unsafe { herdr_tailcat_stop(self.c_path.as_ptr()) };
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(self.directory.join("rpc-client.sock"));
        let _ = fs::remove_dir(&self.directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_tokens_without_starting_bridge() {
        assert!(TailcatBridge::connect("").is_err());
        assert!(TailcatBridge::connect("abc\0def").is_err());
        assert!(TailcatBridge::connect(&"x".repeat(65_537)).is_err());
    }
}
