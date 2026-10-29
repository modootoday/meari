use super::{HostRetry, HostScheduleLimits, NativeDocumentHandle, types::*};
use meari::application::host_callbacks::PendingCallback;

/// No Send/Sync requirement: product registries and document sessions stay on the UI thread.
pub trait NativeWindowHost: 'static {
    type Error: std::error::Error + 'static;
    type CloseCheckpoint: 'static;
    fn admit_callback(
        &mut self,
        stamp: &HostStamp,
        callback: &PendingCallback,
    ) -> IntentAdmission<Self::Error>;
    fn after_dispatch(&mut self, turn: HostTurn) -> Result<HostEffects, Self::Error>;
    fn retry_after_dispatch(&self, _error: &Self::Error) -> Option<HostRetry> {
        None
    }
    fn next_close_observation(&self, _checkpoint: &Self::CloseCheckpoint) -> HostRetry {
        HostRetry::OnWake
    }
    fn can_close(&mut self, _stamp: &HostStamp, _cause: CloseCause) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn close(
        &mut self,
        request: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error>;
    /// Observe existing work without explicitly retrying a terminal failed step.
    fn observe_close(
        &mut self,
        request: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error> {
        self.close(request)
    }
}

/// Owns unadmitted intents during consumer backpressure. It never replays accepted SDK work.
pub struct NativeHostRuntime<H: NativeWindowHost> {
    handle: NativeDocumentHandle,
    host: H,
    pending: Option<PendingTurn>,
    close_checkpoint: Option<H::CloseCheckpoint>,
    close_cause: Option<CloseCause>,
    last_host_error: Option<H::Error>,
    attached: bool,
    pub(crate) schedule_limits: Option<HostScheduleLimits>,
    pub(crate) turn_retry: Option<HostRetry>,
    pub(crate) close_retry: Option<HostRetry>,
}
impl<H: NativeWindowHost> NativeHostRuntime<H> {
    pub fn new(handle: NativeDocumentHandle, host: H) -> Self {
        Self {
            handle,
            host,
            pending: None,
            close_checkpoint: None,
            close_cause: None,
            last_host_error: None,
            attached: false,
            schedule_limits: None,
            turn_retry: None,
            close_retry: None,
        }
    }
    pub fn with_schedule_limits(
        handle: NativeDocumentHandle,
        host: H,
        limits: HostScheduleLimits,
    ) -> Self {
        let mut runtime = Self::new(handle, host);
        runtime.schedule_limits = Some(limits);
        runtime
    }
    pub fn document(&self) -> NativeDocumentHandle {
        self.handle.clone()
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
    pub fn last_host_error(&self) -> Option<&H::Error> {
        self.last_host_error.as_ref()
    }
    pub fn pending_callbacks(&self) -> usize {
        self.pending.as_ref().map_or(0, |turn| turn.callbacks.len())
    }
    pub(crate) fn has_pending_turn(&self) -> bool {
        self.pending.is_some()
    }
    pub fn after_dispatch(
        &mut self,
        reason: HostTurnReason,
    ) -> Result<HostTurnReceipt, HostAccessError> {
        self.turn_retry = None;
        self.handle.take_dispatched_input();
        if self.handle.lifecycle() != HostLifecycle::Open {
            return Err(HostAccessError::Closing);
        }
        if self.pending.is_some()
            && !matches!(
                reason,
                HostTurnReason::ResumePending | HostTurnReason::Completion
            )
        {
            return Err(HostAccessError::Busy);
        }
        if self.pending.is_none() {
            let stamp = self.handle.next_turn()?;
            let callbacks = self
                .handle
                .with_internal_mut(|document| document.drain_callbacks())?;
            self.pending = Some(PendingTurn {
                stamp,
                callbacks: callbacks.into(),
                accepted: 0,
                rejected: 0,
            });
        }
        let turn = self
            .pending
            .as_mut()
            .expect("pending native host turn created");
        let accepted_before = turn.accepted;
        let rejected_before = turn.rejected;
        while let Some(callback) = turn.callbacks.front() {
            match self.host.admit_callback(&turn.stamp, callback) {
                IntentAdmission::Accepted => {
                    turn.callbacks.pop_front();
                    turn.accepted += 1;
                }
                IntentAdmission::Rejected(error) => {
                    turn.callbacks.pop_front();
                    turn.rejected += 1;
                    self.last_host_error = Some(error);
                }
                IntentAdmission::Capacity => break,
            }
        }
        let remaining = turn.callbacks.len();
        self.handle.0.input_blocked.set(remaining != 0);
        let before = self.handle.stamp()?;
        let metadata = HostTurn {
            stamp: before.clone(),
            reason,
            accepted_callbacks: turn.accepted - accepted_before,
            rejected_callbacks: turn.rejected - rejected_before,
            remaining_callbacks: remaining,
        };
        let effects = match self.host.after_dispatch(metadata) {
            Ok(effects) => effects,
            Err(error) => {
                self.turn_retry = self.host.retry_after_dispatch(&error);
                self.last_host_error = Some(error);
                if remaining == 0 {
                    self.pending = None;
                }
                return Err(HostAccessError::Invalid(
                    "native consumer host failed; typed error is retained".into(),
                ));
            }
        };
        let after = self.handle.stamp()?;
        if let Some(receipt) = &effects.committed {
            if receipt.before() != &before || receipt.after() != &after {
                return Err(HostAccessError::Stale);
            }
            if remaining != 0 {
                return Err(HostAccessError::Invalid(
                    "finish admitting a host batch before replacing its callback export".into(),
                ));
            }
        } else if before != after {
            return Err(HostAccessError::Invalid(
                "consumer reproject requires its synchronous commit receipt".into(),
            ));
        }
        let receipt = HostTurnReceipt {
            stamp: after,
            accepted_callbacks: turn.accepted,
            rejected_callbacks: turn.rejected,
            remaining_callbacks: remaining,
            redraw: effects.redraw,
            committed: effects.committed,
            admitted_this_pass: turn.accepted - accepted_before,
        };
        if remaining == 0 {
            self.pending = None;
        }
        Ok(receipt)
    }
    pub fn request_close(&mut self, cause: CloseCause) -> Result<HostCloseStatus, HostAccessError> {
        self.close_inner(cause, true)
    }
    /// Same close checkpoint, with no permission to restart terminal failed child work.
    pub fn observe_close(&mut self, cause: CloseCause) -> Result<HostCloseStatus, HostAccessError> {
        if matches!(
            self.handle.lifecycle(),
            HostLifecycle::Open | HostLifecycle::CleanupFailed
        ) {
            return Err(HostAccessError::Invalid(
                "close observation requires retained in-flight cleanup; explicit request required"
                    .into(),
            ));
        }
        self.close_inner(cause, false)
    }
    fn close_inner(
        &mut self,
        cause: CloseCause,
        explicit: bool,
    ) -> Result<HostCloseStatus, HostAccessError> {
        self.close_retry = None;
        if self.handle.lifecycle() == HostLifecycle::HostCleaned {
            return Ok(HostCloseStatus::HostCleanupComplete);
        }
        if matches!(
            self.handle.lifecycle(),
            HostLifecycle::Closed | HostLifecycle::ForcedIncomplete
        ) {
            return Ok(if self.handle.lifecycle() == HostLifecycle::Closed {
                HostCloseStatus::HostCleanupComplete
            } else {
                HostCloseStatus::ForcedIncomplete
            });
        }
        let stamp = self.handle.stamp()?;
        if self.handle.lifecycle() == HostLifecycle::Open && cause != CloseCause::ForcedOsDestroy {
            match self.host.can_close(&stamp, cause) {
                Ok(false) => return Ok(HostCloseStatus::KeepOpen),
                Ok(true) => {}
                Err(error) => {
                    self.last_host_error = Some(error);
                    return Ok(HostCloseStatus::RetryableFailure);
                }
            }
        }
        let callbacks = self.handle.begin_close(self.pending_callbacks())?;
        let effective_cause = if cause == CloseCause::ForcedOsDestroy {
            cause
        } else {
            *self.close_cause.get_or_insert(cause)
        };
        self.pending = None;
        let request = CloseRequest {
            stamp,
            cause: effective_cause,
            callbacks,
            checkpoint: self.close_checkpoint.take(),
        };
        let decision = if explicit {
            self.host.close(request)
        } else {
            self.host.observe_close(request)
        };
        let status = match decision {
            CloseDecision::KeepOpen => {
                self.handle.0.lifecycle.set(HostLifecycle::CleanupFailed);
                return Err(HostAccessError::Invalid(
                    "cannot reopen a revoked document; use can_close before cleanup".into(),
                ));
            }
            CloseDecision::Pending { checkpoint } => {
                self.close_retry = Some(self.host.next_close_observation(&checkpoint));
                self.close_checkpoint = Some(checkpoint);
                self.handle.0.lifecycle.set(HostLifecycle::Closing);
                HostCloseStatus::Pending
            }
            CloseDecision::RetryableFailure { checkpoint, error } => {
                self.close_checkpoint = Some(checkpoint);
                self.last_host_error = Some(error);
                self.handle.0.lifecycle.set(HostLifecycle::CleanupFailed);
                HostCloseStatus::RetryableFailure
            }
            CloseDecision::ObservationUnavailable { checkpoint, error } => {
                self.close_checkpoint = Some(checkpoint);
                self.last_host_error = Some(error);
                self.close_retry = Some(HostRetry::OnWake);
                self.handle.0.lifecycle.set(HostLifecycle::Closing);
                HostCloseStatus::ObservationUnavailable
            }
            CloseDecision::HostCleanupComplete { checkpoint } => {
                self.close_checkpoint = Some(checkpoint);
                self.handle.0.lifecycle.set(HostLifecycle::HostCleaned);
                HostCloseStatus::HostCleanupComplete
            }
            CloseDecision::ForcedIncomplete { checkpoint, error } => {
                self.close_checkpoint = checkpoint;
                self.last_host_error = error;
                self.handle.0.lifecycle.set(HostLifecycle::ForcedIncomplete);
                if !self.attached {
                    self.handle.finish_close(true)?;
                }
                HostCloseStatus::ForcedIncomplete
            }
        };
        if cause == CloseCause::ForcedOsDestroy && status != HostCloseStatus::ForcedIncomplete {
            self.handle.0.lifecycle.set(HostLifecycle::ForcedIncomplete);
            if !self.attached {
                self.handle.finish_close(true)?;
            }
            return Ok(HostCloseStatus::ForcedIncomplete);
        }
        Ok(status)
    }
    /// Headless callers have no view/surface to release. An attached driver must finish itself.
    pub fn finish_headless_close(&mut self) -> Result<(), HostAccessError> {
        if self.attached {
            return Err(HostAccessError::Invalid(
                "window driver still owns presentation resources".into(),
            ));
        }
        self.handle.finish_close(false)
    }
    #[cfg(feature = "window")]
    pub(crate) fn attach(&mut self) -> Result<(), HostAccessError> {
        if self.handle.lifecycle() != HostLifecycle::Open {
            return Err(HostAccessError::Closing);
        }
        if self.attached {
            return Err(HostAccessError::Busy);
        }
        self.attached = true;
        Ok(())
    }
    #[cfg(feature = "window")]
    pub(crate) fn window_removed(&mut self, forced: bool) -> Result<(), HostAccessError> {
        if forced && self.handle.lifecycle() != HostLifecycle::Closed {
            self.handle.begin_close(self.pending_callbacks())?;
            self.pending = None;
        }
        self.attached = false;
        self.handle.finish_close(forced)
    }
}
impl<H: NativeWindowHost> Drop for NativeHostRuntime<H> {
    fn drop(&mut self) {
        if !self.attached
            && !matches!(
                self.handle.lifecycle(),
                HostLifecycle::Closed | HostLifecycle::ForcedIncomplete
            )
        {
            // Drop is never a successful close acknowledgement or a place to run consumer SDK code.
            let _ = self.handle.begin_close(self.pending_callbacks());
            let _ = self.handle.finish_close(true);
        }
    }
}
