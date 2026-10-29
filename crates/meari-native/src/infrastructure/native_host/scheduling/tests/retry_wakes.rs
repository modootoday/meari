//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use std::{cell::RefCell, rc::Rc, task::Waker};

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
