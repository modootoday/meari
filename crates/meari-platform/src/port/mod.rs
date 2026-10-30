//! Platform port: what Meari needs to know about the host OS. Implemented by the adapters in
//! [`crate::adapter`]; selected once by [`crate::CurrentOs`].

/// OS family reported by an adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsFamily {
    Windows,
    MacOs,
    Unix,
}

/// Host font families tried after the bundled faces (names only; the engine resolves them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFallbacks {
    pub sans: &'static [&'static str],
    pub serif: &'static [&'static str],
}

/// Where Chromium's text-control `avgCharWidth` comes from on this OS.
///
/// Skia's FreeType/Fontations backends report OS/2 `xAvgCharWidth`; Skia's DirectWrite
/// backend (Windows) leaves `fAvgCharWidth` at 0, so Blink falls back to the advance of `x`
/// (`SimpleFontData::PlatformInit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvgCharWidthSource {
    Os2XAvgCharWidth,
    XGlyphAdvance,
}

/// Implemented by one adapter per OS.
pub trait PlatformHost {
    fn os_family(&self) -> OsFamily;
    /// Host fallbacks for the CSS generic families on this OS.
    fn font_fallbacks(&self) -> HostFallbacks;
    /// Source of the average char width that sizes `size=20` text inputs.
    fn avg_char_width_source(&self) -> AvgCharWidthSource {
        AvgCharWidthSource::Os2XAvgCharWidth
    }
}

pub use raw_window_handle::RawWindowHandle;

/// Raw client rectangle and its screen origin, as the OS reports them (unvalidated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub screen_origin: (i32, i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientAreaError {
    /// This OS (or this handle kind) has no client-area readback adapter.
    Unsupported,
    OsReadback(String),
}

/// Native window state read back from the OS (`None` fields are unknown).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub minimized: Option<bool>,
    pub maximized: Option<bool>,
    /// `Some(true)` when an undecorated window covers its whole monitor; `None` if unreadable.
    pub borderless_monitor_coverage: Option<bool>,
}

/// OS operations; implemented by the live adapter (`adapter::{unix,windows}::sys`,
/// `adapter::portable`) and reached through [`crate::CurrentOs`].
pub trait OsPolicy {
    /// Artifact-store check: no setuid/setgid/sticky, executables executable (no-op w/o modes).
    fn executable_mode_matches(
        &self,
        path: &std::path::Path,
        executable: bool,
    ) -> std::io::Result<bool>;
    /// Marks an artifact executable (no-op without POSIX mode bits).
    fn mark_executable(&self, path: &std::path::Path) -> std::io::Result<()>;
    /// True for symlinks (Unix) or any reparse point incl. junctions (Windows).
    fn is_link(&self, metadata: &std::fs::Metadata) -> bool;
    /// Whether [`Self::client_area`] can succeed on this OS.
    fn supports_client_area(&self) -> bool;
    /// Client rectangle and screen origin of a native window handle.
    fn client_area(&self, handle: RawWindowHandle) -> Result<ClientArea, ClientAreaError>;
    /// UI Automation verification of process `pid` (`tabs` selects the tab probe);
    /// `None` where the OS has no UI Automation adapter.
    fn ui_automation_probe(
        &self,
        pid: u32,
        tabs: bool,
    ) -> Option<Result<serde_json::Value, String>>;
    /// Window-state probe of a live native window (UI thread only; the handle is not stored).
    /// `None` where this OS (or this handle kind) has no window-state adapter.
    fn window_state(&self, _handle: RawWindowHandle) -> Option<WindowState> {
        None
    }
    /// Asks process `pid` to terminate gracefully (Unix `SIGTERM`); best effort, no-op where
    /// the OS has no graceful request (callers still kill after their grace period).
    fn request_terminate(&self, _pid: u32) {}
}

/// Current GL context with a proc-address loader, owned by the OS adapter (GL feature).
pub trait GlContextHost {
    /// Makes the bootstrap context current on this thread.
    fn make_current(&self) -> Result<(), String>;
    /// GL function address, or null when unknown.
    fn address(&self, name: &str) -> *const std::ffi::c_void;
}
