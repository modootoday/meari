//! Winit/softbuffer surface with our AccessKit action handler, rather than the upstream TODO.
use crate::infrastructure::{
    accessibility,
    document::InteractiveDocument,
    native_host::{self, access},
};
use accesskit_xplat::{Adapter, EventHandler, WindowEvent as AccessibilityEvent};
use anyrender::WindowRenderer;
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasWindowHandle,
    window::{WindowAttributes, WindowId},
};

struct A11yEvent {
    window: WindowId,
    event: AccessibilityEvent,
}
struct Handler {
    window: WindowId,
    proxy: BlitzShellProxy,
}
impl EventHandler for Handler {
    fn handle_accesskit_event(&self, event: AccessibilityEvent) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(A11yEvent {
                window: self.window,
                event,
            }));
    }
}
struct Stop;
mod actions;
mod driver;
mod input;
pub use actions::NativeWindowActions;
mod forwarding;
mod lifecycle;
mod parent;
mod parent_client;
#[cfg(feature = "test-support")]
pub use parent::test_support;
pub use parent_client::{NativeCheckedParentClient, NativeParentClientError};
mod plain_lifecycle;
mod preparation;
mod scheduling;
mod timers;
mod user_close;
pub use timers::{NativeHostTimer, NativeHostTimerToken};
mod startup;
mod surface_slots;
pub use startup::{
    NativeAttachment, NativeLoopOwner, NativeRunError, NativeRunOutcome, NativeStartup,
    NativeStartupAttachFailure, NativeStartupContext, NativeStartupController, NativeStartupError,
    NativeStartupPoll, NativeStartupStamp, NativeStartupWake, loop_api,
};
pub use surface_slots::{
    NativeSurfaceAccessError, NativeSurfaceRegistrar, NativeSurfaceRegistrationFailure,
    NativeSurfaceReservation, NativeSurfaceSlot, NativeSurfaceSnapshot,
};
mod wake;
pub use driver::{NativeFrameSubmission, NativeWindowDriver};
pub use parent::{
    NativeChildRegistrar, NativeFrameObservation, NativeNoticeError, NativeParentLease,
    NativeScopedParent, NativeWindowGeometry, NativeWindowNotice, NativeWindowNoticeKind,
    NativeWindowObservation, NativeWindowObserver,
};
pub use preparation::{
    NativeAttachFailure, NativeAttachProblem, NativeAttachStage, NativeWindowPreparation,
};
pub use wake::NativeWindowWake;
struct Application {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    adapters: HashMap<WindowId, Adapter>,
    errors: Arc<std::sync::Mutex<Vec<String>>>,
    frames: HashMap<WindowId, super::checked_window::WindowFrames>,
    hosts: HashMap<u64, Box<dyn native_host::window_port::WindowHostPort>>,
    actions: HashMap<u64, std::rc::Rc<actions::ActionBinding>>,
    wakes: HashMap<u64, NativeWindowWake>,
    lifecycles: HashMap<u64, Box<dyn lifecycle::WindowLifecyclePort>>,
    clock: scheduling::NativeClock,
    turns: HashMap<u64, scheduling::HostSchedule>,
    closes: HashMap<u64, scheduling::HostSchedule>,
    pending_removals: HashSet<WindowId>,
    #[cfg(test)]
    timer_destroy_traces: HashMap<WindowId, Arc<std::sync::Mutex<timers::TimerBook>>>,
    lifecycle_deadline: Option<(std::time::Instant, winit::event_loop::ControlFlow)>,
    exit_when_empty: bool,
}

impl Application {
    fn poll_owned_timers(&mut self) -> Option<std::time::Instant> {
        let now = std::time::Instant::now();
        let mut next = None;
        let mut due = Vec::new();
        for (host, wake) in &self.wakes {
            let Some(port) = self.hosts.get(host) else {
                continue;
            };
            let document = port.document();
            let Ok(mut timers) = wake.timers.lock() else {
                continue;
            };
            if document.lifecycle() != native_host::HostLifecycle::Open {
                timers.cancel_all();
                continue;
            }
            let scope = match document.stamp() {
                Ok(scope) => scope,
                // Original owner will wake after its current transaction; do not mint a stale turn.
                Err(_) => continue,
            };
            let busy = self.window_for_host(*host).is_none()
                || !port.completion_idle()
                || self.turns.get(host).is_some_and(|state| state.dirty);
            let (fired, deadline) = timers.poll(now, &scope, busy);
            if fired {
                due.push(*host);
            }
            next = next.into_iter().chain(deadline).min();
        }
        for host in due {
            if let Some(id) = self.window_for_host(host) {
                self.observe_dispatch(id, native_host::HostTurnReason::Completion, false);
                #[cfg(test)]
                if let Some(wake) = self.wakes.get(&host)
                    && let Ok(mut timers) = wake.timers.lock()
                {
                    timers.trace.finish_skipped();
                }
                self.update(id);
            }
        }
        next
    }
    fn report(&self, error: String) {
        self.errors
            .lock()
            .expect("native diagnostics lock")
            .push(error);
    }
    fn after_dispatch(&mut self, id: WindowId, reason: native_host::HostTurnReason) {
        self.observe_dispatch(id, reason, false);
    }
    fn observe_dispatch(
        &mut self,
        id: WindowId,
        reason: native_host::HostTurnReason,
        explicit: bool,
    ) {
        let Some(handle) = self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
        else {
            return;
        };
        handle.take_dispatched_input();
        let schedule = self.turns.entry(handle.id()).or_default();
        if !schedule.ready_at(self.clock.origin.elapsed(), explicit) || !schedule.begin(explicit) {
            return;
        }
        let Some(port) = self.hosts.get(&handle.id()) else {
            return;
        };
        let outcome = port.after_dispatch(reason);
        #[cfg(test)]
        if let Some(wake) = self.wakes.get(&handle.id())
            && let Ok(mut timers) = wake.timers.lock()
        {
            timers.trace.complete(
                outcome
                    .as_ref()
                    .cloned()
                    .map_err(|error| error.error.clone()),
            );
        }
        match outcome {
            Ok(receipt) => {
                self.turns.entry(handle.id()).or_default().complete();
                if receipt.redraw
                    && let Some(view) = self.inner.windows.get(&id)
                {
                    view.window.request_redraw();
                }
                let document_pending = receipt.remaining_callbacks == 0
                    && handle
                        .try_with(|document| document.has_pending_callbacks())
                        .unwrap_or(false);
                if ((receipt.remaining_callbacks > 0 && receipt.admitted_this_pass > 0)
                    || document_pending)
                    && let Some(wake) = self.wakes.get(&handle.id())
                {
                    let _ = wake.wake();
                }
            }
            Err(error) => {
                use crate::application::window_lifecycle::MonotonicClock;
                let schedule = self.turns.entry(handle.id()).or_default();
                let retry_error = if let Some(retry) = error.retry {
                    schedule
                        .defer(self.clock.now(), retry, port.schedule_limits())
                        .err()
                } else {
                    schedule.fail();
                    None
                };
                self.report(error.to_string());
                if let Some(error) = retry_error {
                    self.report(error.to_string());
                }
            }
        }
    }
    fn close_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: WindowId,
        cause: native_host::CloseCause,
    ) {
        let Some(handle) = self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
        else {
            return;
        };
        self.close_host(event_loop, handle.id(), cause);
    }
    fn window_for_host(&self, host: u64) -> Option<WindowId> {
        self.inner.windows.iter().find_map(|(id, view)| {
            access::handle(view.doc.as_ref())
                .filter(|handle| handle.id() == host)
                .map(|_| *id)
        })
    }
    fn close_host(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        host: u64,
        cause: native_host::CloseCause,
    ) {
        self.observe_close(event_loop, host, cause, true);
    }
    fn retire_close_input(&self, host: u64) {
        if let Some(id) = self.window_for_host(host)
            && let (Some(binding), Some(view)) =
                (self.actions.get(&host), self.inner.windows.get(&id))
        {
            binding.restore_cursor(view.window.as_ref());
            binding.retire_input();
        }
        if let Some(wake) = self.wakes.get(&host)
            && let Ok(mut timers) = wake.timers.lock()
        {
            timers.cancel_all();
        }
    }
    fn observe_close(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        host: u64,
        cause: native_host::CloseCause,
        explicit: bool,
    ) {
        if cause != native_host::CloseCause::User {
            self.retire_close_input(host);
        }
        let schedule = self.closes.entry(host).or_default();
        if cause != native_host::CloseCause::ForcedOsDestroy
            && (!schedule.ready_at(self.clock.origin.elapsed(), explicit)
                || !schedule.begin(explicit))
        {
            return;
        }
        let Some(port) = self.hosts.get(&host) else {
            return;
        };
        let forced = cause == native_host::CloseCause::ForcedOsDestroy;
        if forced
            && let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.forced()
        {
            self.report(error);
        }
        let status = port.close(cause, explicit);
        // User veto/error while still Open must not retire its original input/timer owner.
        if cause == native_host::CloseCause::User
            && port.document().lifecycle() != native_host::HostLifecycle::Open
        {
            self.retire_close_input(host);
        }
        use crate::application::window_lifecycle::MonotonicClock;
        let schedule = self.closes.entry(host).or_default();
        let retry_error = match &status {
            Ok((
                native_host::HostCloseStatus::Pending
                | native_host::HostCloseStatus::ObservationUnavailable,
                retry,
            )) => schedule
                .defer(
                    self.clock.now(),
                    retry.unwrap_or(native_host::HostRetry::OnWake),
                    port.schedule_limits(),
                )
                .err(),
            Ok((native_host::HostCloseStatus::RetryableFailure, _)) => {
                schedule.fail();
                None
            }
            Err(error) => {
                if let Some(retry) = error.retry {
                    schedule
                        .defer(self.clock.now(), retry, port.schedule_limits())
                        .err()
                } else {
                    schedule.fail();
                    None
                }
            }
            _ => {
                schedule.complete();
                None
            }
        };
        if let Some(error) = retry_error {
            self.report(error.to_string());
        }
        if !forced
            && port.document().lifecycle() != native_host::HostLifecycle::Open
            && let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.closing()
        {
            self.report(error);
        }
        if matches!(
            status,
            Ok((
                native_host::HostCloseStatus::RetryableFailure
                    | native_host::HostCloseStatus::ObservationUnavailable,
                _
            ))
        ) && let Some(error) = port.host_error()
        {
            self.report(error);
        }
        let remove = forced
            || matches!(
                status,
                Ok((native_host::HostCloseStatus::HostCleanupComplete, _))
            );
        if let Err(error) = status {
            self.report(error.to_string());
        }
        if !remove {
            return;
        }
        if let Some(id) = self.window_for_host(host) {
            #[cfg(test)]
            if let Some(wake) = self.wakes.get(&host) {
                self.timer_destroy_traces.insert(id, wake.timers.clone());
            }
            self.pending_removals.insert(id);
            self.adapters.remove(&id);
            self.frames.remove(&id);
            self.inner.windows.remove(&id);
        } else {
            // Each driver owns exactly one pending config/host; no view exists after creation failure.
            self.inner.pending_windows.clear();
        }
        if let Some(port) = self.hosts.get(&host)
            && let Err(error) = port.removed(forced)
        {
            self.report(error);
            // The runtime still owns an unfinished removal; explicit request_close can retry it.
            return;
        }
        if let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.removed(forced)
        {
            self.report(error);
        }
        self.hosts.remove(&host);
        self.turns.remove(&host);
        self.closes.remove(&host);
        self.lifecycles.remove(&host);
        self.actions.remove(&host);
        if let Some(wake) = self.wakes.remove(&host) {
            wake.close();
        }
        if self.exit_when_empty && self.inner.windows.is_empty() && self.pending_removals.is_empty()
        {
            event_loop.exit();
        }
    }
    fn checked_render(&mut self, id: WindowId, resume: bool) {
        let Some(view) = self.inner.windows.get_mut(&id) else {
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = self.frames.entry(id) {
            match super::checked_window::WindowFrames::new() {
                Ok(frames) => {
                    entry.insert(frames);
                }
                Err(error) => {
                    self.errors
                        .lock()
                        .expect("native diagnostics lock")
                        .push(error);
                    return;
                }
            }
        }
        let frames = self
            .frames
            .get_mut(&id)
            .expect("window frame owner inserted");
        let document_stamp = access::handle(view.doc.as_ref()).map(|handle| handle.stamp());
        let previous_ticket = frames.submission().map(|submission| submission.ticket);
        let rendered = super::checked_window::render(view, frames, resume);
        let submitted = frames
            .submission()
            .filter(|submission| Some(&submission.ticket) != previous_ticket.as_ref());
        let retained = frames.retained();
        let ready = rendered.is_ok()
            && view.renderer.is_active()
            && !view.renderer.is_pending()
            && view.window.surface_size().width > 0
            && view.window.surface_size().height > 0;
        let host = access::handle(view.doc.as_ref()).map(|handle| handle.id());
        if let Err(error) = rendered {
            let error = frames.failure(&error);
            let _ = access::with_mut(view.doc.as_mut(), |document| {
                document.last_error = Some(error.clone())
            });
            let mut errors = self.errors.lock().expect("native diagnostics lock");
            if errors.last() != Some(&error) {
                errors.push(error);
            }
        }
        if ready
            && let Some(lifecycle) = host.and_then(|host| self.lifecycles.get(&host))
            && let Err(error) = lifecycle.renderer_ready()
        {
            self.report(error);
        }
        if let (Some(Ok(document)), Some(submission), Some(lifecycle)) = (
            document_stamp,
            submitted,
            host.and_then(|host| self.lifecycles.get(&host)),
        ) && let Err(error) = lifecycle.submitted(NativeFrameObservation {
            document,
            submission,
            retained,
        }) {
            self.report(error);
        }
    }
    fn refresh_host(&mut self, id: WindowId) {
        let Some(view) = self.inner.windows.get_mut(&id) else {
            return;
        };
        if view.window.surface_size().width == 0 || view.window.surface_size().height == 0 {
            return;
        }
        let result = access::with_mut(view.doc.as_mut(), |document| {
            let zoom = f64::from(document.renderer.document.get_viewport().zoom);
            let result = super::host::snapshot_with_zoom(view.window.as_ref(), zoom)
                .and_then(|environment| document.commit_host_environment(environment));
            if let Err(error) = &result {
                document.last_error = Some(error.clone());
            }
            result
        })
        .map_err(|error| error.to_string())
        .and_then(|result| result);
        if let Err(error) = result {
            self.errors
                .lock()
                .expect("native diagnostics lock")
                .push(error);
        }
        // A separate observation after the mutable document guard has been released.
        let observation = self.inner.windows.get(&id).and_then(|view| {
            let handle = access::handle(view.doc.as_ref())?;
            let zoom = handle
                .try_with(|document| f64::from(document.renderer.document.get_viewport().zoom));
            Some((
                handle.id(),
                zoom.map_err(|error| error.to_string())
                    .and_then(|zoom| super::host::snapshot_with_zoom(view.window.as_ref(), zoom)),
            ))
        });
        if let Some((host, environment)) = observation {
            let result = environment.and_then(|environment| {
                self.lifecycles
                    .get(&host)
                    .map_or(Ok(()), |lifecycle| lifecycle.geometry(environment))
            });
            if let Err(error) = result {
                self.report(error);
            }
        }
    }
    fn lifecycle_turn(&mut self, event_loop: &dyn ActiveEventLoop, host: u64) {
        if let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.poll()
        {
            self.report(error);
        }
        // Completion may continue Pending, but never automatically retry a failed business/child close.
        if self.close_observation_pending(host) {
            self.observe_close(event_loop, host, native_host::CloseCause::Host, false);
        }
    }
    fn close_observation_pending(&self, host: u64) -> bool {
        self.hosts
            .get(&host)
            .is_some_and(|port| port.document().lifecycle() == native_host::HostLifecycle::Closing)
            || self
                .closes
                .get(&host)
                .is_some_and(|schedule| schedule.dirty && !schedule.failed)
    }
    fn creation_failed(&self, host: u64, error: String) {
        self.report(error);
        if let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.failed()
        {
            self.report(error);
        }
    }
    fn resume_window(&mut self, id: WindowId) {
        let host = self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
            .map(|handle| handle.id());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<Option<Adapter>, String> {
                let view = self
                    .inner
                    .windows
                    .get_mut(&id)
                    .ok_or("native view disappeared")?;
                // AccessKit subclasses the native window once, before its first visibility.
                // Surface suspension does not end that window's accessibility lifetime.
                let adapter = if self.adapters.contains_key(&id) {
                    None
                } else {
                    let handle = view
                        .window
                        .window_handle()
                        .map_err(|error| error.to_string())?;
                    Some(Adapter::with_combined_handler(
                        handle.as_raw(),
                        Arc::new(Handler {
                            window: id,
                            proxy: self.inner.proxy.clone(),
                        }),
                    ))
                };
                view.resume();
                view.window.set_visible(true);
                view.is_visible = true;
                view.window.request_redraw();
                Ok(adapter)
            },
        ))
        .unwrap_or_else(|_| {
            Err(
                "native accessibility/renderer initialization panicked; partial owner retained"
                    .into(),
            )
        });
        match result {
            Ok(adapter) => {
                if let Some(adapter) = adapter {
                    self.adapters.insert(id, adapter);
                }
                self.refresh_host(id);
            }
            Err(error) => {
                if let Some(host) = host {
                    self.creation_failed(host, error);
                } else {
                    self.report(error);
                }
            }
        }
    }
    fn update(&mut self, window: WindowId) {
        let Some(view) = self.inner.windows.get(&window) else {
            return;
        };
        if let Some(adapter) = self.adapters.get_mut(&window) {
            let _ = access::with(view.doc.as_ref(), |document| {
                adapter.update_if_active(|| accessibility::tree(document))
            });
        }
    }
    fn accessibility(&mut self, event: &A11yEvent) {
        let mut dispatched = false;
        if let AccessibilityEvent::ActionRequested(request) = &event.event
            && let Some(view) = self.inner.windows.get_mut(&event.window)
        {
            if access::handle(view.doc.as_ref()).is_some_and(|handle| !handle.can_dispatch_input())
            {
                return;
            }
            let result = match access::handle(view.doc.as_ref()) {
                Some(handle) => handle
                    .with_native_input(|document| accessibility::act(document, request))
                    .map_err(|error| error.to_string()),
                None => access::with_mut(view.doc.as_mut(), |document| {
                    accessibility::act(document, request)
                })
                .map_err(|error| error.to_string())
                .and_then(|result| result),
            };
            if let Err(error) = result {
                let error = format!(
                    "accessibility {:?} target {:?}: {error}",
                    request.action, request.target_node
                );
                eprintln!("{error}");
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error);
            } else if let Some(handle) = access::handle(view.doc.as_ref()) {
                if let Err(error) = handle.note_native_action() {
                    eprintln!("native host action: {error}");
                } else {
                    dispatched = true;
                }
            }
            view.window.request_redraw();
            if request.action == accesskit::Action::Focus {
                view.window.focus_window();
            }
        }
        if dispatched {
            self.files(event.window);
            self.after_dispatch(event.window, native_host::HostTurnReason::Accessibility);
        }
        self.update(event.window);
    }
    fn files(&mut self, id: WindowId) {
        if let Some(view) = self.inner.windows.get_mut(&id) {
            let result = access::with_mut(view.doc.as_mut(), |document| {
                if document.file_requests.is_empty() {
                    return Ok(false);
                }
                document.process_file_requests(&mut super::file_picker::NativeFilePicker)?;
                Ok::<_, String>(true)
            })
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            if let Err(error) = &result {
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error.clone());
            }
            if matches!(result, Ok(true)) {
                view.window.request_redraw();
            }
        }
    }
}

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
