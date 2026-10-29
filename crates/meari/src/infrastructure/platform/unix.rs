use std::{fs, io, os::unix::fs::PermissionsExt, path::Path};

/// No setuid/setgid/sticky bits, and executables carry an execute bit.
pub(crate) fn executable_mode_matches(path: &Path, executable: bool) -> io::Result<bool> {
    let mode = fs::metadata(path)?.permissions().mode();
    Ok(!(mode & 0o7000 != 0 || (executable && mode & 0o111 == 0)))
}

pub(crate) fn mark_executable(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}
