//! Meari platform facade: the port ([`PlatformHost`]), the per-OS adapters, and [`CurrentOs`],
//! the single place that selects an adapter with `cfg` (mirrors kinekino's
//! `kinekino-platform::CurrentOs`).
pub mod adapter;
pub mod port;

pub use port::{
    ClientArea, ClientAreaError, GlContextHost, HostFallbacks, OsFamily, OsPolicy, PlatformHost,
    WindowState,
};

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

#[cfg(all(feature = "gl", not(windows)))]
use adapter::portable::gl_bootstrap as live_gl_bootstrap;
#[cfg(all(feature = "gl", windows))]
use adapter::windows::gl::{bootstrap as live_gl_bootstrap, owned_window_count as live_gl_windows};

/// Opens the GL bootstrap context for this OS (`Err` where no adapter exists).
#[cfg(feature = "gl")]
pub fn gl_bootstrap() -> Result<Box<dyn GlContextHost>, String> {
    live_gl_bootstrap()
}

/// Hidden windows currently owned by the GL bootstrap adapter (0 where there is none).
#[cfg(feature = "gl")]
pub fn gl_owned_window_count() -> usize {
    #[cfg(windows)]
    {
        live_gl_windows()
    }
    #[cfg(not(windows))]
    {
        0
    }
}

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
    fn window_state(&self, handle: port::RawWindowHandle) -> Option<WindowState> {
        LiveOs::default().window_state(handle)
    }
    fn request_terminate(&self, pid: u32) {
        LiveOs::default().request_terminate(pid)
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

    #[cfg(feature = "gl")]
    #[test]
    fn gl_bootstrap_without_an_adapter_is_an_error_not_a_cpu_fallback() {
        if !cfg!(windows) {
            assert!(gl_bootstrap().is_err());
            assert_eq!(gl_owned_window_count(), 0);
        }
    }
}
