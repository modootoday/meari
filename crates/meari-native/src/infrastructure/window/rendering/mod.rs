//! Real-window adapters. GPU selection never uses the offscreen Skia GL host.
mod cpu;
#[cfg(any(feature = "window-gpu", test))]
mod flight;
#[cfg(feature = "window-gpu")]
mod gpu;
#[cfg(any(feature = "window-gpu", test))]
mod gpu_scene;
mod metrics;
mod probe;
mod target;
use anyrender::{PaintScene, RenderContext, Scene, WindowHandle, WindowRenderer};
pub use metrics::WindowFrameTimings;
pub use probe::run_resize_latency_probe;
use std::{sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowRendererChoice {
    #[default]
    Cpu,
    /// Fail rather than silently using a CPU adapter. Requires `window-gpu`.
    Gpu,
    /// Diagnostic opt-in to a software GPU adapter; explicitly reported as fallback.
    GpuAllowSoftware,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowBackend {
    VelloCpu,
    VelloGpu,
}
#[derive(Clone, Debug, Default)]
pub struct WindowRendererStatus {
    /// None until initialization succeeds; never infer success from the requested name.
    pub selected: Option<WindowBackend>,
    pub software_fallback: bool,
    pub adapter: Option<String>,
    pub error: Option<String>,
}

enum Backend {
    Suspended,
    Cpu(Box<cpu::Cpu>),
    #[cfg(feature = "window-gpu")]
    Gpu(Box<gpu::Gpu>),
}
/// Exported equivalent adapter for hosts using the shell directly.
pub struct NativeWindowRenderer {
    backend: Backend,
    choice: WindowRendererChoice,
    status: WindowRendererStatus,
    scene: Scene,
    size: (u32, u32),
    target: target::CurrentTarget,
    error: Option<String>,
    deferred: bool,
    pub(crate) did_submit: bool,
    pub(crate) render_timings: metrics::RenderTimings,
    pub(crate) frame_timings: WindowFrameTimings,
    pub(crate) resize_received: Option<Instant>,
}
impl Default for NativeWindowRenderer {
    fn default() -> Self {
        Self::new()
    }
}
impl NativeWindowRenderer {
    pub fn new() -> Self {
        Self {
            backend: Backend::Suspended,
            choice: Default::default(),
            status: Default::default(),
            scene: Scene::new(),
            size: (0, 0),
            target: Default::default(),
            error: None,
            deferred: false,
            did_submit: false,
            render_timings: Default::default(),
            frame_timings: Default::default(),
            resize_received: None,
        }
    }
    /// Selection is only allowed before a surface exists.
    pub fn select(&mut self, choice: WindowRendererChoice) -> Result<(), String> {
        if !matches!(self.backend, Backend::Suspended) {
            return Err("renderer already active".into());
        }
        #[cfg(not(feature = "window-gpu"))]
        if choice != WindowRendererChoice::Cpu {
            return Err("GPU window rendering requires feature window-gpu".into());
        }
        self.choice = choice;
        Ok(())
    }
    /// Bind a real window for a final post-raster physical-size check.
    /// NativeWindowDriver does this automatically. Direct shell embedders should
    /// call it before resuming the renderer.
    pub fn observe_window(&mut self, window: Arc<dyn winit::window::Window>) {
        self.target.bind(window);
    }
    pub fn status(&self) -> WindowRendererStatus {
        let status = self.status.clone();
        #[cfg(feature = "window-gpu")]
        let status = {
            let mut status = status;
            if let Backend::Gpu(gpu) = &self.backend {
                status.error = gpu.error().or(status.error);
            }
            status
        };
        status
    }
    pub fn last_frame_timings(&self) -> WindowFrameTimings {
        self.frame_timings
    }
    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.error.take().or(self.status().error)
    }
    /// Nonblocking completion check BEFORE layout/scene construction. A skipped frame is not submitted.
    pub(crate) fn can_render(&mut self) -> Result<bool, String> {
        match &mut self.backend {
            #[cfg(feature = "window-gpu")]
            Backend::Gpu(gpu) => {
                let ready = gpu.can_render()?;
                self.deferred |= !ready;
                Ok(ready)
            }
            Backend::Cpu(_) => Ok(true),
            Backend::Suspended => Err(self
                .status
                .error
                .clone()
                .unwrap_or("renderer suspended".into())),
        }
    }
    pub(crate) fn needs_completion_poll(&self) -> bool {
        #[cfg(feature = "window-gpu")]
        if let Backend::Gpu(gpu) = &self.backend {
            return (gpu.in_flight() || self.deferred) && self.status().error.is_none();
        }
        false
    }
    pub(crate) fn poll_completion(&mut self) -> Result<bool, String> {
        #[cfg(feature = "window-gpu")]
        if let Backend::Gpu(gpu) = &mut self.backend
            && gpu.can_render()?
        {
            return Ok(std::mem::take(&mut self.deferred));
        }
        Ok(false)
    }
    fn fail(&mut self, error: String) {
        self.status.error = Some(error.clone());
        self.error = Some(error);
    }
}
impl RenderContext for NativeWindowRenderer {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn std::any::Any>,
    ) -> Result<anyrender::ResourceId, anyrender::RegisterResourceError> {
        match &mut self.backend {
            Backend::Cpu(cpu) => cpu.try_register_custom_resource(resource),
            _ => Err(anyrender::RegisterResourceErrorKind::UnsupportedResourceKind.into()),
        }
    }
    fn unregister_resource(&mut self, id: anyrender::ResourceId) {
        if let Backend::Cpu(cpu) = &mut self.backend {
            cpu.unregister_resource(id);
        }
    }
    fn renderer_specific_context(&self) -> Option<Box<dyn std::any::Any>> {
        match &self.backend {
            Backend::Cpu(cpu) => cpu.renderer_specific_context(),
            _ => None,
        }
    }
}
impl WindowRenderer for NativeWindowRenderer {
    type ScenePainter<'a> = Scene;
    fn resume<F: FnOnce() + 'static>(
        &mut self,
        window: Arc<dyn WindowHandle>,
        width: u32,
        height: u32,
        ready: F,
    ) {
        self.suspend();
        self.size = (width.max(1), height.max(1));
        self.status = WindowRendererStatus::default();
        self.error = None;
        let result = match self.choice {
            WindowRendererChoice::Cpu => cpu::Cpu::new(window, self.size).map(|cpu| {
                self.status.selected = Some(WindowBackend::VelloCpu);
                Backend::Cpu(Box::new(cpu))
            }),
            #[cfg(feature = "window-gpu")]
            choice => gpu::Gpu::new(
                window,
                self.size,
                choice == WindowRendererChoice::GpuAllowSoftware,
            )
            .map(|gpu| {
                self.status.selected = Some(WindowBackend::VelloGpu);
                self.status.software_fallback = gpu.software;
                self.status.adapter = Some(gpu.adapter.clone());
                Backend::Gpu(Box::new(gpu))
            }),
            #[cfg(not(feature = "window-gpu"))]
            _ => Err("GPU window rendering requires feature window-gpu".into()),
        };
        match result {
            Ok(backend) => self.backend = backend,
            Err(error) => self.fail(error),
        }
        ready(); // Also deliver failure, so the host is not stuck in Pending.
    }
    fn complete_resume(&mut self) -> bool {
        self.is_active()
    }
    fn suspend(&mut self) {
        self.deferred = false;
        self.did_submit = false;
        self.frame_timings = Default::default();
        self.resize_received = None;
        self.backend = Backend::Suspended;
        self.status.selected = None;
    }
    fn is_active(&self) -> bool {
        !matches!(self.backend, Backend::Suspended) && self.status().error.is_none()
    }
    fn set_size(&mut self, width: u32, height: u32) {
        self.size = (width, height);
    }
    fn render<F: FnOnce(&mut Scene)>(&mut self, draw: F) {
        self.did_submit = false;
        self.scene.reset();
        draw(&mut self.scene);
        let scene = std::mem::take(&mut self.scene);
        let result = match &mut self.backend {
            Backend::Cpu(cpu) => cpu.render(scene, self.size, &self.target),
            #[cfg(feature = "window-gpu")]
            Backend::Gpu(gpu) => gpu.render(scene, self.size, &self.target),
            Backend::Suspended => Err("renderer suspended".into()),
        };
        match result {
            Ok(Some(times)) => {
                self.render_timings = times;
                self.did_submit = true;
            }
            Ok(None) => self.deferred = true,
            Err(error) => self.fail(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_cpu_and_uninitialized_status_do_not_claim_gpu_success() {
        let renderer = NativeWindowRenderer::new();
        assert_eq!(renderer.choice, WindowRendererChoice::Cpu);
        assert!(renderer.status().selected.is_none());
        assert!(!renderer.is_active());
        assert!(!renderer.status().software_fallback);
        assert!(renderer.last_frame_timings().resize_to_displayed.is_none());
    }
    #[cfg(not(feature = "window-gpu"))]
    #[test]
    fn cpu_only_build_rejects_gpu_instead_of_silent_fallback() {
        let mut renderer = NativeWindowRenderer::new();
        assert!(
            renderer
                .select(WindowRendererChoice::Gpu)
                .unwrap_err()
                .contains("window-gpu")
        );
        assert_eq!(renderer.choice, WindowRendererChoice::Cpu);
        assert!(renderer.status().selected.is_none());
    }
    #[test]
    fn failed_render_is_not_a_frame_submission() {
        let mut renderer = NativeWindowRenderer::new();
        renderer.render(|_| {});
        assert!(!renderer.did_submit);
        assert!(renderer.take_error().is_some());
        assert!(renderer.status().selected.is_none());
    }
}
