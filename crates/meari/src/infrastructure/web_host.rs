use crate::{
    application::screen::measured_viewport,
    domain::host::{HostCapabilities, HostEnvironment, HostTarget, Support, VisualViewport},
};
pub mod observer;

/// Reads browser values; it never resizes the browser window or invents safe-area data.
pub fn snapshot() -> Result<HostEnvironment, String> {
    let window = web_sys::window().ok_or("browser Window is unavailable")?;
    let visual = window
        .visual_viewport()
        .ok_or("VisualViewport is unavailable")?;
    let number = |value: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
        value
            .map_err(|error| format!("browser viewport read failed: {error:?}"))?
            .as_f64()
            .ok_or_else(|| "browser viewport is not numeric".to_owned())
    };
    let dark = window
        .match_media("(prefers-color-scheme: dark)")
        .map_err(|error| format!("color scheme query failed: {error:?}"))?
        .ok_or("color scheme query unavailable")?
        .matches();
    let host = HostEnvironment {
        target: HostTarget::Web,
        layout: measured_viewport(
            number(window.inner_width())?,
            number(window.inner_height())?,
            dark,
        )?,
        visual: VisualViewport {
            width: visual.width(),
            height: visual.height(),
            offset_x: visual.offset_left(),
            offset_y: visual.offset_top(),
            scale: visual.scale(),
        },
        device_scale: window.device_pixel_ratio(),
        layout_zoom: None,
        safe_area: None,
        capabilities: HostCapabilities {
            client_resize: Support::Unsupported,
            preview_resize: Support::Unknown,
            reduced_motion: Support::Unknown,
            contrast: Support::Unknown,
            pointer_hover: Support::Unknown,
            accessibility: Support::Unknown,
        },
    };
    host.validate()?;
    Ok(host)
}
