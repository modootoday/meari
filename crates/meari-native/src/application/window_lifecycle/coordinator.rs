use super::{
    authority::{Authority, ChildLease},
    completion::CompletionSignal,
    ports::*,
    state::{Entry, State},
};
use crate::domain::window_lifecycle::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    task::Waker,
    time::Duration,
};

#[derive(Debug)]
pub enum RegistrationProblem<E> {
    Lifecycle(LifecycleError),
    Creation(E),
}
#[derive(Debug)]
pub struct RegistrationFailure<E> {
    pub problem: RegistrationProblem<E>,
    /// A partial child retains its reservation until explicit cleanup or parent teardown.
    pub cleanup: Option<ChildLease>,
}

struct Operation<'a>(&'a Cell<bool>);
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Single UI-thread owner. Backend callbacks run with every internal RefCell guard released.
pub struct WindowLifecycle<C: NativeChild> {
    authority: Rc<Authority>,
    limits: LifecycleLimits,
    clock: Box<dyn MonotonicClock>,
    wake: Waker,
    busy: Cell<bool>,
    state: RefCell<State<C>>,
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
    fn enter(&self) -> Result<Operation<'_>, LifecycleError> {
        if self.busy.replace(true) {
            return Err(LifecycleError::Busy);
        }
        Ok(Operation(&self.busy))
    }
    fn time(&self) -> Result<Duration, LifecycleError> {
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
    fn check_ready(
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
    fn check_lease(&self, state: &State<C>, lease: &ChildLease) -> Result<(), LifecycleError> {
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
    fn prepare(
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
    fn invoke(&self, id: u64, start: bool) {
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
    fn start_barrier(&self, reason: RevocationReason) -> Result<CleanupReport, LifecycleError> {
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
    fn poll_inner(&self, now: Duration) -> Result<CleanupReport, LifecycleError> {
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

#[cfg(test)]
mod parent_authority_tests {
    use super::*;
    use std::{cell::Cell, io};
    struct Clock;
    impl MonotonicClock for Clock {
        fn now(&self) -> Duration {
            Duration::ZERO
        }
    }
    struct Probe {
        starts: Rc<Cell<usize>>,
        losses: Rc<Cell<usize>>,
        hook: Option<Box<dyn FnMut(CleanupRequest)>>,
    }
    impl NativeChild for Probe {
        type Error = io::Error;
        fn parent_lost(&mut self) {
            self.losses.set(self.losses.get() + 1);
        }
        fn begin_cleanup(
            &mut self,
            request: CleanupRequest,
            _: CompletionSignal,
        ) -> Result<CleanupProgress, io::Error> {
            self.starts.set(self.starts.get() + 1);
            if let Some(hook) = &mut self.hook {
                hook(request);
            }
            Ok(CleanupProgress::Complete)
        }
        fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
            Ok(CleanupProgress::Complete)
        }
    }
    fn owner() -> Rc<WindowLifecycle<Probe>> {
        Rc::new(
            WindowLifecycle::new(
                LifecycleLimits::new(2, Duration::from_secs(1)).unwrap(),
                Clock,
                Waker::noop().clone(),
            )
            .unwrap(),
        )
    }
    fn probe() -> Probe {
        Probe {
            starts: Rc::new(Cell::new(0)),
            losses: Rc::new(Cell::new(0)),
            hook: None,
        }
    }

    #[test]
    fn creation_authority_is_not_an_active_cleanup_or_foreign_permission() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let lease = owner
            .register(stamp, |lease| {
                owner.validate_parent_creation(lease).unwrap();
                assert_eq!(owner.validate_access(lease), Err(LifecycleError::Busy));
                Ok(probe())
            })
            .unwrap();
        let parent = lease.for_parent();
        owner.validate_access(&parent).unwrap();
        assert_eq!(
            owner.validate_parent_creation(&parent),
            Err(LifecycleError::WrongState)
        );
        let other = self::owner();
        other.ready(ReadyKind::NewWindow).unwrap();
        assert_eq!(
            other.validate_parent_creation(&parent),
            Err(LifecycleError::Stale)
        );
    }

    #[test]
    fn cleanup_parent_requires_the_lent_adapter_and_current_attempt() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let observed = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(&owner);
        let out = observed.clone();
        owner
            .register(stamp, |lease| {
                let parent = lease.for_parent();
                let mut child = probe();
                child.hook = Some(Box::new(move |request| {
                    let owner = weak.upgrade().unwrap();
                    owner.validate_parent_cleanup(&parent, request).unwrap();
                    assert_eq!(owner.validate_access(&parent), Err(LifecycleError::Busy));
                    let mut stale = request;
                    stale.stamp.attempt += 1;
                    assert_eq!(
                        owner.validate_parent_cleanup(&parent, stale),
                        Err(LifecycleError::Stale)
                    );
                    out.set(true);
                }));
                Ok(child)
            })
            .unwrap();
        assert_eq!(owner.begin_close().unwrap().status, CleanupStatus::Complete);
        assert!(observed.get());
    }

    #[test]
    fn abandon_revokes_without_callbacks_until_explicit_residual_cleanup() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let child = probe();
        let starts = child.starts.clone();
        let losses = child.losses.clone();
        let lease = owner.register(stamp, |_| Ok(child)).unwrap();
        owner.abandon_window().unwrap();
        assert_eq!(starts.get(), 0);
        assert_eq!(losses.get(), 0);
        assert_eq!(owner.validate_access(&lease), Err(LifecycleError::Closed));
        assert_eq!(owner.report().unwrap().remaining, 1);
        let report = owner.force_loss().unwrap();
        assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
        assert_eq!(report.remaining, 0);
        assert_eq!(starts.get(), 1);
        assert_eq!(losses.get(), 1);
        owner.force_loss().unwrap();
        assert_eq!(starts.get(), 1);
        assert!(owner.window_removed().is_err());
    }

    #[test]
    fn abandonment_during_factory_returns_partial_owner_instead_of_active_lease() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let child = probe();
        let starts = child.starts.clone();
        let failure = owner
            .register(stamp, |_| {
                owner.abandon_window().unwrap();
                Ok(child)
            })
            .unwrap_err();
        assert!(matches!(
            failure.problem,
            RegistrationProblem::Lifecycle(LifecycleError::Stale)
        ));
        assert!(failure.cleanup.is_some());
        assert_eq!(starts.get(), 0);
        assert_eq!(owner.report().unwrap().remaining, 1);
        assert_eq!(owner.force_loss().unwrap().remaining, 0);
    }
}
