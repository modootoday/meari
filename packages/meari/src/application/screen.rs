use crate::domain::{
    host::{HostEnvironment, HostTarget, Support},
    viewport::Viewport,
};
use serde::{Deserialize, Serialize};
pub mod resize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeSurface {
    NativeClient,
    WebPreview,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenRequest {
    pub surface: ResizeSurface,
    pub viewport: Viewport,
}

/// CSS measurement policy shared by renderers; it does not resize a host window.
pub fn measured_viewport(width: f64, height: f64, dark: bool) -> Result<Viewport, String> {
    let dimension = |value: f64| -> Result<u32, String> {
        if !value.is_finite() || !(1.0..=8192.0).contains(&value) {
            return Err("CSS screen dimensions must be finite and in 1..=8192".into());
        }
        Ok(value.round() as u32)
    };
    Ok(Viewport {
        width: dimension(width)?,
        height: dimension(height)?,
        dark,
    })
}

impl ScreenRequest {
    pub fn validate_for(&self, host: &HostEnvironment) -> Result<(), String> {
        host.validate()?;
        self.viewport.validate()?;
        let support = match (host.target, self.surface) {
            (HostTarget::Native, ResizeSurface::NativeClient) => host.capabilities.client_resize,
            (HostTarget::Web, ResizeSurface::WebPreview) => host.capabilities.preview_resize,
            _ => return Err("resize surface does not match host target".into()),
        };
        if support != Support::Supported {
            return Err("host resize support is not confirmed".into());
        }
        Ok(())
    }
}
