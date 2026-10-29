//! Win32 window-state probe (UI thread only; no handle is stored).
use crate::application::window_actions::{ActionCapabilities, WindowObservation};
use winit::window::Window;

pub(crate) fn observation(window: &dyn Window) -> (ActionCapabilities, WindowObservation) {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::{
            GetWindowInfo, GetWindowRect, IsIconic, IsWindow, IsZoomed, WINDOWINFO, WS_CAPTION,
            WS_THICKFRAME,
        },
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let unknown = (ActionCapabilities::default(), WindowObservation::default());
    let Ok(handle) = window.window_handle() else {
        return unknown;
    };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else {
        return unknown;
    };
    let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
    // Original UI-thread Window borrow only. No OS handle is stored or exported.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return unknown;
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
        (
            ActionCapabilities {
                minimize: true,
                maximize: true,
                borderless_fullscreen: borderless_monitor_coverage.is_some(),
            },
            WindowObservation {
                minimized: Some(IsIconic(hwnd).as_bool()),
                maximized: Some(IsZoomed(hwnd).as_bool()),
                borderless_monitor_coverage,
            },
        )
    }
}
