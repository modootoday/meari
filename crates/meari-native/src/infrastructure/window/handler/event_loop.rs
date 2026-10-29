//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::*;
use crate::infrastructure::{
    document::InteractiveDocument,
    native_host::{self, access},
};
use blitz_shell::BlitzShellEvent;
use driver::NativeWindowDriver;
use std::time::Duration;
use wake::NativeWindowWake;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop,
    window::WindowId,
};

impl ApplicationHandler for Application {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        for id in self.inner.windows.keys().copied().collect::<Vec<_>>() {
            self.resume_window(id);
        }
        for config in std::mem::take(&mut self.inner.pending_windows) {
            let view = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                blitz_shell::View::init(config, event_loop, &self.inner.proxy)
            })) {
                Ok(view) => view,
                Err(_) => {
                    for host in self.hosts.keys().copied().collect::<Vec<_>>() {
                        self.creation_failed(host, "native View initialization panicked; runtime retained for explicit close".into());
                    }
                    continue;
                }
            };
            let id = view.window_id();
            let host = access::handle(view.doc.as_ref()).map(|handle| handle.id());
            let window = view.window.clone();
            self.inner.windows.insert(id, view);
            if let Some(lifecycle) = host.and_then(|host| self.lifecycles.get(&host))
                && let Err(error) = lifecycle.created(window)
            {
                self.report(error);
            }
            self.resume_window(id);
        }
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.restore_window_cursors();
        // Reentrant suspend observers must not authenticate the previous surface.
        self.frames.clear();
        for lifecycle in self.lifecycles.values() {
            if let Err(error) = lifecycle.suspend() {
                self.report(error);
            }
        }
        self.inner.destroy_surfaces(event_loop);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(test)]
        if matches!(event, WindowEvent::Destroyed)
            && let Some(trace) = self.timer_destroy_traces.remove(&id)
            && let Ok(mut book) = trace.lock()
        {
            book.trace.destroyed = Some(std::time::Instant::now());
        }
        if matches!(event, WindowEvent::Destroyed) && self.pending_removals.remove(&id) {
            if self.exit_when_empty && self.hosts.is_empty() && self.pending_removals.is_empty() {
                event_loop.exit();
            }
            return;
        }
        // Physical destruction revokes handles even if the runtime/document is currently borrowed.
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.close_window(
                event_loop,
                id,
                if matches!(event, WindowEvent::Destroyed) {
                    native_host::CloseCause::ForcedOsDestroy
                } else {
                    native_host::CloseCause::User
                },
            );
            if matches!(event, WindowEvent::Destroyed) {
                self.pending_removals.remove(&id);
            }
            return;
        }
        self.record_window_input(id, &event);
        if self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
            .is_some_and(|handle| !handle.can_enter_engine())
        {
            self.report(native_host::HostAccessError::Busy.to_string());
            return;
        }
        let host_changed = matches!(
            &event,
            WindowEvent::SurfaceResized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::ThemeChanged(_)
        );
        if let Some(adapter) = self.adapters.get_mut(&id) {
            if let WindowEvent::Focused(focused) = &event {
                adapter.set_focus(*focused);
            }
            // Viewport changes are projected on the next redraw. OS bounds are updated below.
            if let Some(view) = self.inner.windows.get(&id) {
                let position = view
                    .window
                    .outer_position()
                    .unwrap_or_default()
                    .cast::<f64>();
                let outer = view.window.outer_size().cast::<f64>();
                let inner_position = view.window.surface_position().cast::<f64>();
                let inner = view.window.surface_size().cast::<f64>();
                adapter.set_window_bounds(
                    accesskit::Rect::from_origin_size(
                        (position.x, position.y),
                        (outer.width, outer.height),
                    ),
                    accesskit::Rect::from_origin_size(
                        (inner_position.x, inner_position.y),
                        (inner.width, inner.height),
                    ),
                );
            }
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            self.checked_render(id, false);
            self.inner
                .proxy
                .send_event(BlitzShellEvent::Poll { window_id: id });
            self.files(id);
            self.update(id);
            return;
        }
        self.inner.window_event(event_loop, id, event);
        if host_changed {
            self.refresh_host(id);
        }
        self.files(id);
        if self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
            .is_some_and(|handle| handle.take_dispatched_input())
        {
            self.after_dispatch(id, native_host::HostTurnReason::Input);
        }
        self.update(id);
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.inner.event_queue.try_recv() {
            if let BlitzShellEvent::CloseWindow { window_id } = &event {
                self.close_window(event_loop, *window_id, native_host::CloseCause::Host);
                continue;
            }
            if let BlitzShellEvent::ResumeReady { window_id } = &event {
                self.checked_render(*window_id, true);
                self.update(*window_id);
                continue;
            }
            if let BlitzShellEvent::Embedder(data) = &event {
                if let Some(signal) = data.downcast_ref::<wake::HostWakeEvent>() {
                    let mut bits = self
                        .wakes
                        .get(&signal.host)
                        .map_or(0, NativeWindowWake::take);
                    if bits & (wake::CLOSE | wake::USER_CLOSE) != 0 {
                        let requested = if bits & wake::USER_CLOSE != 0 {
                            self.wakes
                                .get(&signal.host)
                                .and_then(|wake| wake.user_close.take())
                        } else {
                            None
                        };
                        if bits & wake::CLOSE != 0 {
                            self.close_host(event_loop, signal.host, native_host::CloseCause::Host);
                            continue;
                        }
                        let current = self
                            .actions
                            .get(&signal.host)
                            .ok_or(crate::application::window_actions::ActionError::NotReady)
                            .and_then(|binding| binding.current());
                        match user_close::validate(requested, current) {
                            Ok(()) => self.close_host(
                                event_loop,
                                signal.host,
                                native_host::CloseCause::User,
                            ),
                            Err(error) => self.report(error.to_string()),
                        }
                        let owner_open = self.hosts.get(&signal.host).is_some_and(|port| {
                            port.document().lifecycle() == native_host::HostLifecycle::Open
                        });
                        bits = user_close::continuation(bits, owner_open);
                        if bits == 0 {
                            continue;
                        }
                    }
                    if bits & wake::LIFECYCLE != 0 {
                        self.lifecycle_turn(event_loop, signal.host);
                    }
                    if bits & wake::ACTIONS != 0
                        && let Some(id) = self.window_for_host(signal.host)
                    {
                        self.drive_window_actions(id);
                    }
                    let id = self.window_for_host(signal.host);
                    if let Some(id) = id {
                        let closing = self.hosts.get(&signal.host).is_some_and(|port| {
                            port.document().lifecycle() != native_host::HostLifecycle::Open
                        }) || self.close_observation_pending(signal.host);
                        if closing && bits & wake::COMPLETE != 0 {
                            self.observe_close(
                                event_loop,
                                signal.host,
                                native_host::CloseCause::Host,
                                false,
                            );
                        } else if bits & (wake::COMPLETE | wake::RETRY) != 0 {
                            self.observe_dispatch(
                                id,
                                native_host::HostTurnReason::Completion,
                                bits & wake::RETRY != 0,
                            );
                            self.update(id);
                        }
                    }
                    continue;
                }
                if data.is::<Stop>() {
                    for host in self.hosts.keys().copied().collect::<Vec<_>>() {
                        self.close_host(event_loop, host, native_host::CloseCause::ForcedOsDestroy);
                    }
                    event_loop.exit();
                    return;
                }
                if let Some(event) = data.downcast_ref::<A11yEvent>() {
                    self.accessibility(event);
                    continue;
                }
            }
            if self
                .inner
                .windows
                .values()
                .filter_map(|view| access::handle(view.doc.as_ref()))
                .any(|handle| !handle.can_enter_engine())
            {
                self.report(native_host::HostAccessError::Busy.to_string());
                continue;
            }
            self.inner.handle_blitz_shell_event(event_loop, event);
        }
        self.frames
            .retain(|id, _| self.inner.windows.contains_key(id));
        self.adapters
            .retain(|id, _| self.inner.windows.contains_key(id));
        for id in self.inner.windows.keys().copied().collect::<Vec<_>>() {
            self.update(id);
        }
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.poll_window_actions();
        self.inner.about_to_wait(event_loop);
        use crate::application::window_lifecycle::MonotonicClock;
        let elapsed = self.clock.now();
        let due_close = self
            .closes
            .iter()
            .filter(|(_, state)| state.due(elapsed))
            .map(|(host, _)| *host)
            .collect::<Vec<_>>();
        for host in due_close {
            self.observe_close(event_loop, host, native_host::CloseCause::Host, false);
        }
        let due_turn = self
            .turns
            .iter()
            .filter(|(_, state)| state.due(elapsed))
            .map(|(host, _)| *host)
            .collect::<Vec<_>>();
        for host in due_turn {
            if let Some(id) = self.window_for_host(host) {
                self.observe_dispatch(id, native_host::HostTurnReason::Completion, false);
            }
        }
        let now = std::time::Instant::now();
        let due = self
            .lifecycles
            .iter()
            .filter_map(|(host, lifecycle)| match lifecycle.next_deadline() {
                Ok(Some(deadline)) if deadline <= now => Some(*host),
                Ok(_) => None,
                Err(error) => {
                    self.report(error);
                    None
                }
            })
            .collect::<Vec<_>>();
        for host in due {
            self.lifecycle_turn(event_loop, host);
        }
        let next = self
            .lifecycles
            .values()
            .filter_map(|lifecycle| match lifecycle.next_deadline() {
                Ok(deadline) => deadline,
                Err(error) => {
                    self.report(error);
                    None
                }
            })
            .min();
        let mut deadline_errors = Vec::new();
        let host_next = self
            .turns
            .values_mut()
            .chain(self.closes.values_mut())
            .filter_map(|state| match state.absolute_deadline(self.clock.origin) {
                Ok(next) => next,
                Err(error) => {
                    deadline_errors.push(error);
                    None
                }
            })
            .min();
        for error in deadline_errors {
            self.report(error.to_string());
        }
        let timer_next = self.poll_owned_timers();
        let next = next.into_iter().chain(host_next).chain(timer_next).min();
        scheduling::apply_deadline(event_loop, next, &mut self.lifecycle_deadline);
    }
}

pub fn show(document: InteractiveDocument, seconds: Option<u64>) -> Result<(), String> {
    show_with_options(
        document,
        seconds,
        crate::application::WindowOptions::default(),
    )
}

pub fn show_with_options(
    document: InteractiveDocument,
    seconds: Option<u64>,
    options: crate::application::WindowOptions,
) -> Result<(), String> {
    if document.callback_scope().is_some() {
        return Err("callback documents require show_with_runtime and a consumer host".into());
    }
    let handle =
        native_host::NativeDocumentHandle::new(document, 1).map_err(|error| error.to_string())?;
    let runtime = std::rc::Rc::new(std::cell::RefCell::new(
        native_host::NativeHostRuntime::new(handle, native_host::window_port::StandaloneHost),
    ));
    println!("native_window_pid={}", std::process::id());
    let event_loop = blitz_shell::create_default_event_loop();
    let driver = NativeWindowDriver::build(&event_loop, runtime, options, true)?;
    let proxy = driver.application.inner.proxy.clone();
    if let Some(seconds) = seconds {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds.clamp(1, 600)));
            proxy.send_event(BlitzShellEvent::embedder_event(Stop));
        });
    }
    let errors = driver.application.errors.clone();
    event_loop.run_app(driver).map_err(|e| e.to_string())?;
    let errors = errors.lock().map_err(|e| e.to_string())?;
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(())
}

pub fn show_with_runtime<H: native_host::NativeWindowHost>(
    runtime: std::rc::Rc<std::cell::RefCell<native_host::NativeHostRuntime<H>>>,
    options: crate::application::WindowOptions,
) -> Result<(), String> {
    let event_loop = blitz_shell::create_default_event_loop();
    let driver = NativeWindowDriver::build(&event_loop, runtime, options, true)?;
    let errors = driver.application.errors.clone();
    event_loop
        .run_app(driver)
        .map_err(|error| error.to_string())?;
    let errors = errors.lock().map_err(|error| error.to_string())?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
