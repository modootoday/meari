//! Actual OS surface/driver wiring; normalized actions are synthetic, not OS keyboard input.
use super::*;
use meari_native::infrastructure::window::NativeWindowDriver;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::WindowId,
};

struct Probe {
    driver: NativeWindowDriver,
    runtime: Rc<RefCell<NativeHostRuntime<Consumer>>>,
    handle: NativeDocumentHandle,
    action: String,
    phase: u8,
    first_scene: u64,
    reentrant_checked: bool,
    deadline: Instant,
    result: Rc<RefCell<Option<Result<(), String>>>>,
}
impl Probe {
    fn tick(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.result.borrow().is_some() {
            return;
        }
        let result = self.advance();
        match result {
            Ok(true) => {
                *self.result.borrow_mut() = Some(Ok(()));
                event_loop.exit();
            }
            Err(error) => {
                *self.result.borrow_mut() = Some(Err(error));
                event_loop.exit();
            }
            Ok(false) => (),
        }
    }
    fn advance(&mut self) -> Result<bool, String> {
        if Instant::now() >= self.deadline {
            return Err(format!(
                "native host OS probe timed out in phase {}",
                self.phase
            ));
        }
        let frame = self.driver.last_submission();
        match self.phase {
            0 if frame.is_some() => {
                let frame = frame.unwrap();
                if frame.commands == 0 || frame.width == 0 || frame.height == 0 {
                    return Err("empty checked native submission".into());
                }
                self.first_scene = frame.ticket.scene_generation;
                println!(
                    "host_os_first_scene={} commands={} bounds={}x{}",
                    self.first_scene, frame.commands, frame.width, frame.height
                );
                for _ in 0..2 {
                    self.handle
                        .dispatch_action(Action::Activate {
                            path: self.action.clone(),
                        })
                        .map_err(|error| error.to_string())?;
                }
                self.driver
                    .wake_handle()
                    .wake()
                    .map_err(|error| error.to_string())?;
                self.phase = 1;
            }
            1 if self.runtime.borrow().host().completed == 2
                && frame.is_some_and(|frame| frame.ticket.scene_generation > self.first_scene) =>
            {
                let runtime = self.runtime.borrow();
                if runtime.pending_callbacks() != 0 || runtime.host().received.borrow().len() != 2 {
                    return Err("capacity suffix was not drained exactly once".into());
                }
                if runtime
                    .host()
                    .accepted_per_pass
                    .iter()
                    .copied()
                    .filter(|count| *count != 0)
                    .collect::<Vec<_>>()
                    != [1, 1]
                {
                    return Err(format!(
                        "unexpected callback admission passes: {:?}",
                        runtime.host().accepted_per_pass
                    ));
                }
                drop(runtime);
                self.driver
                    .wake_handle()
                    .request_close()
                    .map_err(|error| error.to_string())?;
                self.phase = 2;
            }
            2 if self.handle.lifecycle() == HostLifecycle::CleanupFailed => {
                self.handle
                    .try_with(|_| ())
                    .map_err(|error| error.to_string())?;
                self.driver
                    .wake_handle()
                    .request_close()
                    .map_err(|error| error.to_string())?;
                self.phase = 3;
            }
            3 if self.handle.lifecycle() == HostLifecycle::Closed => {
                let runtime = self.runtime.borrow();
                if runtime.host().session_closes.get() != 1
                    || runtime.host().parent_closes.get() != 1
                {
                    return Err("consumer cleanup repeated a completed step".into());
                }
                let mut diagnostics = self.driver.presentation_errors();
                diagnostics.sort();
                if !self.reentrant_checked
                    || diagnostics
                        != [
                            "native host access: Busy",
                            "parent cleanup temporarily failed",
                        ]
                {
                    return Err(format!(
                        "unexpected presentation diagnostics: {:?}",
                        self.driver.presentation_errors()
                    ));
                }
                if !matches!(
                    self.driver.wake_handle().wake(),
                    Err(HostAccessError::Closed)
                ) {
                    return Err("closed wake accepted work".into());
                }
                println!("host_os_callbacks=2 cleanup_session=1 cleanup_parent=1 lifecycle=Closed");
                return Ok(true);
            }
            _ => (),
        }
        Ok(false)
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.can_create_surfaces(event_loop);
        self.tick(event_loop);
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.destroy_surfaces(event_loop);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let redraw = matches!(event, WindowEvent::RedrawRequested);
        let turns = self.runtime.borrow().host().accepted_per_pass.len();
        self.driver.window_event(event_loop, id, event);
        if redraw && !self.reentrant_checked {
            let handle = self.handle.clone();
            handle
                .try_with(|_| {
                    self.driver
                        .window_event(event_loop, id, WindowEvent::RedrawRequested)
                })
                .unwrap();
            self.reentrant_checked = true;
        }
        if redraw && self.runtime.borrow().host().accepted_per_pass.len() != turns {
            *self.result.borrow_mut() = Some(Err("pure redraw created a host turn".into()));
            event_loop.exit();
            return;
        }
        self.tick(event_loop);
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.driver.proxy_wake_up(event_loop);
        self.tick(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.tick(event_loop);
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline));
    }
}

#[test]
fn actual_os_driver_presents_reprojects_drains_capacity_and_retries_close() {
    let (handle, mut runtime, action) = consumer();
    runtime.host_mut().fail_parent_once = true;
    let runtime = Rc::new(RefCell::new(runtime));
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let driver =
        NativeWindowDriver::attach(&event_loop, runtime.clone(), Default::default()).unwrap();
    let result = Rc::new(RefCell::new(None));
    let probe = Probe {
        driver,
        runtime: runtime.clone(),
        handle: handle.clone(),
        action,
        phase: 0,
        first_scene: 0,
        reentrant_checked: false,
        deadline: Instant::now() + Duration::from_secs(15),
        result: result.clone(),
    };
    event_loop.run_app(probe).unwrap();
    result
        .borrow_mut()
        .take()
        .expect("OS host probe did not finish")
        .unwrap();
    assert_eq!(handle.lifecycle(), HostLifecycle::Closed);
    assert_eq!(runtime.borrow().host().completed, 2);
    assert!(matches!(
        handle.try_with(|_| ()),
        Err(HostAccessError::Closed)
    ));
}
