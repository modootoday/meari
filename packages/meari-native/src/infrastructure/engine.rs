use crate::{
    application::Renderer,
    domain::{LayoutBox, Viewport},
};
use anyrender::ImageRenderer;
use blitz_html::HtmlDocument;

pub struct NativeRenderer {
    pub document: HtmlDocument,
    layout_time: f64,
    pub last_paint: Option<crate::domain::graphics::FrameReceipt>,
    #[cfg(all(feature = "gl", windows))]
    gl_host: Option<std::rc::Rc<std::cell::RefCell<super::gl::GlHost>>>,
}

impl Renderer for NativeRenderer {
    fn layout(&mut self, viewport: &Viewport, time_seconds: f64) -> Result<(), String> {
        viewport.validate()?;
        if !time_seconds.is_finite() || time_seconds < 0.0 {
            return Err("finite nonnegative animation time required".into());
        }
        self.document
            .set_viewport(blitz_traits::shell::Viewport::new(
                viewport.width,
                viewport.height,
                1.0,
                if viewport.dark {
                    blitz_traits::shell::ColorScheme::Dark
                } else {
                    blitz_traits::shell::ColorScheme::Light
                },
            ));
        self.document.resolve(time_seconds);
        self.layout_time = time_seconds;
        Ok(())
    }
    fn bounds(&self, id: &str) -> Result<LayoutBox, String> {
        let node = self
            .document
            .get_element_by_id(id)
            .ok_or_else(|| format!("missing node: {id}"))?;
        let rect = self
            .document
            .get_client_bounding_rect(node)
            .ok_or("node has no layout")?;
        Ok(LayoutBox {
            id: id.into(),
            x: rect.x as f32,
            y: rect.y as f32,
            width: rect.width as f32,
            height: rect.height as f32,
        })
    }
    fn rasterize(&mut self, viewport: &Viewport) -> Result<Vec<u8>, String> {
        viewport.validate()?;
        // resolve() refreshes hover after clearing damage and can queue another restyle.
        // Settle that work at the same clock instant before the offscreen paint receipt.
        self.document.resolve(self.layout_time);
        #[cfg(all(feature = "gl", windows))]
        if let Some(host) = &self.gl_host {
            let mut host = host.borrow_mut();
            let ticket = host.next_ticket()?;
            let (bytes, receipt) = host.render(&mut self.document, viewport, ticket)?;
            self.last_paint = Some(receipt);
            return Ok(bytes);
        }
        let mut renderer =
            anyrender_vello_cpu::VelloCpuImageRenderer::new(viewport.width, viewport.height);
        let mut rgba = Vec::new();
        renderer.render_to_vec(
            |scene| {
                blitz_paint::paint_scene(
                    scene,
                    &mut self.document,
                    1.0,
                    viewport.width,
                    viewport.height,
                    0,
                    0,
                )
            },
            &mut rgba,
        );
        Ok(rgba)
    }
}

impl NativeRenderer {
    pub(crate) fn snapshot_stylesheet_change(&mut self) {
        // Stylo 0.22 stylesheet invalidation reads class/id snapshots even when
        // Blitz created a state-only snapshot for a pending focus/hover change.
        let mut pending = vec![self.document.root_node().id];
        let mut elements = Vec::new();
        while let Some(id) = pending.pop() {
            if let Some(node) = self.document.get_node(id) {
                pending.extend(node.children.iter().copied());
                if node.is_element() {
                    elements.push(id);
                }
            }
        }
        for id in elements {
            self.document.snapshot_node(id);
        }
    }

    pub(crate) fn refresh_styles(&mut self) {
        self.document.resolve(self.layout_time);
    }

    pub fn from_html(html: &str) -> Self {
        Self {
            document: HtmlDocument::from_html(html, Default::default()),
            layout_time: 0.0,
            last_paint: None,
            #[cfg(all(feature = "gl", windows))]
            gl_host: super::gl::current(),
        }
    }
}
