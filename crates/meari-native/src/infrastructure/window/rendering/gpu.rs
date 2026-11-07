//! GPU-only Vello raster → texture blit → native swapchain. No CPU readback.
use super::metrics::RenderTimings;
use anyrender::{PaintScene, Scene, WindowHandle};
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};
use wgpu::{CurrentSurfaceTexture, TextureUsages};

struct NativeDisplay(Arc<dyn WindowHandle>);
impl std::fmt::Debug for NativeDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeWindowDisplay")
    }
}
impl winit::raw_window_handle::HasDisplayHandle for NativeDisplay {
    fn display_handle(
        &self,
    ) -> Result<winit::raw_window_handle::DisplayHandle<'_>, winit::raw_window_handle::HandleError>
    {
        self.0.display_handle()
    }
}

struct Resources {
    renderer: vello::Renderer,
    texture: wgpu::Texture,
    blitter: wgpu::util::TextureBlitter,
}
pub(super) struct Gpu {
    // Surface must be released on the UI thread before the event loop/window disappears.
    surface: wgpu::Surface<'static>,
    resources: Option<Box<Resources>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    flight: super::flight::Flight,
    failure: Arc<Mutex<Option<String>>>,
    pub software: bool,
    pub adapter: String,
    reconfigure: bool,
    retry_at: Option<Instant>,
}
impl Gpu {
    pub fn new(
        window: Arc<dyn WindowHandle>,
        size: (u32, u32),
        allow_software: bool,
    ) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(NativeDisplay(window.clone())),
        ));
        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("GPU surface init: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| format!("GPU adapter init: {e}"))?;
        let info = adapter.get_info();
        let software = info.device_type == wgpu::DeviceType::Cpu;
        if software && !allow_software {
            return Err(format!(
                "GPU hardware required; software adapter rejected: {}",
                info.name
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("native window GPU"),
            required_features: wgpu::Features::CLEAR_TEXTURE,
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device init: {e}"))?;
        let failure = Arc::new(Mutex::new(None));
        let lost = failure.clone();
        device.set_device_lost_callback(move |reason, _message| {
            // Do not collect driver strings which could include user-provided labels.
            *lost.lock().expect("GPU failure lock") = Some(format!("GPU device lost: {reason:?}"));
        });
        let errors = failure.clone();
        device.on_uncaptured_error(Arc::new(move |_error| {
            *errors.lock().expect("GPU failure lock") =
                Some("GPU validation or device error".into());
        }));
        let mut config = surface
            .get_default_config(&adapter, size.0, size.1)
            .ok_or("GPU surface has no compatible configuration")?;
        if let Some(format) = surface
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|format| !format.is_srgb())
        {
            config.format = format;
        }
        // Avoid a deep FIFO backlog; some platforms may still throttle acquire/present.
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                num_init_threads: std::num::NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("Vello GPU init: {e}"))?;
        let texture = Self::texture(&device, size);
        let blitter = wgpu::util::TextureBlitter::new(&device, config.format);
        device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| format!("GPU init poll: {error}"))?;
        if let Some(error) = failure.lock().expect("GPU failure lock").clone() {
            return Err(format!("GPU renderer init: {error}"));
        }
        Ok(Self {
            surface,
            resources: Some(Box::new(Resources {
                renderer,
                texture,
                blitter,
            })),
            device,
            queue,
            config,
            flight: Default::default(),
            failure,
            software,
            reconfigure: false,
            retry_at: None,
            adapter: format!("{} ({:?}, {:?})", info.name, info.device_type, info.backend),
        })
    }
    fn texture(device: &wgpu::Device, size: (u32, u32)) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window GPU raster target"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }
    pub fn in_flight(&self) -> bool {
        self.flight.busy()
    }
    pub fn error(&self) -> Option<String> {
        self.failure.lock().expect("GPU failure lock").clone()
    }
    pub fn can_render(&mut self) -> Result<bool, String> {
        // Poll never waits for GPU completion. No per-frame wait_indefinitely on the UI thread.
        if let Err(error) = self.device.poll(wgpu::PollType::Poll) {
            let error = format!("GPU poll: {error}");
            *self.failure.lock().expect("GPU failure lock") = Some(error.clone());
            return Err(error);
        }
        if let Some(error) = self.error() {
            return Err(error);
        }
        if self
            .retry_at
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return Ok(false);
        }
        self.retry_at = None;
        self.flight.ready(Instant::now()).map_err(|error| {
            *self.failure.lock().expect("GPU failure lock") = Some(error.into());
            error.to_owned()
        })
    }

    pub fn render(
        &mut self,
        scene: Scene,
        size: (u32, u32),
        current_target: &super::target::CurrentTarget,
    ) -> Result<Option<RenderTimings>, String> {
        super::gpu_scene::validate(&scene)?;
        if !self.can_render()? {
            return Ok(None);
        }
        if size.0 == 0 || size.1 == 0 {
            return Err("zero GPU surface size".into());
        }
        let mut times = RenderTimings::default();
        let start = Instant::now();
        // Configuration/texture replacement is deferred until the previous callback completes.
        if self.reconfigure || (self.config.width, self.config.height) != size {
            self.reconfigure = false;
            self.config.width = size.0;
            self.config.height = size.1;
            self.surface.configure(&self.device, &self.config);
            self.resources
                .as_mut()
                .expect("active GPU resources")
                .texture = Self::texture(&self.device, size);
        }
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                self.retry_at = Some(Instant::now() + Duration::from_millis(16));
                return Ok(None);
            }
            CurrentSurfaceTexture::Outdated => {
                self.reconfigure = true;
                self.retry_at = Some(Instant::now() + Duration::from_millis(2));
                return Ok(None);
            }
            CurrentSurfaceTexture::Lost => {
                return Err("GPU surface lost; suspend/resume required".into());
            }
            CurrentSurfaceTexture::Validation => return Err("GPU surface validation failed".into()),
        };
        times.surface_acquire = start.elapsed();
        let resources = self.resources.as_mut().expect("active GPU resources");
        let start = Instant::now();
        let mut vello_scene = vello::Scene::new();
        anyrender_vello::VelloScenePainter::new(&mut vello_scene)
            .append_scene(scene, Default::default());
        let view = resources.texture.create_view(&Default::default());
        let completed = self.flight.begin(Instant::now()).map_err(str::to_owned)?;
        let raster_result = resources
            .renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &vello_scene,
                &view,
                &vello::RenderParams {
                    base_color: peniko::Color::WHITE,
                    width: size.0,
                    height: size.1,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|e| format!("GPU raster submission: {e}"));
        if let Err(error) = raster_result {
            let lease = (
                vello_scene,
                resources.texture.clone(),
                frame.texture.clone(),
            );
            self.queue.on_submitted_work_done(move || {
                drop(lease);
                completed.store(true, Ordering::Release);
            });
            return Err(error);
        }
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        resources
            .blitter
            .copy(&self.device, &mut encoder, &view, &target);
        self.queue.submit([encoder.finish()]);
        times.raster = start.elapsed();
        // Retain frame resources through queue completion, not merely until present() returns.
        // wgpu also owns references to every submitted buffer/texture and Vello's cached buffers.
        let lease = (
            vello_scene,
            resources.texture.clone(),
            frame.texture.clone(),
        );
        self.queue.on_submitted_work_done(move || {
            drop(lease);
            completed.store(true, Ordering::Release);
        });
        // Size can change while encoding, even before its event is dispatched.
        // The completion callback above still owns discarded frame resources.
        if !current_target.accepts(size) {
            return Ok(None);
        }
        let start = Instant::now();
        self.queue.present(frame);
        times.present_call = start.elapsed();
        times.present_returned_at = Some(Instant::now());
        if let Some(error) = self.error() {
            return Err(error);
        }
        Ok(Some(times))
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        // Surface is NOT sent to another thread. Only device-owned caches retire there.
        if self.flight.busy() {
            let resources = self.resources.take();
            let device = self.device.clone();
            // Bounded shutdown drain off the UI thread. wgpu retains submitted references
            // on timeout/device loss; dropping Rust handles does not free live GPU allocations.
            std::thread::spawn(move || {
                if device
                    .poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(Duration::from_secs(2)),
                    })
                    .is_err()
                {
                    // Cancel a lost/hung device, never leave a callback/resource ownership cycle.
                    eprintln!("GPU shutdown drain failed; cancelling device");
                    device.destroy();
                }
                drop(resources);
            });
        }
    }
}
