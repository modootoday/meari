//! Meari platform facade: the port ([`PlatformHost`]), the per-OS adapters, and [`CurrentOs`],
//! the single place that selects an adapter with `cfg` (mirrors kinekino's
//! `kinekino-platform::CurrentOs`).
pub mod adapter;
pub mod port;

pub use port::{ClientArea, ClientAreaError, HostFallbacks, OsFamily, OsPolicy, PlatformHost};

#[cfg(windows)]
type Live = adapter::windows::WindowsHost;
#[cfg(target_os = "macos")]
type Live = adapter::macos::MacOsHost;
#[cfg(not(any(windows, target_os = "macos")))]
type Live = adapter::unix::UnixHost;

#[cfg(unix)]
type LiveOs = adapter::unix::sys::UnixOs;
#[cfg(windows)]
type LiveOs = adapter::windows::sys::WindowsOs;
#[cfg(not(any(unix, windows)))]
type LiveOs = adapter::portable::PortableOs;

#[cfg(all(feature = "window", not(windows)))]
use adapter::portable::prepare_event_loop as live_prepare_event_loop;
#[cfg(all(feature = "window", windows))]
use adapter::windows::sys::prepare_event_loop as live_prepare_event_loop;

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

impl OsPolicy for CurrentOs {
    fn executable_mode_matches(
        &self,
        path: &std::path::Path,
        executable: bool,
    ) -> std::io::Result<bool> {
        LiveOs::default().executable_mode_matches(path, executable)
    }
    fn mark_executable(&self, path: &std::path::Path) -> std::io::Result<()> {
        LiveOs::default().mark_executable(path)
    }
    fn is_link(&self, metadata: &std::fs::Metadata) -> bool {
        LiveOs::default().is_link(metadata)
    }
    fn supports_client_area(&self) -> bool {
        LiveOs::default().supports_client_area()
    }
    fn client_area(&self, handle: port::RawWindowHandle) -> Result<ClientArea, ClientAreaError> {
        LiveOs::default().client_area(handle)
    }
    fn ui_automation_probe(
        &self,
        pid: u32,
        tabs: bool,
    ) -> Option<Result<serde_json::Value, String>> {
        LiveOs::default().ui_automation_probe(pid, tabs)
    }
}

/// Lets the event loop run on a non-main thread where the OS backend requires opting in
/// (Windows: `with_any_thread`); no-op elsewhere.
#[cfg(feature = "window")]
pub fn prepare_event_loop(builder: &mut winit::event_loop::EventLoopBuilder) {
    live_prepare_event_loop(builder);
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
