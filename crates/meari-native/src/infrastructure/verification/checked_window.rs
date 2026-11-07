//! First frame and redraw use the same checked scene transaction.
use super::checked_scene::{PreparedScene, SceneSubmission};
use crate::domain::graphics::FrameTicket;
use crate::infrastructure::window::NativeWindowRenderer;
use anyrender::WindowRenderer;
use blitz_shell::{BlitzShellEvent, BlitzShellProxy, View};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Wake, Waker},
};
use winit::window::WindowId;

pub(in super::super) struct WindowFrames {
    authority: Rc<Cell<bool>>,
    retained: Option<super::super::native_host::RetainedFrameProof>,
    context: String,
    generation: u64,
    sequence: u64,
    last: Option<SceneSubmission>,
    target: Option<(u32, u32)>,
}
impl WindowFrames {
    pub(in super::super) fn retained(
        &self,
    ) -> Option<super::super::native_host::RetainedFrameProof> {
        self.retained.clone()
    }
    pub(in super::super) fn submission(
        &self,
    ) -> Option<super::super::window::NativeFrameSubmission> {
        self.last
            .as_ref()
            .map(|last| super::super::window::NativeFrameSubmission {
                ticket: last.ticket.clone(),
                width: last.width,
                height: last.height,
                commands: last.commands,
            })
    }
    pub fn new() -> Result<Self, String> {
        static EPOCH: AtomicU64 = AtomicU64::new(1);
        let epoch = EPOCH
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "window frame identity exhausted")?;
        Ok(Self {
            authority: Rc::new(Cell::new(true)),
            retained: None,
            context: format!("native-window-{epoch}"),
            generation: 0,
            sequence: 0,
            last: None,
            target: None,
        })
    }
    pub fn failure(&self, error: &str) -> String {
        match &self.last {
            Some(last) => format!(
                "{error}; last submission record {} unchanged ({}x{}, {} commands)",
                last.ticket.scene_generation, last.width, last.height, last.commands
            ),
            None => format!("{error}; no successful scene has been submitted"),
        }
    }
}
impl Drop for WindowFrames {
    fn drop(&mut self) {
        self.authority.set(false);
    }
}
struct PollWake {
    proxy: BlitzShellProxy,
    window: WindowId,
}
impl Wake for PollWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.proxy.send_event(BlitzShellEvent::Poll {
            window_id: self.window,
        });
    }
}

pub(in super::super) fn render(
    view: &mut View<NativeWindowRenderer>,
    frames: &mut WindowFrames,
    resume: bool,
) -> Result<(), String> {
    match super::super::native_host::access::with(view.doc.as_ref(), |document| {
        document.renderer.invalidate_surface_paint()
    }) {
        Ok(()) | Err(super::super::native_host::HostAccessError::Invalid(_)) => {}
        Err(error) => return Err(error.to_string()),
    }
    if resume {
        frames.authority.set(false);
        frames.authority = Rc::new(Cell::new(true));
        frames.retained = None;
        if !view.renderer.complete_resume() {
            return match view.renderer.take_error() {
                Some(error) => Err(error),
                None => Ok(()),
            };
        }
        frames.generation = frames
            .generation
            .checked_add(1)
            .ok_or("window context generation exhausted")?;
        view.waker = Some(Waker::from(Arc::new(PollWake {
            proxy: view.proxy.clone(),
            window: view.window_id(),
        })));
    }
    if !view.renderer.is_active() || view.renderer.is_pending() || frames.generation == 0 {
        return Ok(());
    }
    if !view.renderer.can_render()? {
        return Ok(());
    }
    let layout_start = std::time::Instant::now();
    let event_wait = view.renderer.frame_timings.event_wait;
    let environment = super::super::native_host::access::with_mut(view.doc.as_mut(), |doc| {
        std::mem::take(&mut doc.environment_timings)
    })
    .unwrap_or_default();
    if view.window.surface_size().width == 0 || view.window.surface_size().height == 0 {
        return Ok(());
    }
    let time = view.current_animation_time();
    let settled = match super::super::native_host::access::with_mut(view.doc.as_mut(), |document| {
        document.renderer.resolve_assets(time)
    }) {
        Ok(result) => {
            result?;
            true
        }
        Err(super::super::native_host::HostAccessError::Invalid(_)) => false,
        Err(error) => return Err(error.to_string()),
    };
    let source = super::super::native_host::access::handle(view.doc.as_ref())
        .and_then(|handle| handle.stamp().ok());
    let surface_scope = source.as_ref().and_then(|stamp| {
        super::super::native_host::access::with(view.doc.as_ref(), |document| {
            document.renderer.surface_render_scope(stamp)
        })
        .ok()
        .and_then(Result::ok)
    });
    let mut document = view.doc.inner_mut();
    if !settled {
        document.resolve(time);
    }
    let (width, height) = document.viewport().window_size;
    let insets = view.safe_area_insets;
    {
        let surface_width = width
            .checked_add(insets.left)
            .and_then(|value| value.checked_add(insets.right))
            .ok_or("window surface width overflow")?;
        let surface_height = height
            .checked_add(insets.top)
            .and_then(|value| value.checked_add(insets.bottom))
            .ok_or("window surface height overflow")?;
        if surface_width == 0 || surface_height == 0 {
            return Ok(());
        }
        let target = (surface_width, surface_height);
        // Configure the actual target here; neither an OS size nor viewport alone proves it.
        if resume || frames.target != Some(target) {
            view.renderer.set_size(surface_width, surface_height);
            frames.target = Some(target);
        }
    }
    if !view.is_visible || width == 0 || height == 0 || document.has_pending_critical_resources() {
        return Ok(());
    }
    let layout = layout_start.elapsed() + environment.0;
    let scene_start = std::time::Instant::now();
    let ticket = FrameTicket {
        context_id: frames.context.clone(),
        context_generation: frames.generation,
        scene_generation: frames
            .sequence
            .checked_add(1)
            .ok_or("window scene generation exhausted")?,
    };
    let animating = document.is_animating();
    let prepared = if let Some(scope) = &surface_scope {
        PreparedScene::prepare_observed(
            &mut document,
            ticket.clone(),
            (insets.left, insets.top),
            &scope.targets,
        )?
    } else {
        PreparedScene::prepare(&mut document, ticket.clone(), (insets.left, insets.top))?
    };
    // Keep document and view exclusively borrowed through submission. FrameTicket
    // is a correlation DTO; asynchronous use would require a fresh owner check.
    let scene_build = scene_start.elapsed();
    // The OS may have delivered a newer size while work was being requested. Do not
    // open a presentation transaction for a target that no longer matches the window.
    let physical = view.window.surface_size();
    if frames.target != Some((physical.width, physical.height)) {
        drop(document);
        view.handle_winit_event(winit::event::WindowEvent::SurfaceResized(physical));
        view.window.request_redraw();
        return Ok(());
    }
    let mut submitted = prepared.submit(&ticket, (width, height), &mut view.renderer)?;
    if let Some(error) = view.renderer.take_error() {
        return Err(error);
    }
    if !view.renderer.did_submit {
        return Ok(());
    }
    let render = view.renderer.render_timings;
    view.renderer.frame_timings = super::super::window::WindowFrameTimings {
        event_wait,
        source_update: environment.1,
        layout,
        scene_build,
        raster: render.raster,
        pixel_copy: render.pixel_copy,
        surface_acquire: render.surface_acquire,
        present_call: render.present_call,
        resize_to_present: view.renderer.resize_received.take().and_then(|received| {
            render
                .present_returned_at
                .map(|returned| returned.saturating_duration_since(received))
        }),
        present_returned_at: render.present_returned_at,
        resize_to_displayed: None,
    };
    let placement = super::super::engine::surface_slots::RendererClientPlacement {
        viewport: (width, height),
        insets: [insets.left, insets.top, insets.right, insets.bottom],
        target: frames
            .target
            .ok_or("window renderer target was not configured")?,
    };
    drop(document);
    if let (Some(scope), Some(trace)) = (surface_scope, submitted.surface_trace.take()) {
        let current = super::super::native_host::access::handle(view.doc.as_ref())
            .and_then(|handle| handle.stamp().ok());
        if current.as_ref() == Some(&scope.document) {
            let committed =
                super::super::native_host::access::with(view.doc.as_ref(), |document| {
                    document.renderer.commit_surface_paint(
                        scope,
                        submitted.ticket.clone(),
                        trace,
                        Some(placement),
                    )
                });
            if !matches!(committed, Ok(Ok(()))) {
                super::super::native_host::access::with(view.doc.as_ref(), |document| {
                    document.renderer.invalidate_surface_paint()
                })
                .map_err(|error| error.to_string())?;
            }
        }
    }
    frames.sequence = submitted.ticket.scene_generation;
    frames.retained = match (
        super::super::native_host::access::handle(view.doc.as_ref()),
        source,
    ) {
        (Some(handle), Some(stamp)) => Some(
            handle
                .checked_frame_proof(&stamp, &submitted.ticket, &frames.authority)
                .map_err(|error| error.to_string())?,
        ),
        _ => None,
    };
    frames.last = Some(submitted);
    if animating {
        view.request_redraw();
    }
    Ok(())
}
