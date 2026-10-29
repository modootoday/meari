//! Observed geometry is metadata, never a surface creation or destruction acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalClientRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl PhysicalClientRect {
    pub fn validate(self) -> Result<(), SurfaceError> {
        if [self.x, self.y, self.width, self.height]
            .into_iter()
            .any(|value| !value.is_finite())
            || self.width < 0.0
            || self.height < 0.0
            || !(self.x + self.width).is_finite()
            || !(self.y + self.height).is_finite()
        {
            return Err(SurfaceError::InvalidGeometry);
        }
        Ok(())
    }
    /// Already physical client pixels. Do not multiply by DPR/zoom again.
    pub fn enclosing_i32(self) -> Result<[i32; 4], SurfaceError> {
        self.validate()?;
        let left = self.x.floor();
        let top = self.y.floor();
        let right = (self.x + self.width).ceil();
        let bottom = (self.y + self.height).ceil();
        let values = [
            left,
            top,
            if self.width == 0.0 { 0.0 } else { right - left },
            if self.height == 0.0 {
                0.0
            } else {
                bottom - top
            },
        ];
        if [left, top, right, bottom, values[2], values[3]]
            .into_iter()
            .any(|v| v < f64::from(i32::MIN) || v > f64::from(i32::MAX))
        {
            return Err(SurfaceError::InvalidGeometry);
        }
        Ok(values.map(|value| value as i32))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceVisibility {
    Hidden,
    Visible,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceClip {
    Rectangular(PhysicalClientRect),
    Unsupported,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfacePlane {
    /// Paint-compatible rectangular placement only; does not acknowledge OS composition.
    RectangularChildWindow,
    Unsupported,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFocus {
    RenderOnly,
    Unsupported,
    Unknown,
}
/// Logical DOM focus is metadata, separate from native keyboard/pointer ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceLogicalFocus {
    pub focused_node: Option<u64>,
    pub surface_focusable: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    Busy,
    Stale,
    Revoked,
    NotReady,
    NotVideo,
    Duplicate,
    Capacity,
    CounterExhausted,
    InvalidGeometry,
    Unsupported(&'static str),
}
impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native surface slot: {self:?}")
    }
}
impl std::error::Error for SurfaceError {}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
