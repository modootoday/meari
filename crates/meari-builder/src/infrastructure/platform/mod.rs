//! OS-specific filesystem adapters for build inputs (cfg isolated here).
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(crate) use unix::is_link;
#[cfg(windows)]
pub(crate) use windows::is_link;
