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
    let viewport = meari::application::screen::measured_viewport(
        f64::from(bounds.width),
        f64::from(bounds.height),
        host.dark,
    )?;
    renderer.layout(&viewport, time_seconds)?;
    Ok(viewport)
}
