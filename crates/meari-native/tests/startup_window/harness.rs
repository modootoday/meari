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

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Failure {}

pub(super) fn limits() -> HostScheduleLimits {
    HostScheduleLimits::new(Duration::from_millis(5), Duration::from_secs(1), 12).unwrap()
}

pub(super) fn document() -> NativeDocumentHandle {
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

pub(super) fn failure_view_exists() -> bool {
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
