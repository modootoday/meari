use crate::{application::Renderer, domain::Viewport};

/// Measures a CSS sizing element against host bounds, then commits a layout viewport.
/// The host remains the reference for sizing; media queries in the final document
/// use the returned viewport. This avoids iterative viewport feedback.
pub fn layout_screen(
    renderer: &mut dyn Renderer,
    host: &Viewport,
    sizing_element: &str,
    time_seconds: f64,
) -> Result<Viewport, String> {
    renderer.layout(host, time_seconds)?;
    let bounds = renderer.bounds(sizing_element)?;
    let dimension = |value: f32| -> Result<u32, String> {
        if !value.is_finite() || !(1.0..=8192.0).contains(&value) {
            return Err("CSS screen dimensions must be finite and in 1..=8192".into());
        }
        Ok(value.round() as u32)
    };
    let viewport = Viewport {
        width: dimension(bounds.width)?,
        height: dimension(bounds.height)?,
        dark: host.dark,
    };
    renderer.layout(&viewport, time_seconds)?;
    Ok(viewport)
}
