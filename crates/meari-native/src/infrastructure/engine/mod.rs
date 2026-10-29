use crate::{
    application::Renderer,
    domain::{LayoutBox, Viewport},
};
use anyrender::ImageRenderer;
use blitz_html::HtmlDocument;
mod generic_fonts;
#[cfg(feature = "window")]
pub(crate) mod surface_slots;

pub struct NativeRenderer {
    #[cfg(feature = "window")]
    surface_identity: std::rc::Rc<()>,
    #[cfg(feature = "window")]
    surface_observation: std::cell::RefCell<surface_slots::SurfaceObservationState>,
    pub document: HtmlDocument,
    layout_time: f64,
    pub last_paint: Option<crate::domain::graphics::FrameReceipt>,
    asset_host: Option<std::sync::Arc<super::asset_provider::AssetProvider>>,
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
        self.resolve_assets(time_seconds)?;
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
        self.resolve_assets(self.layout_time)?;
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
        let mut paint_result = Ok(());
        renderer.render_to_vec(
            |scene| {
                paint_result = blitz_paint::paint_scene_checked(
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
        paint_result.map_err(str::to_owned)?;
        Ok(rgba)
    }
}

impl NativeRenderer {
    pub fn from_html_with_assets(
        html: &str,
        assets: meari::application::assets::VerifiedAssets,
    ) -> Result<Self, String> {
        let provider = super::asset_provider::AssetProvider::new(assets)?;
        let config = blitz_dom::DocumentConfig {
            base_url: Some(super::asset_provider::BASE_URL.into()),
            net_provider: Some(provider.clone()),
            font_ctx: Some(generic_fonts::context()),
            ..Default::default()
        };
        Ok(Self {
            #[cfg(feature = "window")]
            surface_identity: std::rc::Rc::new(()),
            #[cfg(feature = "window")]
            surface_observation: std::cell::RefCell::new(
                surface_slots::SurfaceObservationState::default(),
            ),
            document: super::range_input::configure(HtmlDocument::from_html(html, config)),
            layout_time: 0.0,
            last_paint: None,
            asset_host: Some(provider),
            #[cfg(all(feature = "gl", windows))]
            gl_host: super::gl::current(),
        })
    }
    pub fn asset_report(&self) -> Result<Option<super::AssetLoadReport>, String> {
        self.asset_host
            .as_ref()
            .map(|host| host.report())
            .transpose()
    }
    pub(crate) fn validate_asset_css(&self, css: &str) -> Result<(), String> {
        if let Some(host) = &self.asset_host {
            host.validate_css(css)?;
        }
        Ok(())
    }
    pub(crate) fn validate_asset_references<'a>(
        &self,
        references: impl Iterator<Item = &'a str>,
    ) -> Result<(), String> {
        if let Some(host) = &self.asset_host {
            for reference in references {
                host.validate_reference(reference)?;
            }
        }
        Ok(())
    }
    pub(crate) fn resolve_assets(&mut self, time: f64) -> Result<(), String> {
        super::range_input::refresh(&mut self.document)?;
        let Some(host) = &self.asset_host else {
            self.document.resolve(time);
            return Ok(());
        };
        // Synchronous responses from prior rounds have been ingested when a round
        // creates no new requests. This does not certify font shaping or UI parity.
        for _ in 0..16 {
            let before = host.request_count()?;
            self.document.resolve(time);
            host.ensure_success()?;
            if host.request_count()? == before && !self.document.has_pending_critical_resources() {
                return Ok(());
            }
        }
        Err("declared asset resolution did not settle within its round budget".into())
    }
    pub fn layout_environment(
        &mut self,
        environment: &meari::domain::host::HostEnvironment,
    ) -> Result<(), String> {
        use meari::domain::host::HostTarget;
        environment.validate()?;
        if environment.target != HostTarget::Native {
            return Err("native renderer requires native host".into());
        }
        let zoom = environment
            .layout_zoom
            .ok_or("native document zoom is not observed")?;
        let scale = environment.device_scale;
        let physical = |css: u32| -> Result<u32, String> {
            let pixels = f64::from(css) * scale * zoom;
            if !pixels.is_finite() || !(1.0..=8192.0).contains(&pixels) {
                return Err("native physical viewport exceeds 1..=8192 rendering budget".into());
            }
            Ok(pixels.round() as u32)
        };
        let mut viewport = blitz_traits::shell::Viewport::new(
            physical(environment.layout.width)?,
            physical(environment.layout.height)?,
            scale as f32,
            if environment.layout.dark {
                blitz_traits::shell::ColorScheme::Dark
            } else {
                blitz_traits::shell::ColorScheme::Light
            },
        );
        viewport.zoom = zoom as f32;
        if !viewport.scale().is_finite() || viewport.scale() <= 0.0 {
            return Err("native scale cannot be represented".into());
        }
        let root = self.document.root_element().id;
        self.document.mutate().set_attribute(
            root,
            blitz_dom::QualName::new(
                None,
                blitz_dom::Namespace::from(""),
                blitz_dom::LocalName::from("class"),
            ),
            if environment.layout.dark { "dark" } else { "" },
        );
        self.document.set_viewport(viewport);
        self.resolve_assets(self.layout_time)?;
        Ok(())
    }
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
            #[cfg(feature = "window")]
            surface_identity: std::rc::Rc::new(()),
            #[cfg(feature = "window")]
            surface_observation: std::cell::RefCell::new(
                surface_slots::SurfaceObservationState::default(),
            ),
            document: super::range_input::configure(HtmlDocument::from_html(
                html,
                blitz_dom::DocumentConfig {
                    font_ctx: Some(generic_fonts::context()),
                    ..Default::default()
                },
            )),
            layout_time: 0.0,
            last_paint: None,
            asset_host: None,
            #[cfg(all(feature = "gl", windows))]
            gl_host: super::gl::current(),
        }
    }
}
