//! Native path: offscreen CPU raster through the production InteractiveDocument.
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::document::InteractiveDocument,
};

pub fn render(source: &str, css: &str, dark: bool) -> Result<Vec<u8>, String> {
    let component = meari::parse(source, meari::application::Language::Tsx)
        .map_err(|d| format!("parse: {}", d.message))?;
    let viewport = Viewport {
        width: crate::WIDTH,
        height: crate::HEIGHT,
        dark,
    };
    let mut document = InteractiveDocument::new(component, css, viewport.clone())?;
    document.renderer.rasterize(&viewport)
}
