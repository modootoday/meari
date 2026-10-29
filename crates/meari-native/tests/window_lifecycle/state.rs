use super::*;

#[test]
fn reservation_precedes_creation_and_rejects_not_ready_foreign_owner_and_capacity() {
    let (owner, _, _) = owner(1);
    let called = Cell::new(false);
    let error = owner
        .register(owner.stamp().unwrap(), |_| {
            called.set(true);
            Ok(Child(control(Mode::Complete)))
        })
        .unwrap_err();
    assert!(matches!(
        error.problem,
        RegistrationProblem::Lifecycle(LifecycleError::NotReady)
    ));
    assert!(!called.get());
    let stamp = ready(&owner);
    let first = register(&owner, &control(Mode::Complete));
    let error = owner
        .register(stamp, |_| {
            called.set(true);
            Ok(Child(control(Mode::Complete)))
        })
        .unwrap_err();
    assert!(matches!(
        error.problem,
        RegistrationProblem::Lifecycle(LifecycleError::Capacity)
    ));
    assert!(!called.get());
    let (other, _, _) = super::owner(1);
    ready(&other);
    assert_eq!(other.validate_access(&first), Err(LifecycleError::Stale));
    assert!(matches!(
        other
            .register(stamp, |_| Ok(Child(control(Mode::Complete))))
            .unwrap_err()
            .problem,
        RegistrationProblem::Lifecycle(LifecycleError::Stale)
    ));
    owner.validate_access(&first).unwrap();
}

#[test]
fn failed_creation_retains_partial_native_owner_and_cleanup_reservation() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let state = control(Mode::Pending);
    let failure = owner
        .register(owner.stamp().unwrap(), |_| {
            Err(ChildCreationFailure {
                error: io::Error::other("partial native creation"),
                partial: Some(Child(state.clone())),
            })
        })
        .unwrap_err();
    let lease = failure.cleanup.unwrap();
    assert!(matches!(failure.problem, RegistrationProblem::Creation(_)));
    assert_eq!(owner.validate_access(&lease), Err(LifecycleError::Closing));
    assert!(matches!(
        owner
            .register(owner.stamp().unwrap(), |_| Ok(Child(control(
                Mode::Complete
            ))))
            .unwrap_err()
            .problem,
        RegistrationProblem::Lifecycle(LifecycleError::Capacity)
    ));
    assert_eq!(
        owner.release_child(&lease).unwrap().status,
        CleanupStatus::Pending
    );
    finish(&state);
    assert_eq!(owner.poll().unwrap().status, CleanupStatus::Complete);
    assert_eq!(owner.validate_access(&lease), Err(LifecycleError::Stale));
    let replacement = register(&owner, &control(Mode::Complete));
    assert!(replacement.id() > lease.id());
}

#[test]
fn suspend_releases_children_and_new_ready_never_revives_old_lease() {
    let (owner, _, _) = owner(2);
    let first = ready(&owner);
    let child = register(&owner, &control(Mode::Complete));
    assert_eq!(owner.suspend().unwrap().status, CleanupStatus::Complete);
    assert_eq!(owner.phase().unwrap(), WindowPhase::Suspended);
    assert_eq!(owner.validate_access(&child), Err(LifecycleError::Stale));
    let resumed = owner.ready(ReadyKind::SameWindow).unwrap();
    assert_eq!(resumed.owner(), first.owner());
    assert_eq!(resumed.epoch(), first.epoch());
    assert_ne!(resumed.surface_generation(), first.surface_generation());
    register(&owner, &control(Mode::Complete));
    owner.suspend().unwrap();
    let replaced = owner.ready(ReadyKind::NewWindow).unwrap();
    assert!(replaced.epoch() > resumed.epoch());
}

#[test]
fn callbacks_have_no_internal_borrow_and_reentrant_operations_return_busy() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let state = control(Mode::Complete);
    let observed = Rc::new(Cell::new(false));
    let weak = Rc::downgrade(&owner);
    let out = observed.clone();
    state.borrow_mut().hook = Some(Box::new(move || {
        let owner = weak.upgrade().unwrap();
        assert!(owner.report().is_ok());
        assert_eq!(owner.begin_close(), Err(LifecycleError::Busy));
        out.set(true);
    }));
    register(&owner, &state);
    assert_eq!(owner.begin_close().unwrap().status, CleanupStatus::Complete);
    assert!(observed.get());
}

#[test]
fn clock_rollback_and_deadline_overflow_never_start_teardown_or_report_success() {
    let (owner, clock, _) = owner(1);
    ready(&owner);
    let state = control(Mode::Pending);
    register(&owner, &state);
    clock.0.set(Duration::MAX);
    assert_eq!(owner.begin_close(), Err(LifecycleError::DeadlineOverflow));
    assert_eq!(state.borrow().starts, 0);
    assert_eq!(owner.phase().unwrap(), WindowPhase::Ready);
    clock.0.set(Duration::ZERO);
    assert_eq!(owner.begin_close(), Err(LifecycleError::ClockWentBackwards));
    assert_eq!(state.borrow().starts, 0);
}

#[test]
fn creation_unwind_never_certifies_cleanup_or_reopens_reserved_capacity() {
    let (owner, _, _) = owner(1);
    let stamp = ready(&owner);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        owner.register(stamp, |_| -> Result<Child, ChildCreationFailure<Child>> {
            panic!("technical native creation unwind")
        })
    }));
    assert!(outcome.is_err());
    assert_eq!(owner.phase().unwrap(), WindowPhase::ForcedIncomplete);
    let report = owner.report().unwrap();
    assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
    assert_eq!(report.remaining, 1);
    assert_eq!(owner.window_removed(), Err(LifecycleError::WrongState));
}
