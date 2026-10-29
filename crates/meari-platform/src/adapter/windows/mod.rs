//! Windows adapter. Pure data, so it compiles on every host and its tables are checked on all
//! of them; only [`crate::CurrentOs`] decides which adapter is live.
use crate::port::{HostFallbacks, OsFamily, PlatformHost};

pub const FONT_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Segoe UI", "Malgun Gothic", "Yu Gothic UI", "Arial"],
    serif: &["Batang", "Yu Mincho", "Times New Roman"],
};

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsHost;

impl PlatformHost for WindowsHost {
    fn os_family(&self) -> OsFamily {
        OsFamily::Windows
    }
    fn font_fallbacks(&self) -> HostFallbacks {
        FONT_FALLBACKS
    }
}

#[cfg(all(windows, feature = "gl"))]
pub mod gl;
#[cfg(windows)]
pub mod sys;
#[cfg(windows)]
mod uia_probe;
#[cfg(windows)]
mod window_state;
