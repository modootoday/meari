//! Windows OS policy: reparse points, Win32 client-area readback, UI Automation probe.
use crate::port::{ClientArea, ClientAreaError, OsPolicy, RawWindowHandle};
use std::{fs, io, os::windows::fs::MetadataExt, path::Path};

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsOs;

impl OsPolicy for WindowsOs {
    fn executable_mode_matches(&self, _path: &Path, _executable: bool) -> io::Result<bool> {
        Ok(true) // no POSIX mode bits to verify
    }
    fn mark_executable(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
    /// FILE_ATTRIBUTE_REPARSE_POINT covers symlinks and junctions.
    fn is_link(&self, metadata: &fs::Metadata) -> bool {
        metadata.file_attributes() & 0x400 != 0
    }
    fn supports_client_area(&self) -> bool {
        true
    }
    fn client_area(&self, handle: RawWindowHandle) -> Result<ClientArea, ClientAreaError> {
        use windows::Win32::{
            Foundation::{HWND, POINT, RECT},
            Graphics::Gdi::ClientToScreen,
            UI::WindowsAndMessaging::GetClientRect,
        };
        let RawWindowHandle::Win32(raw) = handle else {
            return Err(ClientAreaError::Unsupported);
        };
        let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        // The caller's window borrow keeps the owner alive; no raw handle is retained.
        unsafe {
            GetClientRect(hwnd, &mut rect)
                .map_err(|e| ClientAreaError::OsReadback(e.to_string()))?;
            ClientToScreen(hwnd, &mut origin)
                .ok()
                .map_err(|e| ClientAreaError::OsReadback(e.to_string()))?;
        }
        Ok(ClientArea {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            screen_origin: (origin.x, origin.y),
        })
    }
    fn ui_automation_probe(
        &self,
        pid: u32,
        tabs: bool,
    ) -> Option<Result<serde_json::Value, String>> {
        Some(if tabs {
            super::uia_probe::verify_tabs(pid)
        } else {
            super::uia_probe::verify(pid)
        })
    }
}

#[cfg(feature = "window")]
pub fn prepare_event_loop(builder: &mut winit::event_loop::EventLoopBuilder) {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    builder.with_any_thread(true);
}
