//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{
        FindWindowW, GetClientRect, GetWindowRect, GetWindowThreadProcessId, IsWindow, MoveWindow,
    },
};

#[test]
#[ignore = "explicit Windows plain observed startup technical window slot"]
fn plain_observed_frames_keep_the_pre_ready_document_stamp() {
    let mut owner = NativeLoopOwner::new(limits()).unwrap();
    let startup = owner.startup().unwrap();
    let document = document();
    let before = document.stamp().unwrap();
    let frames = Rc::new(RefCell::new(Vec::new()));
    let mut result = owner.run_startup(
        startup,
        ObservedController {
            document: document.clone(),
            wake: Rc::new(RefCell::new(None)),
            frames: frames.clone(),
            ready_commit: Rc::new(RefCell::new(None)),
            observation: None,
            watchdog: None,
        },
    );
    let (stop, worker) = result.controller.watchdog.take().unwrap();
    let _ = stop.send(());
    assert!(!worker.join().unwrap(), "window exceeded fixture deadline");
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert_eq!(document.lifecycle(), HostLifecycle::Closed);
    let frames = frames.borrow();
    assert!(frames.len() >= 2);
    let ready_commit = result.controller.ready_commit.borrow();
    let ready_commit = ready_commit.as_ref().unwrap();
    assert_eq!(&frames[0].document, ready_commit.before());
    assert_eq!(frames[0].document.owner, before.owner);
    assert_eq!(
        frames[1].document.document_revision,
        ready_commit.after().document_revision
    );
    assert_ne!(frames[0].submission.ticket, frames[1].submission.ticket);
    assert!(
        frames
            .iter()
            .all(|frame| frame.submission.width > 0 && frame.submission.height > 0)
    );
    assert!(matches!(
        result
            .controller
            .observation
            .as_ref()
            .unwrap()
            .retry_observer(),
        Err(NativeNoticeError::Revoked)
    ));
    println!(
        "plain observed frames={} initial_revision={} next_revision={} closed=true",
        frames.len(),
        frames[0].document.document_revision,
        frames[1].document.document_revision
    );
}

#[test]
#[ignore = "explicit Windows doc0 loop/timer slot; no window/system suspend"]
fn document_zero_last_notification_then_future_timer_joins_without_more_input() {
    let mut owner = NativeLoopOwner::new(limits()).unwrap();
    let startup = owner.startup().unwrap();
    assert!(matches!(
        owner.startup(),
        Err(NativeStartupError::AlreadyIssued)
    ));
    let wake = startup.wake_handle();
    let worker_wake = wake.clone();
    let notifications = Arc::new(AtomicUsize::new(0));
    let count = notifications.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        assert!(worker_wake.notify().unwrap());
        count.fetch_add(1, Ordering::SeqCst);
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    eprintln!("doc0 final notification received; entering owned loop");
    let mut result = owner.run_startup(
        startup,
        Timer {
            worker: Some(worker),
            release: Some(release_tx),
            calls: 0,
            joined: 0,
            first: None,
            deadline: Instant::now() + Duration::from_secs(5),
        },
    );
    if let Some(release) = result.controller.release.take() {
        let _ = release.send(());
    }
    if let Some(worker) = result.controller.worker.take() {
        worker.join().unwrap();
    }
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert_eq!(result.controller.joined, 1);
    assert!(result.controller.calls >= 2 && result.controller.calls <= 12);
    assert_eq!(notifications.load(Ordering::SeqCst), 1);
    assert!(result.runtime.is_none());
    assert!(
        result
            .startup
            .as_ref()
            .unwrap()
            .retained_document()
            .is_none()
    );
    assert!(matches!(wake.notify(), Err(NativeStartupError::Closed)));
    println!(
        "doc0=true notifications=1 joined=1 observations={} additional_input=0 windows=0",
        result.controller.calls
    );
}

#[test]
#[ignore = "explicit Windows owned technical window slot"]
fn strict_startup_same_loop_prepared_resize_and_pending_host_timer_close() {
    let mut owner = NativeLoopOwner::new(limits()).unwrap();
    let startup = owner.startup().unwrap();
    let notify = startup.wake_handle();
    let control = Rc::new(RefCell::new(Control::default()));
    let wake = Rc::new(RefCell::new(None));
    let mut result = owner.run_startup(
        startup,
        WindowController {
            control: control.clone(),
            wake,
            mounted: false,
            hook_calls: 0,
            watchdog: None,
        },
    );
    let timed_out = result.controller.stop_watchdog();
    assert!(
        !timed_out,
        "owned watchdog recovered a timed-out window; not a normal gate pass"
    );
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert_eq!(result.controller.hook_calls, 1);
    let state = control.borrow();
    assert!(state.resized);
    assert_eq!(state.child_cleanup, 1);
    assert_eq!(state.close_times.len(), 2);
    assert!(state.close_times[1].duration_since(state.close_times[0]) >= Duration::from_millis(10));
    assert!(!unsafe { IsWindow(state.parent).as_bool() });
    assert_eq!(
        result
            .runtime
            .as_ref()
            .unwrap()
            .borrow()
            .document()
            .lifecycle(),
        HostLifecycle::Closed
    );
    assert!(matches!(notify.notify(), Err(NativeStartupError::Closed)));
    println!(
        "same_loop=true resized=true child_cleanup=1 host_close_observations=2 parent_window=false"
    );
}

#[test]
#[ignore = "explicit Windows loop slot; attached failure before actual View"]
fn attached_failure_returns_the_same_owner_and_error_without_creating_a_view() {
    let mut owner = NativeLoopOwner::new(limits()).unwrap();
    let startup = owner.startup().unwrap();
    let notify = startup.wake_handle();
    let control = Rc::new(RefCell::new(Control::default()));
    let result = owner.run_startup(
        startup,
        HookFailure {
            runtime: None,
            control: control.clone(),
            wake: Rc::new(RefCell::new(None)),
        },
    );
    assert!(matches!(
        result.result,
        Err(NativeRunError::Consumer(Failure(
            "exact canonical attached failure"
        )))
    ));
    assert!(Rc::ptr_eq(
        result.runtime.as_ref().unwrap(),
        result.controller.runtime.as_ref().unwrap()
    ));
    assert_eq!(control.borrow().close_times.len(), 0);
    assert!(control.borrow().parent.is_none());
    assert!(!failure_view_exists());
    assert_eq!(
        result
            .runtime
            .as_ref()
            .unwrap()
            .borrow()
            .document()
            .lifecycle(),
        HostLifecycle::ForcedIncomplete
    );
    assert!(matches!(notify.notify(), Err(NativeStartupError::Closed)));
    println!(
        "attached_failure=canonical same_runtime=true host_cleanup=0 actual_view=0 normal_close=false"
    );
}

#[test]
#[ignore = "explicit Windows post-view failure ownership slot"]
fn post_view_typed_error_is_retained_while_explicit_close_reclaims_the_owned_window() {
    let mut owner = NativeLoopOwner::new(limits()).unwrap();
    let startup = owner.startup().unwrap();
    let control = Rc::new(RefCell::new(Control {
        fail_after_view: true,
        ..Default::default()
    }));
    let mut result = owner.run_startup(
        startup,
        WindowController {
            control: control.clone(),
            wake: Rc::new(RefCell::new(None)),
            mounted: false,
            hook_calls: 0,
            watchdog: None,
        },
    );
    assert!(!result.controller.stop_watchdog());
    assert!(result.result.is_ok(), "{:?}", result.result);
    let runtime = result.runtime.as_ref().unwrap().borrow();
    assert!(matches!(
        runtime.last_host_error(),
        Some(ChildHostError::Host(Failure(
            "exact post-view observation failure"
        )))
    ));
    assert_eq!(runtime.document().lifecycle(), HostLifecycle::Closed);
    let state = control.borrow();
    assert!(state.resized);
    assert_eq!(state.failure_calls, 1);
    assert_eq!(state.child_cleanup, 1);
    assert_eq!(state.close_times.len(), 2);
    assert!(!unsafe { IsWindow(state.parent).as_bool() });
    println!(
        "post_view_error=canonical failed_observations=1 after_dispatch={} explicit_close=true child_cleanup=1 parent_window=false",
        state.after_calls
    );
}
