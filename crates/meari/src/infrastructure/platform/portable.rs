use std::{io, path::Path};

/// Platforms without POSIX mode bits have nothing to verify.
pub(crate) fn executable_mode_matches(_path: &Path, _executable: bool) -> io::Result<bool> {
    Ok(true)
}

pub(crate) fn mark_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}
