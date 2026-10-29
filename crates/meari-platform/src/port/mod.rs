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

/// Implemented by one adapter per OS.
pub trait PlatformHost {
    fn os_family(&self) -> OsFamily;
    /// Host fallbacks for the CSS generic families on this OS.
    fn font_fallbacks(&self) -> HostFallbacks;
}
