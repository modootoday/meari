//! macOS adapter. Pure data, so it compiles on every host and its tables are checked on all
//! of them; only [`crate::CurrentOs`] decides which adapter is live.
use crate::port::{HostFallbacks, OsFamily, PlatformHost};

pub const FONT_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Apple SD Gothic Neo", "Hiragino Sans"],
    serif: &["AppleMyungjo", "Hiragino Mincho ProN", "Times New Roman"],
};

#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsHost;

impl PlatformHost for MacOsHost {
    fn os_family(&self) -> OsFamily {
        OsFamily::MacOs
    }
    fn font_fallbacks(&self) -> HostFallbacks {
        FONT_FALLBACKS
    }
}
