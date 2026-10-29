//! Win32 window-state probe (UI thread only; no handle is stored).
use crate::port::{RawWindowHandle, WindowState};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::WindowsAndMessaging::{
        GetWindowInfo, GetWindowRect, IsIconic, IsWindow, IsZoomed, WINDOWINFO, WS_CAPTION,
        WS_THICKFRAME,
    },
};

pub(crate) fn probe(handle: RawWindowHandle) -> Option<WindowState> {
    let RawWindowHandle::Win32(raw) = handle else {
        return None;
    };
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    // Original UI-thread window borrow only. No OS handle is stored or exported.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let mut rect = RECT::default();
        let mut info = WINDOWINFO {
            cbSize: std::mem::size_of::<WINDOWINFO>() as u32,
            ..Default::default()
        };
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let borderless_monitor_coverage = if GetWindowRect(hwnd, &mut rect).is_ok()
            && GetWindowInfo(hwnd, &mut info).is_ok()
            && GetMonitorInfoW(
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            )
            .as_bool()
        {
            Some(
                info.dwStyle.0 & (WS_CAPTION.0 | WS_THICKFRAME.0) == 0
                    && rect.left == monitor.rcMonitor.left
                    && rect.top == monitor.rcMonitor.top
                    && rect.right == monitor.rcMonitor.right
                    && rect.bottom == monitor.rcMonitor.bottom,
            )
        } else {
            None
        };
        Some(WindowState {
            minimized: Some(IsIconic(hwnd).as_bool()),
            maximized: Some(IsZoomed(hwnd).as_bool()),
            borderless_monitor_coverage,
        })
    }
}
