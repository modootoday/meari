use crate::domain::WindowTelemetry;
use anyhow::{Result, ensure};
use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::GetCurrentProcess,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
        },
    },
    core::BOOL,
};

struct Search {
    pid: u32,
    window: HWND,
    area: i64,
}
unsafe extern "system" fn find_window(window: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows invokes synchronously with our live Search; all HWNDs
    // originate from the OS. Only this process's visible window is selected.
    unsafe {
        let search = &mut *(parameter.0 as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == search.pid && IsWindowVisible(window).as_bool() {
            let mut rect = RECT::default();
            if GetWindowRect(window, &mut rect).is_ok() {
                let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
                if area > search.area {
                    search.window = window;
                    search.area = area;
                }
            }
        }
    }
    BOOL(1)
}

pub fn telemetry() -> Result<WindowTelemetry> {
    let mut search = Search {
        pid: std::process::id(),
        window: HWND::default(),
        area: 0,
    };
    // SAFETY: all output pointers refer to initialized, correctly sized local
    // structs. We query this process/window only and do not alter display state.
    unsafe {
        EnumWindows(
            Some(find_window),
            LPARAM((&mut search as *mut Search) as isize),
        )?;
        ensure!(
            !search.window.is_invalid() && search.area > 10000,
            "owned render window not found"
        );
        let mut rect = RECT::default();
        GetWindowRect(search.window, &mut rect)?;
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        ensure!(
            GetMonitorInfoW(
                MonitorFromWindow(search.window, MONITOR_DEFAULTTONEAREST),
                &mut monitor
            )
            .as_bool(),
            "monitor query failed"
        );
        let mut memory = PROCESS_MEMORY_COUNTERS {
            cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut memory,
            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )?;
        Ok(WindowTelemetry {
            pid: search.pid,
            hwnd: search.window.0 as u64,
            bounds: [rect.left, rect.top, rect.right, rect.bottom],
            monitor: [
                monitor.rcMonitor.left,
                monitor.rcMonitor.top,
                monitor.rcMonitor.right,
                monitor.rcMonitor.bottom,
            ],
            primary: monitor.dwFlags & 1 != 0,
            working_set_bytes: memory.WorkingSetSize as u64,
        })
    }
}

pub fn validate_virtual_window(info: &WindowTelemetry) -> Result<()> {
    ensure!(
        !info.primary && info.hwnd != 0,
        "verification requires an owned nonprimary-monitor window"
    );
    ensure!(
        info.bounds[0] >= info.monitor[0]
            && info.bounds[1] >= info.monitor[1]
            && info.bounds[2] <= info.monitor[2]
            && info.bounds[3] <= info.monitor[3],
        "window outside monitor"
    );
    Ok(())
}
