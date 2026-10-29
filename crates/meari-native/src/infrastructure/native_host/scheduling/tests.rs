use super::*;
use crate::{
    application::window_lifecycle::*,
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*},
};
use std::{cell::RefCell, fmt, rc::Rc, task::Waker};
#[derive(Debug)]
struct Failure(u64);
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "canonical {}", self.0)
    }
}
impl std::error::Error for Failure {}
#[derive(Default)]
struct Control {
    after_error: Option<u64>,
    wait: bool,
    fail_close: bool,
    unavailable_close: bool,
    closes: usize,
    close_observations: usize,
    hints: usize,
}
struct Host(Rc<RefCell<Control>>);
impl NativeWindowHost for Host {
    type Error = Failure;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Failure> {
        panic!("no callback")
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Failure> {
        if let Some(value) = self.0.borrow().after_error {
            Err(Failure(value))
        } else {
            Ok(HostEffects::default())
        }
    }
    fn retry_after_dispatch(&self, error: &Failure) -> Option<HostRetry> {
        (error.0 == 41).then_some(HostRetry::OnWake)
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Failure> {
        let mut state = self.0.borrow_mut();
        state.closes += 1;
        if state.unavailable_close {
            CloseDecision::ObservationUnavailable {
                checkpoint: (),
                error: Failure(84),
            }
        } else if state.fail_close {
            CloseDecision::RetryableFailure {
                checkpoint: (),
                error: Failure(82),
            }
        } else if state.wait {
            CloseDecision::Pending { checkpoint: () }
        } else {
            CloseDecision::HostCleanupComplete { checkpoint: () }
        }
    }
    fn next_close_observation(&self, _: &()) -> HostRetry {
        self.0.borrow_mut().hints += 1;
        HostRetry::After(Duration::from_millis(5))
    }
    fn observe_close(&mut self, request: CloseRequest<()>) -> CloseDecision<(), Failure> {
        self.0.borrow_mut().close_observations += 1;
        self.close(request)
    }
}
fn document() -> NativeDocumentHandle {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>Schedule</main>;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    NativeDocumentHandle::new(
        InteractiveDocument::new(
            source,
            "",
            Viewport {
                width: 320,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap()
}
fn limits() -> HostScheduleLimits {
    HostScheduleLimits::new(Duration::from_millis(2), Duration::from_secs(1), 3).unwrap()
}
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
#[derive(Clone)]
struct Clock;
impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}
#[derive(Default)]
struct ChildState {
    done: bool,
    signal: Option<CompletionSignal>,
}
struct Child(Rc<RefCell<ChildState>>);
impl NativeChild for Child {
    type Error = std::io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, Self::Error> {
        self.0.borrow_mut().signal = Some(signal);
        Ok(CleanupProgress::Pending)
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
        Ok(if self.0.borrow().done {
            CleanupProgress::Complete
        } else {
            CleanupProgress::Pending
        })
    }
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

#[derive(Default)]
struct ObservationState {
    done: bool,
    fail_once: bool,
    pending: bool,
    starts: usize,
    observations: usize,
    dropped: usize,
    signal: Option<CompletionSignal>,
}
struct ObservationChild {
    state: Rc<RefCell<ObservationState>>,
    diagnostic: Failure,
}
impl Drop for ObservationChild {
    fn drop(&mut self) {
        self.state.borrow_mut().dropped += 1;
    }
}
impl NativeChild for ObservationChild {
    type Error = Failure;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, Failure> {
        let mut state = self.state.borrow_mut();
        state.starts += 1;
        state.signal = Some(signal);
        if state.fail_once {
            state.fail_once = false;
            return Err(Failure(99));
        }
        Ok(if state.pending {
            CleanupProgress::Pending
        } else {
            CleanupProgress::ObservationUnavailable
        })
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Failure> {
        let mut state = self.state.borrow_mut();
        state.observations += 1;
        Ok(if state.done {
            CleanupProgress::Complete
        } else if state.pending {
            CleanupProgress::Pending
        } else {
            CleanupProgress::ObservationUnavailable
        })
    }
    fn observation_error(&self) -> Option<&Failure> {
        Some(&self.diagnostic)
    }
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

#[test]
#[cfg(feature = "window")]
fn private_port_consumes_busy_explicit_retry_once_without_promoting_later_wakes() {
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
    children
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
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::with_schedule_limits(
        document(),
        ChildWindowHost::new(Host(Rc::new(RefCell::new(Control::default()))), children),
        limits(),
    )));
    let port = crate::infrastructure::native_host::window_port::port(runtime.clone()).unwrap();
    assert_eq!(
        port.close(CloseCause::Host, true).unwrap().0,
        HostCloseStatus::ObservationUnavailable
    );
    let original = unknown.borrow().signal.as_ref().unwrap().stamp();
    let guard = runtime.borrow_mut();
    assert_eq!(
        port.close(CloseCause::Host, true).unwrap_err().error,
        HostAccessError::Busy
    );
    assert_eq!(
        port.close(CloseCause::Host, true).unwrap_err().error,
        HostAccessError::Busy
    );
    drop(guard);
    // A second terminal failure makes accidental repeated explicit consumption observable.
    failed.borrow_mut().fail_once = true;
    assert_eq!(
        port.close(CloseCause::Host, false).unwrap().0,
        HostCloseStatus::ObservationUnavailable
    );
    assert_eq!((unknown.borrow().starts, failed.borrow().starts), (1, 2));
    for _ in 0..4 {
        port.close(CloseCause::Host, false).unwrap();
    }
    assert_eq!((unknown.borrow().starts, failed.borrow().starts), (1, 2));
    assert_eq!(unknown.borrow().signal.as_ref().unwrap().stamp(), original);
    assert_eq!(
        runtime.borrow().document().lifecycle(),
        HostLifecycle::Closing
    );
    assert_eq!(runtime.borrow().close_retry, Some(HostRetry::OnWake));
}
