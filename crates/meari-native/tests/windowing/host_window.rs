#![cfg(all(windows, feature = "window"))]

use meari::application::screen::{
    ResizeSurface, ScreenRequest, measured_viewport,
    resize::{ResizeStatus, ResizeTransaction},
};
use meari_native::infrastructure::host;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowAttributes, WindowId},
};

struct Probe {
    window: Option<Box<dyn Window>>,
    transaction: ResizeTransaction,
    ticket: Option<u64>,
    deadline: Instant,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}

impl Probe {
    fn finish(&mut self, event_loop: &dyn ActiveEventLoop, result: Result<(), String>) {
        *self.result.lock().unwrap() = Some(result);
        self.window = None;
        event_loop.exit();
    }

    fn acknowledge(&mut self, event_loop: &dyn ActiveEventLoop) {
        let result = (|| {
            let window = self.window.as_ref().ok_or("missing window")?;
            let observed = host::snapshot_with_zoom(window.as_ref(), 1.25)?;
            let ack = self
                .transaction
                .acknowledge(self.ticket.ok_or("missing ticket")?, observed)?;
            if ack.status != ResizeStatus::Applied {
                return Err(format!(
                    "OS adjusted requested client size to {}x{}",
                    ack.observed.layout.width, ack.observed.layout.height
                ));
            }
            println!(
                "host_resize=500x300 device_scale={} layout_zoom={:?}",
                ack.observed.device_scale, ack.observed.layout_zoom
            );
            Ok(())
        })();
        self.finish(event_loop, result);
    }
}

impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let result = (|| {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Meari host contract probe")
                        .with_visible(false)
                        .with_surface_size(winit::dpi::LogicalSize::new(640, 480)),
                )
                .map_err(|error| error.to_string())?;
            let observed = host::snapshot(window.as_ref())?;
            if (observed.layout.width, observed.layout.height) != (640, 480) {
                return Err("initial native CSS viewport differs from requested size".into());
            }
            let observed = host::snapshot_with_zoom(window.as_ref(), 1.25)?;
            if (observed.layout.width, observed.layout.height) != (512, 384) {
                return Err("native document zoom is not reflected in CSS bounds".into());
            }
            let request = ScreenRequest {
                surface: ResizeSurface::NativeClient,
                viewport: measured_viewport(500.0, 300.0, observed.layout.dark)?,
            };
            self.ticket = Some(self.transaction.begin(request.clone(), &observed)?);
            let immediate =
                host::request_resize_with_zoom(window.as_ref(), &request, 1.25)?.is_some();
            self.window = Some(window);
            Ok(immediate)
        })();
        match result {
            Ok(true) => self.acknowledge(event_loop),
            Ok(false) => (),
            Err(error) => self.finish(event_loop, Err(error)),
        }
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_some_and(|window| window.id() == id)
            && matches!(event, WindowEvent::SurfaceResized(_))
        {
            self.acknowledge(event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if Instant::now() >= self.deadline {
            self.finish(
                event_loop,
                Err("native resize acknowledgement timed out".into()),
            );
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline));
        }
    }
}

#[test]
fn actual_native_client_snapshot_and_resize_acknowledgement() {
    let result = Arc::new(Mutex::new(None));
    let probe = Probe {
        window: None,
        transaction: ResizeTransaction::default(),
        ticket: None,
        deadline: Instant::now() + Duration::from_secs(10),
        result: result.clone(),
    };
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(probe).unwrap();
    result
        .lock()
        .unwrap()
        .take()
        .expect("probe did not report a result")
        .unwrap();
}
