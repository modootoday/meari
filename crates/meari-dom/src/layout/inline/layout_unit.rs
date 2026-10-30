#[inline(always)]
pub(super) fn f32_max(a: f32, b: f32) -> f32 {
    a.max(b)
}

/// Meari patch (blitz-dom 0.3.0-beta.4): round a CSS px length up to Chrome's 1/64px
/// LayoutUnit (`LayoutUnit::FromFloatCeil`) instead of a whole pixel.
pub(crate) fn layout_unit_ceil(px: f32) -> f32 {
    (px * 64.0).ceil() / 64.0
}
