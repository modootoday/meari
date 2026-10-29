//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{authority::ChildLease, completion::CompletionSignal};
use super::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    time::Duration,
};

impl<C: NativeChild> WindowLifecycle<C> {
    pub(super) fn start_barrier(
        &self,
        reason: RevocationReason,
    ) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        let now = self.time()?;
        let ids = {
            let state = self.state.borrow();
            match (reason, state.phase) {
                (_, WindowPhase::Closed | WindowPhase::ForcedIncomplete) => {
                    return Err(LifecycleError::Closed);
                }
                (RevocationReason::Suspend, WindowPhase::Ready) => (),
                (RevocationReason::Suspend, _) => return Err(LifecycleError::WrongState),
                (RevocationReason::Close, _) => (),
                _ => return Err(LifecycleError::WrongState),
            }
            state
                .entries
                .iter()
                .filter(|(_, entry)| {
                    matches!(entry.phase, ChildPhase::Active | ChildPhase::NeedsCleanup)
                })
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
        };
        let next_surface = {
            let state = self.state.borrow();
            if matches!(
                state.phase,
                WindowPhase::Closing | WindowPhase::Suspending | WindowPhase::Suspended
            ) {
                state.surface
            } else {
                state
                    .surface
                    .checked_add(1)
                    .ok_or(LifecycleError::CounterExhausted)?
            }
        };
        // Preflight both the surface counter and every reserved attempt before revocation.
        self.prepare(&ids, reason, now)?;
        {
            let mut state = self.state.borrow_mut();
            state.surface = next_surface;
            state.phase = if reason == RevocationReason::Suspend {
                WindowPhase::Suspending
            } else {
                WindowPhase::Closing
            };
            state.finish_suspend();
        }
        for id in ids {
            self.invoke(id, true);
        }
        self.poll_inner(now)
    }

    pub fn suspend(&self) -> Result<CleanupReport, LifecycleError> {
        self.start_barrier(RevocationReason::Suspend)
    }

    pub fn begin_close(&self) -> Result<CleanupReport, LifecycleError> {
        self.start_barrier(RevocationReason::Close)
    }

    pub fn release_child(&self, lease: &ChildLease) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        let now = self.time()?;
        {
            let state = self.state.borrow();
            self.check_lease(&state, lease)?;
            if !matches!(
                state.entries[&lease.child].phase,
                ChildPhase::Active | ChildPhase::NeedsCleanup
            ) {
                return Ok(state.report());
            }
        }
        self.prepare(&[lease.child], RevocationReason::ExplicitRelease, now)?;
        self.invoke(lease.child, true);
        self.poll_inner(now)
    }

    pub(super) fn poll_inner(&self, now: Duration) -> Result<CleanupReport, LifecycleError> {
        let ids = {
            let mut state = self.state.borrow_mut();
            let forced = state.phase == WindowPhase::ForcedIncomplete;
            let mut ids = Vec::new();
            for (id, entry) in &mut state.entries {
                if matches!(entry.phase, ChildPhase::Waiting | ChildPhase::TimedOut) {
                    if entry.deadline.is_some_and(|deadline| now >= deadline) {
                        entry.phase = ChildPhase::TimedOut;
                    }
                    if forced || entry.signal.as_ref().is_some_and(CompletionSignal::take) {
                        ids.push(*id);
                    }
                }
            }
            ids
        };
        for id in ids {
            self.invoke(id, false);
        }
        self.report()
    }

    pub fn poll(&self) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        self.poll_inner(self.time()?)
    }

    /// Explicit observation of the same barrier/attempt; never starts or retries work.
    pub fn reobserve_unavailable(
        &self,
        expected: BarrierStamp,
    ) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        if self.barrier_stamp()? != expected {
            return Err(LifecycleError::Stale);
        }
        let now = self.time()?;
        let ids = {
            let mut state = self.state.borrow_mut();
            if state.phase == WindowPhase::Closed {
                return Err(LifecycleError::Closed);
            }
            state
                .entries
                .iter_mut()
                .filter_map(|(id, entry)| {
                    if !entry.observation_unavailable
                        || !matches!(entry.phase, ChildPhase::Waiting | ChildPhase::TimedOut)
                    {
                        return None;
                    }
                    if entry.deadline.is_some_and(|deadline| now >= deadline) {
                        entry.phase = ChildPhase::TimedOut;
                    }
                    if let Some(signal) = &entry.signal {
                        signal.take();
                    }
                    Some(*id)
                })
                .collect::<Vec<_>>()
        };
        for id in ids {
            self.invoke(id, false);
        }
        self.report()
    }

    pub fn retry_failed(&self) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        let now = self.time()?;
        let failed = self
            .state
            .borrow()
            .entries
            .iter()
            .filter(|(_, entry)| entry.phase == ChildPhase::Failed)
            .map(|(id, entry)| (*id, entry.reason))
            .collect::<Vec<_>>();
        {
            let state = self.state.borrow();
            state
                .revocation
                .checked_add(
                    failed
                        .len()
                        .try_into()
                        .map_err(|_| LifecycleError::CounterExhausted)?,
                )
                .ok_or(LifecycleError::CounterExhausted)?;
            now.checked_add(self.limits.cleanup_timeout())
                .ok_or(LifecycleError::DeadlineOverflow)?;
            for (id, _) in &failed {
                state.entries[id]
                    .attempt
                    .checked_add(1)
                    .ok_or(LifecycleError::CounterExhausted)?;
            }
        }
        for (id, reason) in failed {
            self.prepare(&[id], reason, now)?;
            self.invoke(id, true);
        }
        self.poll_inner(now)
    }

    /// Parent loss never restarts cleanup already in flight or certifies normal closure.
    pub fn force_loss(&self) -> Result<CleanupReport, LifecycleError> {
        let _operation = self.enter()?;
        let now = self.time();
        let ids = {
            let mut state = self.state.borrow_mut();
            if state.phase == WindowPhase::Closed {
                return Ok(state.report());
            }
            if state.phase == WindowPhase::ForcedIncomplete && !state.loss_notification_pending {
                return Ok(state.report());
            }
            if state.phase != WindowPhase::ForcedIncomplete {
                state.phase = WindowPhase::ForcedIncomplete;
                state.epoch = state
                    .epoch
                    .checked_add(1)
                    .ok_or(LifecycleError::CounterExhausted)?;
                state.surface = state
                    .surface
                    .checked_add(1)
                    .ok_or(LifecycleError::CounterExhausted)?;
            }
            state.loss_notification_pending = false;
            state.entries.keys().copied().collect::<Vec<_>>()
        };
        for id in &ids {
            let adapter = self
                .state
                .borrow_mut()
                .entries
                .get_mut(id)
                .expect("lost child retained")
                .adapter
                .take();
            if let Some(mut adapter) = adapter {
                let result = catch_unwind(AssertUnwindSafe(|| adapter.parent_lost()));
                self.state
                    .borrow_mut()
                    .entries
                    .get_mut(id)
                    .expect("lost child retained")
                    .adapter = Some(adapter);
                if let Err(payload) = result {
                    resume_unwind(payload);
                }
            }
        }
        let now = now?;
        let starts = self
            .state
            .borrow()
            .entries
            .iter()
            .filter(|(_, entry)| {
                matches!(
                    entry.phase,
                    ChildPhase::Active | ChildPhase::NeedsCleanup | ChildPhase::Failed
                )
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        self.prepare(&starts, RevocationReason::ForcedLoss, now)?;
        for id in starts {
            self.invoke(id, true);
        }
        self.poll_inner(now)
    }

    /// Driver Drop revokes access only. Explicit recovery may later notify/drain retained adapters.
    #[cfg(any(feature = "window", test))]
    pub(crate) fn abandon_window(&self) -> Result<(), LifecycleError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if matches!(
            state.phase,
            WindowPhase::Closed | WindowPhase::ForcedIncomplete
        ) {
            return Ok(());
        }
        state.phase = WindowPhase::ForcedIncomplete;
        state.loss_notification_pending = true;
        for entry in state.entries.values() {
            entry.deactivate();
        }
        let epoch = state
            .epoch
            .checked_add(1)
            .ok_or(LifecycleError::CounterExhausted)?;
        let surface = state
            .surface
            .checked_add(1)
            .ok_or(LifecycleError::CounterExhausted)?;
        state.epoch = epoch;
        state.surface = surface;
        Ok(())
    }

    pub fn next_deadline(&self) -> Result<Option<Duration>, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .entries
            .values()
            .filter(|entry| entry.phase == ChildPhase::Waiting)
            .filter_map(|entry| entry.deadline)
            .min())
    }

    pub fn with_error<R>(
        &self,
        lease: &ChildLease,
        read: impl for<'e> FnOnce(Option<&'e C::Error>) -> R,
    ) -> Result<R, LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        self.check_lease(&state, lease)?;
        let entry = &state.entries[&lease.child];
        let error = entry.error.as_ref().or_else(|| {
            if entry.observation_unavailable {
                entry
                    .adapter
                    .as_ref()
                    .and_then(NativeChild::observation_error)
            } else {
                None
            }
        });
        Ok(read(error))
    }

    pub fn window_removed(&self) -> Result<(), LifecycleError> {
        let _operation = self.enter()?;
        let mut state = self.state.borrow_mut();
        if state.phase != WindowPhase::Closing || !state.entries.is_empty() {
            return Err(LifecycleError::WrongState);
        }
        state.phase = WindowPhase::Closed;
        Ok(())
    }
}

impl<C: NativeChild> Drop for WindowLifecycle<C> {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        for entry in state.entries.values() {
            entry.deactivate();
        }
        if state.phase != WindowPhase::Closed {
            state.phase = WindowPhase::ForcedIncomplete;
        }
        // Never invoke backend/business cleanup from Drop. Adapter fields keep their own RAII contract.
    }
}
