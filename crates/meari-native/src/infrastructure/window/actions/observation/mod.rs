//! Window-state observation port; the OS adapter is selected here and nowhere else.
#[cfg(not(windows))]
mod portable;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub(super) use portable::observation;
#[cfg(windows)]
pub(super) use windows::observation;
