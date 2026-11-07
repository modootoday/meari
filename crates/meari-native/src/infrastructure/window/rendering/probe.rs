//! Deterministic, content-free developer probe using the actual native driver.
use super::{WindowFrameTimings, WindowRendererChoice};
use crate::{
    application::WindowOptions,
    domain::Viewport,
    infrastructure::{
        document::InteractiveDocument,
        native_host::{NativeDocumentHandle, NativeHostRuntime},
        window::NativeWindowDriver,
    },
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

struct ResizeClock {
    size: (u32, u32),
    requested: Instant,
}

struct Probe {
    driver: NativeWindowDriver,
    started: Option<Instant>,
    deadline: Instant,
    samples: Vec<WindowFrameTimings>,
    requests: Arc<Mutex<Vec<ResizeClock>>>,
    request_samples: Vec<Duration>,
    presentation_gaps: Vec<Duration>,
    previous_present: Option<Instant>,
    last_request_index: Option<usize>,
    stale_order: usize,
    last: Option<crate::domain::graphics::FrameTicket>,
    launched: bool,
}
impl Probe {
    fn record(&mut self) {
        if let Some(submission) = self.driver.last_submission()
            && self.last.as_ref() != Some(&submission.ticket)
        {
            self.last = Some(submission.ticket);
            if let Some(sample) = self.driver.last_frame_timings()
                && sample.resize_to_present.is_some()
                && let Some(returned) = sample.present_returned_at
            {
                let requests = self.requests.lock().expect("probe request clock");
                if let Some((index, request)) =
                    requests.iter().enumerate().rev().find(|(_, request)| {
                        request.size == (submission.width, submission.height)
                            && request.requested <= returned
                    })
                {
                    if self
                        .last_request_index
                        .is_some_and(|previous| index < previous)
                    {
                        self.stale_order += 1;
                        self.driver.application.errors.lock().expect("probe diagnostics")
                            .push("resize probe observed an older-size presentation after a newer frame".into());
                    }
                    self.last_request_index = Some(index);
                    self.request_samples
                        .push(returned.saturating_duration_since(request.requested));
                    let previous = self.previous_present.unwrap_or(requests[0].requested);
                    self.presentation_gaps
                        .push(returned.saturating_duration_since(previous));
                    self.previous_present = Some(returned);
                    self.samples.push(sample);
                }
            }
        }
    }
    fn report(&self) {
        let status = self.driver.renderer_status().unwrap_or_default();
        println!(
            "backend={:?} software_fallback={} adapter={:?} error={:?}",
            status.selected, status.software_fallback, status.adapter, status.error
        );
        println!(
            "requested_resizes={} configured_resizes=240 presented_resize_samples={} interval_ms=8 warmup_ms=250",
            self.requests.lock().expect("probe request clock").len(),
            self.samples.len()
        );
        println!("stale_order_observations={}", self.stale_order);
        println!("stage,p50_ms,p95_ms,p99_ms,max_ms");
        for (name, field) in [
            (
                "event_receipt_to_present_return",
                (|s: &WindowFrameTimings| s.resize_to_present.unwrap_or_default())
                    as fn(&WindowFrameTimings) -> Duration,
            ),
            ("event_wait", |s| s.event_wait.unwrap_or_default()),
            ("responsive_source_update", |s| s.source_update),
            ("layout", |s| s.layout),
            ("scene_build", |s| s.scene_build),
            ("raster_encode_or_cpu_raster", |s| s.raster),
            ("pixel_copy", |s| s.pixel_copy),
            ("surface_acquire_configure", |s| s.surface_acquire),
            ("present_call", |s| s.present_call),
        ] {
            print_quantiles(name, self.samples.iter().map(field));
        }
        print_quantiles(
            "resize_request_to_present_return",
            self.request_samples.iter().copied(),
        );
        // Captures starvation hidden by latest-size coalescing: first request ->
        // first resize present, then gaps between successful resize presentations.
        print_quantiles(
            "resize_burst_inter_present_gap",
            self.presentation_gaps.iter().copied(),
        );
        println!(
            "actually_displayed=unobservable; present return is NOT display or GPU completion"
        );
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.can_create_surfaces(event_loop);
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.destroy_surfaces(event_loop);
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.proxy_wake_up(event_loop);
        self.record();
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.driver.window_event(event_loop, id, event);
        self.record();
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.about_to_wait(event_loop);
        if !self.launched
            && self
                .driver
                .renderer_status()
                .is_some_and(|status| status.selected.is_some())
            && self.driver.last_submission().is_some()
            && let Some(view) = self.driver.application.inner.windows.values().next()
            && !view.renderer.needs_completion_poll()
        {
            self.launched = true;
            self.started = Some(Instant::now());
            let window = view.window.clone();
            let requests = self.requests.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                for step in 0..240 {
                    let width = 640 + ((step * 37) % 400);
                    let height = 400 + ((step * 23) % 240);
                    requests
                        .lock()
                        .expect("probe request clock")
                        .push(ResizeClock {
                            size: (width, height),
                            requested: Instant::now(),
                        });
                    let _ = window
                        .request_surface_size(winit::dpi::PhysicalSize::new(width, height).into());
                    std::thread::sleep(Duration::from_millis(8));
                }
                window.request_redraw();
            });
        }
        let failed = self
            .driver
            .renderer_status()
            .is_some_and(|status| status.error.is_some());
        if failed
            || Instant::now() >= self.deadline
            || !self.driver.presentation_errors().is_empty()
            || self
                .started
                .is_some_and(|start| start.elapsed() > Duration::from_secs(5))
        {
            self.report();
            if !failed {
                let complete = self.requests.lock().expect("probe request clock").len() == 240
                    && self.last_request_index == Some(239);
                if !complete {
                    self.driver
                        .application
                        .errors
                        .lock()
                        .expect("probe diagnostics")
                        .push(
                            "resize probe did not complete the burst and present its final size"
                                .into(),
                        );
                }
            }
            event_loop.exit();
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(2),
            ));
        }
    }
}
/// Run a fixed 240-event resize burst against a synthetic 64-control window.
/// This reports event-receipt → present-return, never claimed compositor visibility.
/// `Gpu` rejects software adapters; `GpuAllowSoftware` is diagnostic only.
pub fn run_resize_latency_probe(choice: WindowRendererChoice) -> Result<(), String> {
    let mut markup = String::from("type Props={};export function App(props:Props){return <main>");
    for index in 0..64 {
        markup.push_str(&format!("<button id=\"b{index}\">Control {index}</button>"));
    }
    markup.push_str("</main>;}");
    let source = meari::parse(&markup, meari::application::Language::Tsx).map_err(|e| e.message)?;
    let document = InteractiveDocument::new(
        source,
        "main { display:flex; flex-wrap:wrap; gap:8px; padding:12px; } button { padding:12px; border-radius:8px; background:#ddeeff; border:1px solid #334455; }",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )?;
    let handle = NativeDocumentHandle::new(document, 1).map_err(|e| e.to_string())?;
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
        handle,
        crate::infrastructure::native_host::window_port::StandaloneHost,
    )));
    let event_loop = blitz_shell::create_default_event_loop();
    let mut driver = NativeWindowDriver::attach_with_renderer(
        &event_loop,
        runtime,
        WindowOptions::default(),
        choice,
    )?;
    // X11 does not report a system theme. Select a known fixture theme explicitly;
    // do not weaken the host's strict unknown-color-scheme policy.
    for config in &mut driver.application.inner.pending_windows {
        config.attributes_mut().preferred_theme = Some(winit::window::Theme::Light);
    }
    let errors = driver.application.errors.clone();
    event_loop
        .run_app(Probe {
            driver,
            started: None,
            deadline: Instant::now() + Duration::from_secs(30),
            samples: Vec::new(),
            requests: Arc::new(Mutex::new(Vec::with_capacity(240))),
            request_samples: Vec::new(),
            presentation_gaps: Vec::new(),
            previous_present: None,
            last_request_index: None,
            stale_order: 0,
            last: None,
            launched: false,
        })
        .map_err(|e| e.to_string())?;
    let errors = errors.lock().map_err(|e| e.to_string())?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn quantiles(values: impl Iterator<Item = Duration>) -> Option<[f64; 4]> {
    let mut values = values
        .map(|value| value.as_secs_f64() * 1000.)
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return None;
    }
    Some([50, 95, 99, 100].map(|percent| {
        values[(values.len() * percent)
            .div_ceil(100)
            .saturating_sub(1)
            .min(values.len() - 1)]
    }))
}
fn print_quantiles(name: &str, values: impl Iterator<Item = Duration>) {
    if let Some([p50, p95, p99, max]) = quantiles(values) {
        println!("{name},{p50:.3},{p95:.3},{p99:.3},{max:.3}");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_rank_and_empty_samples_are_explicit() {
        assert_eq!(quantiles(std::iter::empty()), None);
        assert_eq!(
            quantiles([5, 1, 4, 2, 3].into_iter().map(Duration::from_millis)),
            Some([3., 5., 5., 5.])
        );
    }
}
