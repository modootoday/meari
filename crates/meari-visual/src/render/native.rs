//! Native path: offscreen CPU raster through the production InteractiveDocument.
use crate::scroll::Offset;
use blitz_dom::ScrollBehavior;
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::document::InteractiveDocument,
};

pub fn render(source: &str, css: &str, dark: bool, scroll: &[Offset]) -> Result<Vec<u8>, String> {
    let component = meari::parse(source, meari::application::Language::Tsx)
        .map_err(|d| format!("parse: {}", d.message))?;
    let viewport = Viewport {
        width: crate::WIDTH,
        height: crate::HEIGHT,
        dark,
    };
    let mut document = InteractiveDocument::new(component, css, viewport.clone())?;
    if scroll.is_empty() {
        return document.renderer.rasterize(&viewport);
    }
    // Lay out once so scroll extents exist, then pin the offsets exactly like `Element.scrollTo`.
    document.renderer.rasterize(&viewport)?;
    for offset in scroll {
        let doc = &mut document.renderer.document;
        let node = doc
            .query_selector(&offset.selector)
            .map_err(|e| format!("scroll selector {}: {e:?}", offset.selector))?
            .ok_or_else(|| format!("scroll selector {} matched nothing", offset.selector))?;
        doc.scroll_to(node, offset.x, offset.y, ScrollBehavior::Instant);
    }
    // Capture the settled frame: overlay scrollbars flash on scroll and fade after 500+200 ms
    // (Chromium overlay timing); the Web capture hides scrollbars, so compare the idle state.
    std::thread::sleep(std::time::Duration::from_millis(750));
    document.renderer.rasterize(&viewport)
}
