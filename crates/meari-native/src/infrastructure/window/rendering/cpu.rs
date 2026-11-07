//! The existing Vello CPU → RGBA → softbuffer path, with separated stage clocks.
use super::metrics::RenderTimings;
use anyrender::{ImageRenderer, PaintScene, Scene, WindowHandle};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use softbuffer::{Context, Surface};
use std::{num::NonZeroU32, sync::Arc, time::Instant};

pub(super) struct Cpu {
    surface: Surface<Arc<dyn WindowHandle>, Arc<dyn WindowHandle>>,
    _context: Context<Arc<dyn WindowHandle>>,
    renderer: VelloCpuImageRenderer,
    buffer: Vec<u8>,
    size: (u32, u32),
}
impl Cpu {
    pub fn new(window: Arc<dyn WindowHandle>, size: (u32, u32)) -> Result<Self, String> {
        let context = Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = Surface::new(&context, window).map_err(|e| e.to_string())?;
        let mut this = Self {
            surface,
            _context: context,
            renderer: VelloCpuImageRenderer::new(size.0, size.1),
            buffer: Vec::new(),
            size: (0, 0),
        };
        this.resize(size)?;
        Ok(this)
    }
    pub fn resize(&mut self, size: (u32, u32)) -> Result<(), String> {
        if self.size != size {
            self.surface
                .resize(
                    NonZeroU32::new(size.0).ok_or("zero surface width")?,
                    NonZeroU32::new(size.1).ok_or("zero surface height")?,
                )
                .map_err(|e| e.to_string())?;
            self.renderer.resize(size.0, size.1);
            self.size = size;
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        scene: Scene,
        size: (u32, u32),
        target: &super::target::CurrentTarget,
    ) -> Result<Option<RenderTimings>, String> {
        let mut times = RenderTimings::default();
        let start = Instant::now();
        self.resize(size)?;
        let mut buffer = self.surface.buffer_mut().map_err(|e| e.to_string())?;
        times.surface_acquire = start.elapsed();
        let start = Instant::now();
        let size = self.size;
        self.renderer.render_to_vec(
            |painter| {
                painter.fill(
                    peniko::Fill::NonZero,
                    Default::default(),
                    peniko::Color::WHITE,
                    None,
                    &kurbo::Rect::new(0., 0., size.0 as f64, size.1 as f64),
                );
                painter.append_scene(scene, Default::default());
            },
            &mut self.buffer,
        );
        times.raster = start.elapsed();
        let start = Instant::now();
        let (pixels, remainder) = self.buffer.as_chunks::<4>();
        if pixels.len() != buffer.len() || !remainder.is_empty() {
            return Err("CPU raster and window buffer sizes differ".into());
        }
        for (&[r, g, b, _], dest) in pixels.iter().zip(buffer.iter_mut()) {
            *dest = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        }
        times.pixel_copy = start.elapsed();
        if !target.accepts(self.size) {
            self.renderer.reset();
            return Ok(None);
        }
        let start = Instant::now();
        buffer.present().map_err(|e| e.to_string())?;
        times.present_call = start.elapsed();
        times.present_returned_at = Some(Instant::now());
        self.renderer.reset();
        Ok(Some(times))
    }
}

impl anyrender::RenderContext for Cpu {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn std::any::Any>,
    ) -> Result<anyrender::ResourceId, anyrender::RegisterResourceError> {
        self.renderer.try_register_custom_resource(resource)
    }
    fn unregister_resource(&mut self, id: anyrender::ResourceId) {
        self.renderer.unregister_resource(id);
    }
    fn renderer_specific_context(&self) -> Option<Box<dyn std::any::Any>> {
        self.renderer.renderer_specific_context()
    }
}
