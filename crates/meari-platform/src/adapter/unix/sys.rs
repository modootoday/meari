//! Unix (Linux, BSD, macOS) OS policy: POSIX mode bits and symlinks; no client-area readback
//! or UI Automation probe.
use crate::port::{ClientArea, ClientAreaError, OsPolicy, RawWindowHandle};
use std::{fs, io, os::unix::fs::PermissionsExt, path::Path};

#[derive(Debug, Default, Clone, Copy)]
pub struct UnixOs;

impl OsPolicy for UnixOs {
    /// No setuid/setgid/sticky bits, and executables carry an execute bit.
    fn executable_mode_matches(&self, path: &Path, executable: bool) -> io::Result<bool> {
        let mode = fs::metadata(path)?.permissions().mode();
        Ok(!(mode & 0o7000 != 0 || (executable && mode & 0o111 == 0)))
    }
    fn mark_executable(&self, path: &Path) -> io::Result<()> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
    fn is_link(&self, metadata: &fs::Metadata) -> bool {
        metadata.file_type().is_symlink()
    }
    fn supports_client_area(&self) -> bool {
        false
    }
    fn client_area(&self, _handle: RawWindowHandle) -> Result<ClientArea, ClientAreaError> {
        Err(ClientAreaError::Unsupported)
    }
    fn ui_automation_probe(
        &self,
        _pid: u32,
        _tabs: bool,
    ) -> Option<Result<serde_json::Value, String>> {
        None
    }
    fn request_terminate(&self, pid: u32) {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
}
