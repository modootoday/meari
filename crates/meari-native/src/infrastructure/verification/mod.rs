//! Grouped submodules; re-exported by the parent to keep their paths.

pub mod artifacts;
#[cfg(any(feature = "window", test))]
pub mod checked_scene;
#[cfg(feature = "window")]
pub mod checked_window;
#[cfg(windows)]
pub mod uia_probe;
