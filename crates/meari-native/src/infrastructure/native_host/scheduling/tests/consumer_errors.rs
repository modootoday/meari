//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use std::{cell::RefCell, rc::Rc, task::Waker};

#[test]
fn consumer_error_is_retained_without_string_based_retry_classification() {
    let control = Rc::new(RefCell::new(Control {
        after_error: Some(41),
        ..Default::default()
    }));
    let mut runtime =
        NativeHostRuntime::with_schedule_limits(document(), Host(control.clone()), limits());
    assert!(runtime.after_dispatch(HostTurnReason::Completion).is_err());
    assert_eq!(runtime.last_host_error().unwrap().0, 41);
    assert_eq!(runtime.turn_retry, Some(HostRetry::OnWake));
    control.borrow_mut().after_error = Some(99);
    assert!(runtime.after_dispatch(HostTurnReason::Completion).is_err());
    assert_eq!(runtime.turn_retry, None);
    assert_eq!(runtime.last_host_error().unwrap().0, 99);
}

#[test]
fn pending_is_schedulable_but_true_failure_does_not_consult_a_retry_hint() {
    let control = Rc::new(RefCell::new(Control {
        wait: true,
        ..Default::default()
    }));
    let mut runtime =
        NativeHostRuntime::with_schedule_limits(document(), Host(control.clone()), limits());
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(
        runtime.close_retry,
        Some(HostRetry::After(Duration::from_millis(5)))
    );
    control.borrow_mut().fail_close = true;
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(runtime.close_retry, None);
    assert_eq!(control.borrow().hints, 1);
    assert_eq!(runtime.last_host_error().unwrap().0, 82);
    let closes = control.borrow().closes;
    assert!(runtime.observe_close(CloseCause::Host).is_err());
    assert_eq!(control.borrow().closes, closes);
}

#[test]
fn child_first_barrier_defers_host_deadline_until_the_actual_host_checkpoint_exists() {
    let children = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    let stamp = children.ready(ReadyKind::NewWindow).unwrap();
    let child = Rc::new(RefCell::new(ChildState::default()));
    children
        .register(stamp, |_| Ok(Child(child.clone())))
        .unwrap();
    let control = Rc::new(RefCell::new(Control {
        wait: true,
        ..Default::default()
    }));
    let mut runtime = NativeHostRuntime::with_schedule_limits(
        document(),
        ChildWindowHost::new(Host(control.clone()), children),
        limits(),
    );
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(runtime.close_retry, Some(HostRetry::OnWake));
    assert_eq!(control.borrow().closes, 0);
    child.borrow_mut().done = true;
    child.borrow().signal.as_ref().unwrap().notify();
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(
        runtime.close_retry,
        Some(HostRetry::After(Duration::from_millis(5)))
    );
    assert_eq!(control.borrow().closes, 1);
}

#[test]
fn legacy_constructor_keeps_timed_observation_opt_in() {
    let runtime =
        NativeHostRuntime::new(document(), Host(Rc::new(RefCell::new(Control::default()))));
    assert!(runtime.schedule_limits.is_none());
}

#[test]
fn unavailable_child_close_retains_closing_checkpoint_and_recovers_on_original_signal() {
    let children = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    let stamp = children.ready(ReadyKind::NewWindow).unwrap();
    let child = Rc::new(RefCell::new(ObservationState::default()));
    let lease = children
        .register(stamp, |_| {
            Ok(ObservationChild {
                state: child.clone(),
                diagnostic: Failure(17),
            })
        })
        .unwrap();
    let control = Rc::new(RefCell::new(Control::default()));
    let mut runtime = NativeHostRuntime::with_schedule_limits(
        document(),
        ChildWindowHost::new(Host(control.clone()), children.clone()),
        limits(),
    );
    assert!(runtime.observe_close(CloseCause::Host).is_err());
    let handle = runtime.document();
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::ObservationUnavailable
    );
    assert_eq!(handle.lifecycle(), HostLifecycle::Closing);
    assert_eq!(runtime.close_retry, Some(HostRetry::OnWake));
    assert!(
        matches!(runtime.last_host_error(),Some(ChildHostError::Children(report)) if report.unavailable == 1)
    );
    assert_eq!(
        children
            .with_error(&lease, |error| error.unwrap().0)
            .unwrap(),
        17
    );
    assert!(runtime.finish_headless_close().is_err());
    for _ in 0..4 {
        assert_eq!(
            runtime.request_close(CloseCause::Host).unwrap(),
            HostCloseStatus::ObservationUnavailable
        );
    }
    assert_eq!(
        (
            child.borrow().starts,
            child.borrow().observations,
            child.borrow().dropped
        ),
        (1, 0, 0)
    );
    assert_eq!(control.borrow().closes, 0);
    assert_eq!(Rc::strong_count(&children), 2);
    let original = child.borrow().signal.as_ref().unwrap().clone();
    child.borrow_mut().done = true;
    assert!(original.notify());
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(
        (
            child.borrow().starts,
            child.borrow().observations,
            child.borrow().dropped
        ),
        (1, 1, 1)
    );
    assert_eq!(control.borrow().closes, 1);
    runtime.finish_headless_close().unwrap();
    assert_eq!(handle.lifecycle(), HostLifecycle::Closed);
}

#[test]
fn wrapped_host_observation_error_is_retained_without_failure_or_timed_retry_hint() {
    let children = Rc::new(
        WindowLifecycle::<Child>::new(
            LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    children.ready(ReadyKind::NewWindow).unwrap();
    let control = Rc::new(RefCell::new(Control {
        unavailable_close: true,
        ..Default::default()
    }));
    let mut runtime = NativeHostRuntime::with_schedule_limits(
        document(),
        ChildWindowHost::new(Host(control.clone()), children),
        limits(),
    );
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::ObservationUnavailable
    );
    assert_eq!(runtime.document().lifecycle(), HostLifecycle::Closing);
    assert_eq!(runtime.close_retry, Some(HostRetry::OnWake));
    assert_eq!(control.borrow().hints, 0);
    assert!(matches!(
        runtime.last_host_error(),
        Some(ChildHostError::Host(Failure(84)))
    ));
    control.borrow_mut().unavailable_close = false;
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
}

#[test]
fn mixed_unavailable_and_failed_children_retry_only_on_explicit_host_request() {
    let children = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(2, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    let ready = children.ready(ReadyKind::NewWindow).unwrap();
    let unknown = Rc::new(RefCell::new(ObservationState::default()));
    let failed = Rc::new(RefCell::new(ObservationState {
        fail_once: true,
        pending: true,
        ..Default::default()
    }));
    let unknown_lease = children
        .register(ready, |_| {
            Ok(ObservationChild {
                state: unknown.clone(),
                diagnostic: Failure(17),
            })
        })
        .unwrap();
    children
        .register(ready, |_| {
            Ok(ObservationChild {
                state: failed.clone(),
                diagnostic: Failure(18),
            })
        })
        .unwrap();
    let control = Rc::new(RefCell::new(Control::default()));
    let mut runtime = NativeHostRuntime::with_schedule_limits(
        document(),
        ChildWindowHost::new(Host(control.clone()), children.clone()),
        limits(),
    );
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::ObservationUnavailable
    );
    assert_eq!(
        (
            children.report().unwrap().failed,
            children.report().unwrap().unavailable
        ),
        (1, 1)
    );
    let original = unknown.borrow().signal.as_ref().unwrap().clone();
    let original_stamp = original.stamp();
    let failed_old = failed.borrow().signal.as_ref().unwrap().clone();
    for _ in 0..3 {
        assert_eq!(
            runtime.observe_close(CloseCause::Host).unwrap(),
            HostCloseStatus::ObservationUnavailable
        );
    }
    assert_eq!((unknown.borrow().starts, failed.borrow().starts), (1, 1));
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::ObservationUnavailable
    );
    assert_eq!((unknown.borrow().starts, failed.borrow().starts), (1, 2));
    assert_eq!(
        unknown.borrow().signal.as_ref().unwrap().stamp(),
        original_stamp
    );
    assert!(!failed_old.notify());
    assert_eq!(
        children
            .with_error(&unknown_lease, |error| error.unwrap().0)
            .unwrap(),
        17
    );
    assert_eq!(runtime.document().lifecycle(), HostLifecycle::Closing);
    assert_eq!(runtime.close_retry, Some(HostRetry::OnWake));
    for _ in 0..3 {
        runtime.observe_close(CloseCause::Host).unwrap();
    }
    assert_eq!(failed.borrow().starts, 2);
    assert_eq!(control.borrow().closes, 0);
    failed.borrow_mut().done = true;
    failed.borrow().signal.as_ref().unwrap().notify();
    runtime.observe_close(CloseCause::Host).unwrap();
    unknown.borrow_mut().done = true;
    assert!(original.notify());
    assert_eq!(
        runtime.observe_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!((unknown.borrow().starts, failed.borrow().starts), (1, 2));
    assert_eq!(control.borrow().closes, 1);
}

#[test]
#[cfg(feature = "window")]
fn private_port_retains_initial_explicit_close_across_actual_runtime_borrow_conflict() {
    let control = Rc::new(RefCell::new(Control {
        wait: true,
        ..Default::default()
    }));
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::with_schedule_limits(
        document(),
        Host(control.clone()),
        limits(),
    )));
    let port = crate::infrastructure::native_host::window_port::port(runtime.clone()).unwrap();
    let guard = runtime.borrow_mut();
    for _ in 0..2 {
        let error = port.close(CloseCause::User, true).unwrap_err();
        assert_eq!(error.error, HostAccessError::Busy);
        assert_eq!(error.retry, Some(HostRetry::OnWake));
    }
    assert_eq!(control.borrow().closes, 0);
    drop(guard);
    assert_eq!(
        port.close(CloseCause::Host, false).unwrap().0,
        HostCloseStatus::Pending
    );
    assert_eq!(
        (control.borrow().closes, control.borrow().close_observations),
        (1, 0)
    );
    for _ in 0..3 {
        port.close(CloseCause::Host, false).unwrap();
    }
    assert_eq!(
        control.borrow().closes - control.borrow().close_observations,
        1
    );
    control.borrow_mut().wait = false;
    assert_eq!(
        port.close(CloseCause::Host, false).unwrap().0,
        HostCloseStatus::HostCleanupComplete
    );
    port.removed(false).unwrap();
    assert_eq!(
        runtime.borrow().document().lifecycle(),
        HostLifecycle::Closed
    );
}
