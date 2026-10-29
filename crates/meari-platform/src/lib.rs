//! Meari platform facade: the port ([`PlatformHost`]), the per-OS adapters, and [`CurrentOs`],
//! the single place that selects an adapter with `cfg` (mirrors kinekino's
//! `kinekino-platform::CurrentOs`).
pub mod adapter;
pub mod port;

pub use port::{HostFallbacks, OsFamily, PlatformHost};

#[cfg(windows)]
type Live = adapter::windows::WindowsHost;
#[cfg(target_os = "macos")]
type Live = adapter::macos::MacOsHost;
#[cfg(not(any(windows, target_os = "macos")))]
type Live = adapter::unix::UnixHost;

/// [`PlatformHost`] backed by the adapter for the OS this binary was built for.
#[derive(Debug, Default, Clone, Copy)]
pub struct CurrentOs;

impl PlatformHost for CurrentOs {
    fn os_family(&self) -> OsFamily {
        Live::default().os_family()
    }
    fn font_fallbacks(&self) -> HostFallbacks {
        Live::default().font_fallbacks()
    }
}

/// Returns the adapter for the OS this binary was built for.
pub fn current() -> CurrentOs {
    CurrentOs
}

/// Every adapter, for checks that must hold on all hosts (e.g. CSS stacks naming each family).
pub fn all_adapters() -> [&'static dyn PlatformHost; 3] {
    [
        &adapter::macos::MacOsHost,
        &adapter::windows::WindowsHost,
        &adapter::unix::UnixHost,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_os_matches_the_build_target() {
        let expected = if cfg!(windows) {
            OsFamily::Windows
        } else if cfg!(target_os = "macos") {
            OsFamily::MacOs
        } else {
            OsFamily::Unix
        };
        assert_eq!(current().os_family(), expected);
    }

    #[test]
    fn every_adapter_names_sans_and_serif_fallbacks() {
        for host in all_adapters() {
            let fonts = host.font_fallbacks();
            assert!(!fonts.sans.is_empty() && !fonts.serif.is_empty());
        }
    }
}
