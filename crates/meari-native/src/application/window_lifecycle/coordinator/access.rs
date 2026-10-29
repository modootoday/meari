//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{
    authority::{Authority, ChildLease},
    completion::CompletionSignal,
    state::{Entry, State},
};
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    task::Waker,
    time::Duration,
};

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl<C: NativeChild> WindowLifecycle<C> {
    pub fn new(
        limits: LifecycleLimits,
        clock: impl MonotonicClock,
        wake: Waker,
    ) -> Result<Self, LifecycleError> {
        Ok(Self {
            authority: Authority::new()?,
            limits,
            clock: Box::new(clock),
            wake,
            busy: Cell::new(false),
            state: RefCell::new(State {
                phase: WindowPhase::NotReady,
                epoch: 0,
                surface: 0,
                sequence: 0,
                revocation: 0,
                last_now: Duration::ZERO,
                loss_notification_pending: false,
                entries: BTreeMap::new(),
            }),
        })
    }

    pub(super) fn enter(&self) -> Result<Operation<'_>, LifecycleError> {
        if self.busy.replace(true) {
            return Err(LifecycleError::Busy);
        }
        Ok(Operation(&self.busy))
    }

    pub(super) fn time(&self) -> Result<Duration, LifecycleError> {
        let now = self.clock.now();
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if now < state.last_now {
            return Err(LifecycleError::ClockWentBackwards);
        }
        state.last_now = now;
        Ok(now)
    }

    pub fn phase(&self) -> Result<WindowPhase, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .phase)
    }

    pub fn owner_id(&self) -> u64 {
        self.authority.id
    }

    pub fn barrier_stamp(&self) -> Result<BarrierStamp, LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        Ok(BarrierStamp {
            window: state.stamp(self.authority.id),
            revocation: state.revocation,
            phase: state.phase,
        })
    }

    pub fn stamp(&self) -> Result<WindowStamp, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .stamp(self.authority.id))
    }

    pub fn report(&self) -> Result<CleanupReport, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .report())
    }

    pub fn ready(&self, kind: ReadyKind) -> Result<WindowStamp, LifecycleError> {
        let _operation = self.enter()?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if !state.entries.is_empty() {
            return Err(LifecycleError::Busy);
        }
        match (state.phase, kind) {
            (WindowPhase::NotReady | WindowPhase::Suspended, ReadyKind::NewWindow) => {
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
            }
            (WindowPhase::Suspended, ReadyKind::SameWindow) => (),
            (WindowPhase::Closed | WindowPhase::ForcedIncomplete, _) => {
                return Err(LifecycleError::Closed);
            }
            _ => return Err(LifecycleError::WrongState),
        }
        state.phase = WindowPhase::Ready;
        Ok(state.stamp(self.authority.id))
    }

    pub(super) fn check_ready(
        state: &State<C>,
        expected: WindowStamp,
        owner: u64,
    ) -> Result<(), LifecycleError> {
        match state.phase {
            WindowPhase::NotReady => return Err(LifecycleError::NotReady),
            WindowPhase::Suspending | WindowPhase::Suspended => {
                return Err(LifecycleError::Suspended);
            }
            WindowPhase::Closing => return Err(LifecycleError::Closing),
            WindowPhase::Closed | WindowPhase::ForcedIncomplete => {
                return Err(LifecycleError::Closed);
            }
            WindowPhase::Ready => (),
        }
        if state.stamp(owner) != expected {
            return Err(LifecycleError::Stale);
        }
        Ok(())
    }

    pub(super) fn check_lease(
        &self,
        state: &State<C>,
        lease: &ChildLease,
    ) -> Result<(), LifecycleError> {
        if !Rc::ptr_eq(&self.authority, &lease.authority)
            || !state
                .entries
                .get(&lease.child)
                .is_some_and(|entry| entry.window == lease.window)
        {
            return Err(LifecycleError::Stale);
        }
        Ok(())
    }

    pub fn validate_access(&self, lease: &ChildLease) -> Result<(), LifecycleError> {
        if self.busy.get() {
            return Err(LifecycleError::Busy);
        }
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        self.check_lease(&state, lease)?;
        Self::check_ready(&state, lease.window, self.authority.id)?;
        if state.entries[&lease.child].phase != ChildPhase::Active {
            return Err(LifecycleError::Closing);
        }
        Ok(())
    }

    #[cfg(any(feature = "window", test))]
    pub(crate) fn validate_parent_creation(
        &self,
        lease: &ChildLease,
    ) -> Result<(), LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        self.check_lease(&state, lease)?;
        Self::check_ready(&state, lease.window, self.authority.id)?;
        let entry = &state.entries[&lease.child];
        if !self.busy.get() || entry.phase != ChildPhase::Creating || entry.adapter.is_some() {
            return Err(LifecycleError::WrongState);
        }
        Ok(())
    }

    #[cfg(any(feature = "window", test))]
    pub(crate) fn validate_parent_cleanup(
        &self,
        lease: &ChildLease,
        request: CleanupRequest,
    ) -> Result<(), LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        self.check_lease(&state, lease)?;
        if matches!(
            state.phase,
            WindowPhase::Closed | WindowPhase::ForcedIncomplete
        ) {
            return Err(LifecycleError::Closed);
        }
        let entry = &state.entries[&lease.child];
        if !self.busy.get()
            || entry.adapter.is_some()
            || !matches!(entry.phase, ChildPhase::Waiting | ChildPhase::TimedOut)
            || entry.reason != request.reason
            || !entry
                .signal
                .as_ref()
                .is_some_and(|signal| signal.stamp() == request.stamp)
        {
            return Err(LifecycleError::Stale);
        }
        Ok(())
    }

    pub fn register(
        &self,
        expected: WindowStamp,
        create: impl FnOnce(&ChildLease) -> Result<C, ChildCreationFailure<C>>,
    ) -> Result<ChildLease, RegistrationFailure<C::Error>> {
        let fail = |error| RegistrationFailure {
            problem: RegistrationProblem::Lifecycle(error),
            cleanup: None,
        };
        let _operation = self.enter().map_err(fail)?;
        let lease = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| fail(LifecycleError::Busy))?;
            Self::check_ready(&state, expected, self.authority.id).map_err(fail)?;
            if state.entries.len() >= self.limits.max_children() {
                return Err(fail(LifecycleError::Capacity));
            }
            let child = state
                .sequence
                .checked_add(1)
                .ok_or_else(|| fail(LifecycleError::CounterExhausted))?;
            state.sequence = child;
            state.entries.insert(
                child,
                Entry {
                    window: expected,
                    adapter: None,
                    phase: ChildPhase::Creating,
                    revocation: 0,
                    attempt: 0,
                    reason: RevocationReason::ExplicitRelease,
                    deadline: None,
                    signal: None,
                    observation_unavailable: false,
                    error: None,
                },
            );
            ChildLease {
                authority: self.authority.clone(),
                window: expected,
                child,
            }
        };
        // Native creation happens only after both active and cleanup slots are reserved.
        match catch_unwind(AssertUnwindSafe(|| create(&lease))) {
            Ok(Ok(child)) => {
                let mut state = self.state.borrow_mut();
                let stale =
                    state.phase != WindowPhase::Ready || state.stamp(self.authority.id) != expected;
                let entry = state.entries.get_mut(&lease.child).expect("reserved child");
                entry.adapter = Some(child);
                entry.phase = if stale {
                    ChildPhase::NeedsCleanup
                } else {
                    ChildPhase::Active
                };
                if stale {
                    Err(RegistrationFailure {
                        problem: RegistrationProblem::Lifecycle(LifecycleError::Stale),
                        cleanup: Some(lease),
                    })
                } else {
                    Ok(lease)
                }
            }
            Ok(Err(failure)) => {
                let mut state = self.state.borrow_mut();
                let cleanup = if let Some(partial) = failure.partial {
                    let entry = state.entries.get_mut(&lease.child).expect("reserved child");
                    entry.adapter = Some(partial);
                    entry.phase = ChildPhase::NeedsCleanup;
                    Some(lease)
                } else {
                    state.entries.remove(&lease.child);
                    None
                };
                Err(RegistrationFailure {
                    problem: RegistrationProblem::Creation(failure.error),
                    cleanup,
                })
            }
            Err(payload) => {
                let mut state = self.state.borrow_mut();
                state.phase = WindowPhase::ForcedIncomplete;
                state.loss_notification_pending = true;
                drop(state);
                resume_unwind(payload)
            }
        }
    }

    pub(super) fn prepare(
        &self,
        ids: &[u64],
        reason: RevocationReason,
        now: Duration,
    ) -> Result<(), LifecycleError> {
        if ids.is_empty() {
            return Ok(());
        }
        let deadline = now
            .checked_add(self.limits.cleanup_timeout())
            .ok_or(LifecycleError::DeadlineOverflow)?;
        let mut state = self.state.borrow_mut();
        let revocation = state
            .revocation
            .checked_add(1)
            .ok_or(LifecycleError::CounterExhausted)?;
        // Preflight every counter before modifying any lease.
        for id in ids {
            state.entries[id]
                .attempt
                .checked_add(1)
                .ok_or(LifecycleError::CounterExhausted)?;
        }
        state.revocation = revocation;
        for id in ids {
            let entry = state
                .entries
                .get_mut(id)
                .expect("cleanup child preflighted");
            entry.deactivate();
            entry.revocation = revocation;
            entry.attempt += 1;
            entry.reason = reason;
            entry.deadline = Some(deadline);
            entry.error = None;
            entry.observation_unavailable = false;
            entry.phase = ChildPhase::Waiting;
            entry.signal = Some(CompletionSignal::new(
                CleanupStamp {
                    window: entry.window,
                    child: *id,
                    revocation,
                    attempt: entry.attempt,
                },
                self.wake.clone(),
            ));
        }
        Ok(())
    }

    pub(super) fn invoke(&self, id: u64, start: bool) {
        let (mut adapter, request, signal) = {
            let mut state = self.state.borrow_mut();
            let entry = state.entries.get_mut(&id).expect("cleanup child prepared");
            let Some(adapter) = entry.adapter.take() else {
                state.phase = WindowPhase::ForcedIncomplete;
                return;
            };
            let signal = entry
                .signal
                .as_ref()
                .expect("cleanup signal reserved")
                .clone();
            (
                adapter,
                CleanupRequest {
                    stamp: signal.stamp(),
                    reason: entry.reason,
                },
                signal,
            )
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            if start {
                adapter.begin_cleanup(request, signal)
            } else {
                adapter.observe_cleanup(request)
            }
        }));
        let mut state = self.state.borrow_mut();
        let entry = state
            .entries
            .get_mut(&id)
            .expect("cleanup child retained across callback");
        entry.adapter = Some(adapter);
        match outcome {
            Ok(Ok(CleanupProgress::Complete)) => {
                let entry = state.entries.remove(&id).expect("completed child");
                entry.deactivate();
                state.finish_suspend();
                drop(state);
                drop(entry);
            }
            Ok(Ok(CleanupProgress::Pending)) => entry.observation_unavailable = false,
            Ok(Ok(CleanupProgress::ObservationUnavailable)) => {
                entry.observation_unavailable = true;
            }
            Ok(Err(error)) => {
                entry.observation_unavailable = false;
                entry.phase = ChildPhase::Failed;
                entry.error = Some(error);
                entry.deadline = None;
                entry.deactivate();
            }
            Err(payload) => {
                entry.deactivate();
                state.phase = WindowPhase::ForcedIncomplete;
                state.loss_notification_pending = true;
                drop(state);
                resume_unwind(payload);
            }
        }
    }
}
