//! Adapter for targets that are neither Unix nor Windows (e.g. wasm32): std-only behaviour.
use crate::port::{ClientArea, ClientAreaError, OsPolicy, RawWindowHandle};
use std::{fs, io, path::Path};

#[derive(Debug, Default, Clone, Copy)]
pub struct PortableOs;

impl OsPolicy for PortableOs {
    fn executable_mode_matches(&self, _path: &Path, _executable: bool) -> io::Result<bool> {
        Ok(true) // no POSIX mode bits to verify
    }
    fn mark_executable(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
    fn is_link(&self, metadata: &fs::Metadata) -> bool {
        metadata.file_type().is_symlink()
    }
    fn supports_client_area(&self) -> bool {
        false
    }
    fn client_area(&self, _handle: RawWindowHandle) -> Result<ClientArea, ClientAreaError> {
        Err(ClientAreaError::Unsupported)
    }
    fn ui_automation_probe(
        &self,
        _pid: u32,
        _tabs: bool,
    ) -> Option<Result<serde_json::Value, String>> {
        None
    }
}

/// No event-loop thread restriction to lift on this OS.
#[cfg(feature = "window")]
pub fn prepare_event_loop(_builder: &mut winit::event_loop::EventLoopBuilder) {}
