use super::viewport::Viewport;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostTarget {
    Native,
    Web,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostCapabilities {
    pub client_resize: Support,
    pub preview_resize: Support,
    pub reduced_motion: Support,
    pub contrast: Support,
    pub pointer_hover: Support,
    pub accessibility: Support,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualViewport {
    pub width: f64,
    pub height: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub scale: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// Observed host values; no defaults imply actual device capabilities.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostEnvironment {
    pub target: HostTarget,
    pub layout: Viewport,
    pub visual: VisualViewport,
    pub device_scale: f64,
    /// Native document zoom when observed; browser page zoom is already reflected in DPR.
    pub layout_zoom: Option<f64>,
    pub safe_area: Option<Insets>,
    pub capabilities: HostCapabilities,
}

impl HostEnvironment {
    pub fn validate(&self) -> Result<(), String> {
        self.layout.validate()?;
        if self
            .layout_zoom
            .is_some_and(|zoom| !zoom.is_finite() || zoom <= 0.0)
        {
            return Err("layout zoom must be finite and positive when observed".into());
        }
        for value in [
            self.device_scale,
            self.visual.scale,
            self.visual.width,
            self.visual.height,
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err("host scale and visual dimensions must be finite and positive".into());
            }
        }
        if !self.visual.offset_x.is_finite() || !self.visual.offset_y.is_finite() {
            return Err("visual viewport offsets must be finite".into());
        }
        if let Some(insets) = &self.safe_area {
            for value in [insets.top, insets.right, insets.bottom, insets.left] {
                if !value.is_finite() || value < 0.0 {
                    return Err("safe area insets must be finite and nonnegative".into());
                }
            }
        }
        Ok(())
    }
}
