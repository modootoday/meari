use super::*;

#[test]
fn early_completion_is_retained_before_pending_return_and_duplicate_wake_is_harmless() {
    let (owner, _, wakes) = owner(1);
    ready(&owner);
    let state = control(Mode::Early);
    register(&owner, &state);
    assert_eq!(owner.begin_close().unwrap().status, CleanupStatus::Complete);
    assert_eq!(state.borrow().starts, 1);
    assert_eq!(state.borrow().observations, 1);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    assert!(!state.borrow().signals[0].notify());
    owner.poll().unwrap();
    assert_eq!(state.borrow().observations, 1);
}

#[test]
fn timeout_is_not_ack_and_never_restarts_inflight_cleanup() {
    let (owner, clock, _) = owner(1);
    ready(&owner);
    let state = control(Mode::Pending);
    register(&owner, &state);
    assert_eq!(owner.begin_close().unwrap().status, CleanupStatus::Pending);
    assert_eq!(owner.next_deadline().unwrap(), Some(Duration::from_secs(5)));
    clock.0.set(Duration::from_secs(5));
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::TimedOut);
    assert!(owner.window_removed().is_err());
    assert_eq!(owner.next_deadline().unwrap(), None);
    owner.retry_failed().unwrap();
    owner.begin_close().unwrap();
    assert_eq!(state.borrow().starts, 1);
    assert_eq!(state.borrow().observations, 0);
    finish(&state);
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::Complete);
    owner.window_removed().unwrap();
    assert_eq!(owner.phase().unwrap(), WindowPhase::Closed);
}

#[test]
fn only_failed_step_retries_and_old_attempt_signal_cannot_ack_new_attempt() {
    let (owner, _, _) = owner(2);
    ready(&owner);
    let complete = control(Mode::Complete);
    let fails = control(Mode::FailOnce);
    register(&owner, &complete);
    let failed_lease = register(&owner, &fails);
    assert_eq!(
        owner.begin_close().unwrap().status,
        CleanupStatus::RetryableFailure
    );
    assert!(
        owner
            .with_error(&failed_lease, |error| error.is_some())
            .unwrap()
    );
    let old = fails.borrow().signals[0].clone();
    assert_eq!(owner.retry_failed().unwrap().status, CleanupStatus::Pending);
    let new = fails.borrow().signals[1].clone();
    assert!(new.stamp().attempt() > old.stamp().attempt());
    assert!(!old.notify());
    owner.poll().unwrap();
    assert_eq!(fails.borrow().observations, 0);
    assert_eq!(complete.borrow().starts, 1);
    finish(&fails);
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::Complete);
    assert_eq!(fails.borrow().starts, 2);
}

#[test]
fn forced_loss_does_not_restart_pending_and_never_becomes_normal_closed() {
    let (owner, _, _) = owner(1);
    let initial = ready(&owner);
    let state = control(Mode::Pending);
    register(&owner, &state);
    owner.begin_close().unwrap();
    let report = owner.force_loss().unwrap();
    assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
    assert_eq!(state.borrow().losses, 1);
    assert_eq!(state.borrow().starts, 1);
    assert!(owner.stamp().unwrap().epoch() > initial.epoch());
    finish(&state);
    let report = owner.poll().unwrap();
    assert_eq!(report.remaining, 0);
    assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
    assert!(owner.window_removed().is_err());
}

#[test]
fn dropping_coordinator_revokes_worker_signal_without_invoking_cleanup() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let state = control(Mode::Pending);
    register(&owner, &state);
    owner.begin_close().unwrap();
    let signal = state.borrow().signals[0].clone();
    drop(owner);
    assert!(!signal.notify());
    assert_eq!(state.borrow().starts, 1);
    assert_eq!(state.borrow().observations, 0);
}

#[derive(Default)]
struct ObservationControl {
    unavailable: bool,
    fail_once: bool,
    done: bool,
    starts: usize,
    observations: usize,
    losses: usize,
    drops: usize,
    signals: Vec<CompletionSignal>,
}
struct ObservedChild {
    control: Rc<RefCell<ObservationControl>>,
    error: io::Error,
}
impl ObservedChild {
    fn new(control: Rc<RefCell<ObservationControl>>) -> Self {
        Self {
            control,
            error: io::Error::other("original observation diagnostic"),
        }
    }
    fn progress(&self) -> CleanupProgress {
        let state = self.control.borrow();
        if state.done {
            CleanupProgress::Complete
        } else if state.unavailable {
            CleanupProgress::ObservationUnavailable
        } else {
            CleanupProgress::Pending
        }
    }
}
impl Drop for ObservedChild {
    fn drop(&mut self) {
        self.control.borrow_mut().drops += 1;
    }
}
impl NativeChild for ObservedChild {
    type Error = io::Error;
    fn parent_lost(&mut self) {
        self.control.borrow_mut().losses += 1;
    }
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, io::Error> {
        {
            let mut state = self.control.borrow_mut();
            state.starts += 1;
            state.signals.push(signal);
            if state.fail_once {
                state.fail_once = false;
                return Err(io::Error::other("terminal failure without in-flight work"));
            }
        }
        Ok(self.progress())
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
        self.control.borrow_mut().observations += 1;
        Ok(self.progress())
    }
    fn observation_error(&self) -> Option<&io::Error> {
        Some(&self.error)
    }
}
type ObservedOwner = (
    WindowLifecycle<ObservedChild>,
    Clock,
    Rc<RefCell<ObservationControl>>,
    ChildLease,
);
fn observed_owner() -> ObservedOwner {
    let clock = Clock::default();
    let owner = WindowLifecycle::new(
        LifecycleLimits::new(2, Duration::from_secs(5)).unwrap(),
        clock.clone(),
        Waker::noop().clone(),
    )
    .unwrap();
    let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
    let control = Rc::new(RefCell::new(ObservationControl {
        unavailable: true,
        ..Default::default()
    }));
    let lease = owner
        .register(stamp, |_| Ok(ObservedChild::new(control.clone())))
        .unwrap();
    (owner, clock, control, lease)
}

#[test]
fn unavailable_preserves_owner_attempt_signal_deadline_and_late_completion() {
    let (owner, clock, control, lease) = observed_owner();
    let before = owner.barrier_stamp().unwrap();
    let report = owner.begin_close().unwrap();
    assert_eq!(report.status, CleanupStatus::ObservationUnavailable);
    assert_eq!(
        (report.remaining, report.unavailable, report.failed),
        (1, 1, 0)
    );
    assert!(
        owner
            .with_error(&lease, |error| error.unwrap().to_string()
                == "original observation diagnostic")
            .unwrap()
    );
    assert!(matches!(
        owner.reobserve_unavailable(before),
        Err(LifecycleError::Stale)
    ));
    let signal = control.borrow().signals[0].clone();
    let stamp = signal.stamp();
    assert_eq!(owner.next_deadline().unwrap(), Some(Duration::from_secs(5)));
    clock.0.set(Duration::from_secs(5));
    let report = owner.poll().unwrap();
    assert_eq!(report.status, CleanupStatus::ObservationUnavailable);
    assert_eq!(
        (report.remaining, report.unavailable, report.timed_out),
        (1, 1, 1)
    );
    assert_eq!(owner.next_deadline().unwrap(), None);
    owner.retry_failed().unwrap();
    owner.begin_close().unwrap();
    for _ in 0..8 {
        owner.poll().unwrap();
    }
    assert_eq!(
        (
            control.borrow().starts,
            control.borrow().observations,
            control.borrow().drops
        ),
        (1, 0, 0)
    );
    assert_eq!(control.borrow().signals[0].stamp(), stamp);
    assert!(owner.window_removed().is_err());
    control.borrow_mut().done = true;
    assert!(signal.notify());
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::Complete);
    assert_eq!(
        (
            control.borrow().starts,
            control.borrow().observations,
            control.borrow().drops
        ),
        (1, 1, 1)
    );
    assert!(!signal.notify());
    owner.window_removed().unwrap();
}

#[test]
fn explicit_reobservation_uses_same_barrier_and_never_restarts_work() {
    let (owner, _, control, lease) = observed_owner();
    owner.begin_close().unwrap();
    let barrier = owner.barrier_stamp().unwrap();
    let original = control.borrow().signals[0].stamp();
    let (foreign, _, _, _) = observed_owner();
    assert!(matches!(
        owner.reobserve_unavailable(foreign.barrier_stamp().unwrap()),
        Err(LifecycleError::Stale)
    ));
    for expected in 1..=3 {
        assert_eq!(
            owner.reobserve_unavailable(barrier).unwrap().status,
            CleanupStatus::ObservationUnavailable
        );
        assert_eq!(control.borrow().observations, expected);
        assert_eq!(control.borrow().starts, 1);
        assert_eq!(control.borrow().signals[0].stamp(), original);
    }
    control.borrow_mut().unavailable = false;
    assert_eq!(
        owner.reobserve_unavailable(barrier).unwrap().status,
        CleanupStatus::Pending
    );
    assert!(owner.with_error(&lease, |error| error.is_none()).unwrap());
    control.borrow_mut().unavailable = true;
    control.borrow().signals[0].notify();
    owner.poll().unwrap();
    let report = owner.force_loss().unwrap();
    assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
    assert_eq!(control.borrow().starts, 1);
    assert_eq!(control.borrow().losses, 1);
    assert!(matches!(
        owner.reobserve_unavailable(barrier),
        Err(LifecycleError::Stale)
    ));
    control.borrow_mut().done = true;
    let report = owner
        .reobserve_unavailable(owner.barrier_stamp().unwrap())
        .unwrap();
    assert_eq!(
        (report.status, report.remaining),
        (CleanupStatus::ForcedIncomplete, 0)
    );
    assert!(owner.window_removed().is_err());
}

#[test]
fn failed_retry_does_not_restart_the_unavailable_sibling() {
    let (owner, _, unknown, _) = observed_owner();
    let failing = Rc::new(RefCell::new(ObservationControl {
        fail_once: true,
        ..Default::default()
    }));
    owner
        .register(owner.stamp().unwrap(), |_| {
            Ok(ObservedChild::new(failing.clone()))
        })
        .unwrap();
    let report = owner.begin_close().unwrap();
    assert_eq!(
        (report.unavailable, report.failed, report.remaining),
        (1, 1, 2)
    );
    let old_failed_signal = failing.borrow().signals[0].clone();
    owner.retry_failed().unwrap();
    assert_eq!((unknown.borrow().starts, failing.borrow().starts), (1, 2));
    assert!(!old_failed_signal.notify());
    failing.borrow_mut().done = true;
    failing.borrow().signals[1].notify();
    assert_eq!(owner.poll().unwrap().remaining, 1);
    unknown.borrow_mut().done = true;
    unknown.borrow().signals[0].notify();
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::Complete);
}
