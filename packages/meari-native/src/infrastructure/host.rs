use meari::{
    application::screen::measured_viewport,
    domain::host::{HostCapabilities, HostEnvironment, HostTarget, Support, VisualViewport},
};
use winit::window::{Theme, Window};

/// Observes client bounds in CSS pixels, without changing desktop DPI or monitor state.
pub fn snapshot(window: &dyn Window) -> Result<HostEnvironment, String> {
    snapshot_with_zoom(window, 1.0)
}

pub fn snapshot_with_zoom(
    window: &dyn Window,
    layout_zoom: f64,
) -> Result<HostEnvironment, String> {
    let device_scale = window.scale_factor();
    if !device_scale.is_finite()
        || device_scale <= 0.0
        || !layout_zoom.is_finite()
        || layout_zoom <= 0.0
    {
        return Err("invalid native scale factor".into());
    }
    let size = window.surface_size();
    let width = f64::from(size.width) / device_scale / layout_zoom;
    let height = f64::from(size.height) / device_scale / layout_zoom;
    let dark = window.theme().ok_or("native color scheme is unavailable")? == Theme::Dark;
    let host = HostEnvironment {
        target: HostTarget::Native,
        layout: measured_viewport(width, height, dark)?,
        visual: VisualViewport {
            width,
            height,
            offset_x: 0.0,
            offset_y: 0.0,
            scale: 1.0,
        },
        device_scale,
        layout_zoom: Some(layout_zoom),
        safe_area: None,
        capabilities: HostCapabilities {
            client_resize: if window.is_resizable() {
                Support::Supported
            } else {
                Support::Unsupported
            },
            preview_resize: Support::Unsupported,
            reduced_motion: Support::Unknown,
            contrast: Support::Unknown,
            pointer_hover: Support::Unknown,
            accessibility: Support::Unknown,
        },
    };
    host.validate()?;
    Ok(host)
}

/// None means the caller must wait for SurfaceResized before acknowledging the ticket.
pub fn request_resize(
    window: &dyn Window,
    request: &meari::application::screen::ScreenRequest,
) -> Result<Option<HostEnvironment>, String> {
    request_resize_with_zoom(window, request, 1.0)
}

pub fn request_resize_with_zoom(
    window: &dyn Window,
    request: &meari::application::screen::ScreenRequest,
    layout_zoom: f64,
) -> Result<Option<HostEnvironment>, String> {
    request.validate_for(&snapshot_with_zoom(window, layout_zoom)?)?;
    let width = f64::from(request.viewport.width) * layout_zoom;
    let height = f64::from(request.viewport.height) * layout_zoom;
    let scale = window.scale_factor();
    if !(1.0..=8192.0).contains(&(width * scale)) || !(1.0..=8192.0).contains(&(height * scale)) {
        return Err("native resize exceeds physical rendering budget".into());
    }
    let size = winit::dpi::LogicalSize::new(width, height);
    match window.request_surface_size(size.into()) {
        Some(_) => snapshot_with_zoom(window, layout_zoom).map(Some),
        None => Ok(None),
    }
}
