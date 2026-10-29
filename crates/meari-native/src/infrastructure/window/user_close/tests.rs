use super::*;
use crate::domain::window_lifecycle::WindowStamp;
use crate::infrastructure::window::{NativeWindowWake, wake};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicUsize},
};
fn scope(surface: u64) -> ActionScope {
    ActionScope {
        host: 7,
        window: WindowStamp {
            owner: 1,
            epoch: 1,
            surface,
        },
    }
}
#[derive(Debug)]
struct Count(AtomicUsize);
impl winit::event_loop::EventLoopProxyProvider for Count {
    fn wake_up(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn target() -> (NativeWindowWake, Arc<Count>) {
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let (proxy, _events) =
        blitz_shell::BlitzShellProxy::new(winit::event_loop::EventLoopProxy::new(count.clone()));
    (
        NativeWindowWake {
            host: 7,
            proxy,
            pending: Arc::new(AtomicU8::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
            timers: Default::default(),
            user_close: Default::default(),
        },
        count,
    )
}
#[test]
fn scoped_user_close_coalesces_one_original_scope_and_preserves_host_close_bit() {
    let (wake, count) = target();
    assert_eq!(wake.request_user_close(scope(1)), Ok(true));
    assert_eq!(wake.request_user_close(scope(1)), Ok(false));
    assert_eq!(wake.request_user_close(scope(2)), Err(ActionError::Busy));
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(wake.take(), wake::USER_CLOSE);
    assert_eq!(wake.user_close.take(), Some(scope(1)));
    assert!(wake.request_close().unwrap());
    assert_eq!(wake.take(), wake::CLOSE);
    assert_eq!(wake.user_close.take(), None);
}
#[test]
fn foreign_closed_and_stale_user_close_never_acquire_a_current_window() {
    let (wake, _) = target();
    let mut foreign = scope(1);
    foreign.host = 8;
    assert_eq!(
        wake.request_user_close(foreign),
        Err(ActionError::ForeignHost)
    );
    assert_eq!(wake.take(), 0);
    assert_eq!(
        validate(Some(scope(1)), Ok(scope(2))),
        Err(ActionError::StaleWindow)
    );
    assert_eq!(
        validate(Some(scope(1)), Err(ActionError::Closing)),
        Err(ActionError::Closing)
    );
    assert_eq!(validate(None, Ok(scope(1))), Err(ActionError::StaleRequest));
    assert_eq!(validate(Some(scope(1)), Ok(scope(1))), Ok(()));
    wake.request_user_close(scope(1)).unwrap();
    wake.close();
    assert_eq!(wake.user_close.take(), None);
    assert_eq!(wake.take(), 0);
    assert_eq!(wake.request_user_close(scope(1)), Err(ActionError::Closed));
}

#[test]
fn user_veto_or_stale_scope_preserves_coalesced_completion_actions_timer_and_lifecycle_once() {
    let work = wake::COMPLETE | wake::ACTIONS | wake::TIMER_UPDATE | wake::LIFECYCLE | wake::RETRY;
    for current in [Ok(scope(1)), Ok(scope(2))] {
        let (wake, count) = target();
        wake.request_user_close(scope(1)).unwrap();
        wake.send(work).unwrap();
        let bits = wake.take();
        let expected = wake.user_close.take();
        let _validation = validate(expected, current);
        // Valid-but-vetoed and invalid requests both leave the original host Open.
        let remaining = continuation(bits, true);
        assert_eq!(remaining, work);
        let dispatched = [
            wake::COMPLETE,
            wake::ACTIONS,
            wake::TIMER_UPDATE,
            wake::LIFECYCLE,
            wake::RETRY,
        ]
        .into_iter()
        .filter(|bit| remaining & bit != 0)
        .collect::<Vec<_>>();
        assert_eq!(dispatched.len(), 5);
        assert_eq!(wake.take(), 0);
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert_eq!(wake.user_close.take(), None);
    }
}
#[test]
fn accepted_user_close_and_explicit_host_close_do_not_replay_open_owner_work() {
    let work = wake::COMPLETE | wake::ACTIONS | wake::TIMER_UPDATE;
    assert_eq!(continuation(wake::USER_CLOSE | work, false), 0);
    assert_eq!(continuation(wake::USER_CLOSE | wake::CLOSE | work, true), 0);
}
