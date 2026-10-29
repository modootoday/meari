//! Windows GL adapter: hidden host-owned WGL bootstrap. No desktop placement or display configuration changes.
use crate::port::GlContextHost;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version},
    display::{Display, DisplayApiPreference, GetGlDisplay},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::{
    ffi::CString,
    num::{NonZeroIsize, NonZeroU32},
};
use windows::{
    Win32::{
        Foundation::HWND, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
    },
    core::w,
};

struct HiddenWindow(HWND);
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn owned_window_count() -> usize {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}
impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe {
            if DestroyWindow(self.0).is_ok() {
                LIVE.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

/// Opens the hidden WGL bootstrap context.
pub fn bootstrap() -> Result<Box<dyn GlContextHost>, String> {
    Ok(Box::new(Platform::new()?))
}

struct Platform {
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    display: Display,
    _window: HiddenWindow,
}
impl Platform {
    fn new() -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let window = HiddenWindow(
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("Meari GL bootstrap"),
                    WS_POPUP,
                    0,
                    0,
                    16,
                    16,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .map_err(|e| e.to_string())?,
            );
            LIVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut handle = Win32WindowHandle::new(
                NonZeroIsize::new(window.0.0 as isize).ok_or("null hidden window")?,
            );
            handle.hinstance = NonZeroIsize::new(instance.0 as isize);
            let raw = RawWindowHandle::Win32(handle);
            let display = Display::new(
                RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                DisplayApiPreference::Wgl(Some(raw)),
            )
            .map_err(|e| e.to_string())?;
            let config = display
                .find_configs(ConfigTemplateBuilder::new().with_alpha_size(8).build())
                .map_err(|e| e.to_string())?
                .min_by_key(|c| c.num_samples())
                .ok_or("no WGL configuration")?;
            let context = display
                .create_context(
                    &config,
                    &ContextAttributesBuilder::new()
                        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
                        .build(Some(raw)),
                )
                .map_err(|e| e.to_string())?;
            let attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
                raw,
                NonZeroU32::new(16).unwrap(),
                NonZeroU32::new(16).unwrap(),
            );
            let surface = config
                .display()
                .create_window_surface(&config, &attributes)
                .map_err(|e| e.to_string())?;
            let context = context.make_current(&surface).map_err(|e| e.to_string())?;
            Ok(Self {
                surface,
                context,
                display,
                _window: window,
            })
        }
    }
}

impl GlContextHost for Platform {
    fn make_current(&self) -> Result<(), String> {
        self.context
            .make_current(&self.surface)
            .map_err(|e| e.to_string())
    }
    fn address(&self, name: &str) -> *const std::ffi::c_void {
        CString::new(name).map_or(std::ptr::null(), |name| {
            self.display.get_proc_address(&name)
        })
    }
}
