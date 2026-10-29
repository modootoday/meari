//! Linux/BSD (fontconfig names) adapter. Pure data, so it compiles on every host and its tables are checked on all
//! of them; only [`crate::CurrentOs`] decides which adapter is live.
use crate::port::{HostFallbacks, OsFamily, PlatformHost};

pub const FONT_FALLBACKS: HostFallbacks = HostFallbacks {
    sans: &["Noto Sans CJK KR", "Noto Sans KR"],
    serif: &["Noto Serif CJK KR", "Noto Serif KR"],
};

#[derive(Debug, Default, Clone, Copy)]
pub struct UnixHost;

impl PlatformHost for UnixHost {
    fn os_family(&self) -> OsFamily {
        OsFamily::Unix
    }
    fn font_fallbacks(&self) -> HostFallbacks {
        FONT_FALLBACKS
    }
}
