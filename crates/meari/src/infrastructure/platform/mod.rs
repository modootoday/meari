//! OS-specific file-permission adapters for the artifact store.
//! Every `cfg(unix)`/`cfg(windows)` branch of the store lives here, behind one port.
#[cfg(not(unix))]
mod portable;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub(crate) use portable::{executable_mode_matches, mark_executable};
#[cfg(unix)]
pub(crate) use unix::{executable_mode_matches, mark_executable};
