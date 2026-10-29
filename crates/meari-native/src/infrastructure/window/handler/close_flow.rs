//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::*;
use crate::infrastructure::native_host::{self, access};
use accesskit_xplat::{EventHandler, WindowEvent as AccessibilityEvent};
use anyrender::WindowRenderer;
use blitz_shell::BlitzShellEvent;
use parent::NativeFrameObservation;
use winit::{event_loop::ActiveEventLoop, window::WindowId};

impl EventHandler for Handler {
    fn handle_accesskit_event(&self, event: AccessibilityEvent) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(A11yEvent {
                window: self.window,
                event,
            }));
    }
}

impl Application {
    pub(in super::super) fn poll_owned_timers(&mut self) -> Option<std::time::Instant> {
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

    pub(in super::super) fn report(&self, error: String) {
        self.errors
            .lock()
            .expect("native diagnostics lock")
            .push(error);
    }

    pub(in super::super) fn after_dispatch(
        &mut self,
        id: WindowId,
        reason: native_host::HostTurnReason,
    ) {
        self.observe_dispatch(id, reason, false);
    }

    pub(in super::super) fn observe_dispatch(
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

    pub(in super::super) fn close_window(
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

    pub(in super::super) fn window_for_host(&self, host: u64) -> Option<WindowId> {
        self.inner.windows.iter().find_map(|(id, view)| {
            access::handle(view.doc.as_ref())
                .filter(|handle| handle.id() == host)
                .map(|_| *id)
        })
    }

    pub(in super::super) fn close_host(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        host: u64,
        cause: native_host::CloseCause,
    ) {
        self.observe_close(event_loop, host, cause, true);
    }

    pub(in super::super) fn retire_close_input(&self, host: u64) {
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

    pub(in super::super) fn observe_close(
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

    pub(in super::super) fn checked_render(&mut self, id: WindowId, resume: bool) {
        let Some(view) = self.inner.windows.get_mut(&id) else {
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = self.frames.entry(id) {
            match super::super::super::checked_window::WindowFrames::new() {
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
        let rendered = super::super::super::checked_window::render(view, frames, resume);
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
}
