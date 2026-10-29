#![cfg(all(windows, feature = "window"))]
//! Each ignored test must run in a separate process: winit creates one loop per process.
use meari_native::{
    application::{WindowOptions, window_lifecycle::*},
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*, window::*},
};
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

#[derive(Debug, PartialEq, Eq)]
struct Failure(&'static str);
struct ObservedHost;
impl NativeWindowHost for ObservedHost {
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
        Ok(HostEffects {
            redraw: true,
            ..HostEffects::default()
        })
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Failure> {
        CloseDecision::HostCleanupComplete { checkpoint: () }
    }
}
struct ObservedController {
    document: NativeDocumentHandle,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
    frames: Rc<RefCell<Vec<NativeFrameObservation>>>,
    ready_commit: Rc<RefCell<Option<HostCommitReceipt>>>,
    observation: Option<NativeWindowObservation>,
    watchdog: Option<(mpsc::Sender<()>, JoinHandle<bool>)>,
}
impl NativeStartupController for ObservedController {
    type Host = ObservedHost;
    type Error = Failure;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<ObservedHost>, Failure> {
        let document = self.document.clone();
        let wake = self.wake.clone();
        let frames = self.frames.clone();
        let ready_commit = self.ready_commit.clone();
        let observer = Box::new(move |notice: NativeWindowNotice| {
            assert!(
                notice.window.is_none(),
                "plain observation grants no child authority"
            );
            match notice.kind {
                NativeWindowNoticeKind::Ready => {
                    let stamp = document.stamp().unwrap();
                    let (_, receipt) = document
                        .try_reproject(&stamp, |_| Ok::<_, Failure>(()))
                        .unwrap();
                    eprintln!(
                        "Ready before={:?} after={:?}",
                        receipt.before(),
                        receipt.after()
                    );
                    *ready_commit.borrow_mut() = Some(receipt);
                }
                NativeWindowNoticeKind::FrameSubmitted => {
                    let frame = notice.frame.unwrap();
                    eprintln!("submitted stamp={:?}", frame.document);
                    let count = {
                        let mut frames = frames.borrow_mut();
                        frames.push(frame);
                        frames.len()
                    };
                    if count == 1 {
                        wake.borrow().as_ref().unwrap().wake().unwrap();
                    } else {
                        wake.borrow().as_ref().unwrap().request_close().unwrap();
                    }
                }
                _ => {}
            }
            Ok(())
        });
        let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
            self.document.clone(),
            ObservedHost,
        )));
        let token = context
            .attach_observed(
                runtime,
                WindowOptions {
                    title: "Meari plain observed technical window".into(),
                },
                observer,
            )
            .map_err(|_| Failure("observed attach"))?;
        self.observation = token.observation();
        Ok(NativeStartupPoll::Attach(token))
    }
    fn attached(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<ObservedHost>>>,
        wake: NativeWindowWake,
    ) -> Result<(), Failure> {
        assert!(runtime.try_borrow_mut().is_ok());
        let (sender, receiver) = mpsc::channel();
        let watchdog_wake = wake.clone();
        self.watchdog = Some((
            sender,
            thread::spawn(move || {
                if receiver.recv_timeout(Duration::from_secs(5)).is_err() {
                    let _ = watchdog_wake.request_close();
                    true
                } else {
                    false
                }
            }),
        ));
        *self.wake.borrow_mut() = Some(wake);
        Ok(())
    }
}
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
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Failure {}
fn limits() -> HostScheduleLimits {
    HostScheduleLimits::new(Duration::from_millis(5), Duration::from_secs(1), 12).unwrap()
}
fn document() -> NativeDocumentHandle {
    let source=meari::parse("type Props={};export function App(props:Props){return <main>Strict native startup</main>;}",meari::application::Language::Tsx).unwrap();
    NativeDocumentHandle::new(
        InteractiveDocument::new(
            source,
            "main{padding:12px}",
            Viewport {
                width: 360,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap()
}
#[derive(Default)]
struct Control {
    parent: Option<HWND>,
    resized: bool,
    requested: bool,
    child_cleanup: usize,
    close_times: Vec<Instant>,
    fail_after_view: bool,
    after_calls: usize,
    failure_calls: usize,
    command: Option<mpsc::Sender<WakeCommand>>,
}
enum WakeCommand {
    Close,
    Stop,
}
fn failure_view_exists() -> bool {
    unsafe {
        FindWindowW(
            windows::core::PCWSTR::null(),
            windows::core::w!("Meari attached failure technical window"),
        )
    }
    .is_ok_and(|window| {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
        pid == std::process::id()
    })
}
struct Host {
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
}
impl NativeWindowHost for Host {
    type Error = Failure;
    type CloseCheckpoint = bool;
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Failure> {
        panic!("no source callback in technical window fixture")
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Failure> {
        {
            let mut state = self.control.borrow_mut();
            state.after_calls += 1;
            if state.fail_after_view && state.resized {
                state.failure_calls += 1;
                state
                    .command
                    .as_ref()
                    .unwrap()
                    .send(WakeCommand::Close)
                    .unwrap();
                return Err(Failure("exact post-view observation failure"));
            }
        }
        let request = {
            let mut state = self.control.borrow_mut();
            let request = state.resized && !state.requested;
            if request {
                state.requested = true;
            }
            request
        };
        if request {
            self.wake
                .borrow()
                .as_ref()
                .unwrap()
                .request_close()
                .unwrap();
        }
        Ok(HostEffects::default())
    }
    fn close(&mut self, request: CloseRequest<bool>) -> CloseDecision<bool, Failure> {
        let mut state = self.control.borrow_mut();
        if let Some(parent) = state.parent {
            assert!(unsafe { IsWindow(Some(parent)).as_bool() });
            assert_eq!(state.child_cleanup, 1);
        }
        state.close_times.push(Instant::now());
        if request.checkpoint.unwrap_or(false) {
            CloseDecision::HostCleanupComplete { checkpoint: true }
        } else {
            CloseDecision::Pending { checkpoint: true }
        }
    }
    fn next_close_observation(&self, _: &bool) -> HostRetry {
        HostRetry::After(Duration::from_millis(10))
    }
}
struct Timer {
    worker: Option<JoinHandle<()>>,
    release: Option<mpsc::Sender<()>>,
    calls: usize,
    joined: usize,
    first: Option<Instant>,
    deadline: Instant,
}
impl NativeStartupController for Timer {
    type Host = Host;
    type Error = Failure;
    fn observe(
        &mut self,
        _: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<Host>, Failure> {
        if Instant::now() > self.deadline {
            return Err(Failure("doc0 timer timeout"));
        }
        self.calls += 1;
        eprintln!(
            "doc0 observe={} elapsed={:?}",
            self.calls,
            self.first.map(|first| first.elapsed())
        );
        if self.first.is_none() {
            assert!(!self.worker.as_ref().unwrap().is_finished());
            self.first = Some(Instant::now());
            self.release.take().unwrap().send(()).unwrap();
            return Ok(NativeStartupPoll::Pending(HostRetry::After(
                Duration::from_millis(10),
            )));
        }
        assert!(self.first.unwrap().elapsed() >= Duration::from_millis(10));
        if self.worker.as_ref().unwrap().is_finished() {
            self.worker.take().unwrap().join().unwrap();
            self.joined += 1;
            Ok(NativeStartupPoll::Stop)
        } else {
            Ok(NativeStartupPoll::Pending(HostRetry::After(
                Duration::from_millis(10),
            )))
        }
    }
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

struct ParentProbe {
    lease: NativeParentLease,
    control: Rc<RefCell<Control>>,
}
impl NativeChild for ParentProbe {
    type Error = std::io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        request: CleanupRequest,
        _: CompletionSignal,
    ) -> Result<CleanupProgress, Self::Error> {
        self.lease
            .with_cleanup_parent(request, |_| ())
            .map_err(std::io::Error::other)?;
        self.control.borrow_mut().child_cleanup += 1;
        Ok(CleanupProgress::Complete)
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
        panic!("complete child cleanup must not restart")
    }
}
struct WindowController {
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
    mounted: bool,
    hook_calls: usize,
    watchdog: Option<(mpsc::Sender<WakeCommand>, JoinHandle<bool>)>,
}
impl WindowController {
    fn stop_watchdog(&mut self) -> bool {
        if let Some((sender, worker)) = self.watchdog.take() {
            let _ = sender.send(WakeCommand::Stop);
            worker.join().unwrap()
        } else {
            false
        }
    }
}
impl NativeStartupController for WindowController {
    type Host = ChildWindowHost<Host, ParentProbe>;
    type Error = Failure;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<Self::Host>, Failure> {
        assert!(!self.mounted);
        self.mounted = true;
        let registrar: Rc<RefCell<Option<NativeChildRegistrar<ParentProbe>>>> =
            Rc::new(RefCell::new(None));
        let registered = registrar.clone();
        let control = self.control.clone();
        let wake = self.wake.clone();
        let observer = Box::new(move |notice: NativeWindowNotice| {
            if notice.kind == NativeWindowNoticeKind::Ready {
                use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let control = control.clone();
                registered
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .register(notice.window.unwrap(), |parent, lease| {
                        let raw = parent.window_handle().unwrap().as_raw();
                        let RawWindowHandle::Win32(raw) = raw else {
                            panic!("Windows parent required")
                        };
                        let hwnd = HWND(raw.hwnd.get() as *mut std::ffi::c_void);
                        control.borrow_mut().parent = Some(hwnd);
                        let mut rect = RECT::default();
                        unsafe { GetWindowRect(hwnd, &mut rect) }.unwrap();
                        unsafe {
                            MoveWindow(
                                hwnd,
                                rect.left,
                                rect.top,
                                rect.right - rect.left + 80,
                                rect.bottom - rect.top + 60,
                                true,
                            )
                        }
                        .unwrap();
                        Ok(ParentProbe { lease, control })
                    })
                    .unwrap();
            }
            if notice.kind == NativeWindowNoticeKind::GeometryChanged
                && notice
                    .geometry
                    .as_ref()
                    .is_some_and(|geometry| geometry.environment.layout.width > 360)
            {
                let should_wake = {
                    let mut state = control.borrow_mut();
                    let first = !state.resized;
                    state.resized = true;
                    first
                };
                if should_wake {
                    let mut rect = RECT::default();
                    unsafe { GetClientRect(control.borrow().parent.unwrap(), &mut rect) }.unwrap();
                    assert!(rect.right > 360);
                    wake.borrow().as_ref().unwrap().wake().unwrap();
                }
            }
            Ok(())
        });
        let doc = document();
        let prepared = context
            .prepare(
                doc.clone(),
                LifecycleLimits::new(1, Duration::from_secs(2)).unwrap(),
                WindowOptions {
                    title: "Meari strict startup technical window".into(),
                },
                Some(observer),
            )
            .map_err(|_| Failure("prepare"))?;
        *registrar.borrow_mut() = Some(prepared.registrar());
        let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
            doc,
            ChildWindowHost::new(
                Host {
                    control: self.control.clone(),
                    wake: self.wake.clone(),
                },
                prepared.children(),
            ),
        )));
        let token = context
            .attach_prepared(prepared, runtime)
            .map_err(|_| Failure("attachment"))?;
        Ok(NativeStartupPoll::Attach(token))
    }
    fn attached(
        &mut self,
        _: Rc<RefCell<NativeHostRuntime<Self::Host>>>,
        wake: NativeWindowWake,
    ) -> Result<(), Failure> {
        self.hook_calls += 1;
        assert!(self.control.borrow().parent.is_none());
        let (sender, receiver) = mpsc::channel();
        self.control.borrow_mut().command = Some(sender.clone());
        let watchdog_wake = wake.clone();
        let worker = thread::spawn(
            move || match receiver.recv_timeout(Duration::from_secs(5)) {
                Ok(WakeCommand::Stop) => false,
                Ok(WakeCommand::Close) => {
                    let _ = watchdog_wake.request_close();
                    false
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = watchdog_wake.request_close();
                    true
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => false,
            },
        );
        self.watchdog = Some((sender, worker));
        *self.wake.borrow_mut() = Some(wake);
        Ok(())
    }
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
struct HookFailure {
    runtime: Option<Rc<RefCell<NativeHostRuntime<Host>>>>,
    control: Rc<RefCell<Control>>,
    wake: Rc<RefCell<Option<NativeWindowWake>>>,
}
impl NativeStartupController for HookFailure {
    type Host = Host;
    type Error = Failure;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<Host>, Failure> {
        let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
            document(),
            Host {
                control: self.control.clone(),
                wake: self.wake.clone(),
            },
        )));
        self.runtime = Some(runtime.clone());
        Ok(NativeStartupPoll::Attach(
            context
                .attach(
                    runtime,
                    WindowOptions {
                        title: "Meari attached failure technical window".into(),
                    },
                )
                .map_err(|_| Failure("attach"))?,
        ))
    }
    fn attached(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<Host>>>,
        _: NativeWindowWake,
    ) -> Result<(), Failure> {
        assert!(Rc::ptr_eq(&runtime, self.runtime.as_ref().unwrap()));
        assert!(runtime.try_borrow_mut().is_ok());
        assert!(
            !failure_view_exists(),
            "actual fixture HWND exists before attached error"
        );
        Err(Failure("exact canonical attached failure"))
    }
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
