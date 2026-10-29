use super::*;
use parent::{Child, Mode, State};

type Runtime = Rc<RefCell<NativeHostRuntime<ChildWindowHost<Host, Child>>>>;
struct Probe {
    driver: Option<NativeWindowDriver>,
    runtime: Runtime,
    children: Rc<WindowLifecycle<Child>>,
    registrar: NativeChildRegistrar<Child>,
    host_closes: Rc<Cell<usize>>,
    pending: Rc<RefCell<State>>,
    failing: Rc<RefCell<State>>,
    old_lease: Option<ChildLease>,
    stamp: Option<WindowStamp>,
    geometry: u64,
    stage: u8,
    deadline: Instant,
    completed: Rc<Cell<bool>>,
    failure: Rc<RefCell<Option<String>>>,
}
impl Probe {
    fn dispatch(&mut self, event_loop: &dyn ActiveEventLoop, call: impl FnOnce(&mut Self)) {
        if self.driver.is_none() {
            return;
        }
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(self)))
        {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "native probe callback panicked".into());
            *self.failure.borrow_mut() = Some(message);
            // Release only this probe's window before asserting outside the Windows callback loop.
            drop(self.driver.take());
            event_loop.exit();
        }
    }
    fn advance(&mut self, event_loop: &dyn ActiveEventLoop) {
        assert!(
            Instant::now() < self.deadline,
            "native lifecycle gate timed out at stage {}",
            self.stage
        );
        match self.stage {
            0 if self.registrar.phase().unwrap() == WindowPhase::Ready => {
                let stamp = self.registrar.stamp().unwrap();
                self.stamp = Some(stamp);
                // Partial creation keeps its real OS resource in the reserved slot until cleanup.
                let partial = parent::state(Mode::Complete);
                let captured = partial.clone();
                let failure = self
                    .registrar
                    .register(stamp, |parent, lease| {
                        let child = Child::create(parent, lease, captured)?;
                        Err(ChildCreationFailure {
                            error: io::Error::other("injected failure after OS creation"),
                            partial: Some(child),
                        })
                    })
                    .unwrap_err();
                assert_eq!(self.children.report().unwrap().remaining, 1);
                let retained = partial.borrow().original.unwrap();
                assert!(parent::is_window(retained));
                self.children
                    .release_child(&failure.cleanup.unwrap())
                    .unwrap();
                assert!(!parent::is_window(retained));
                assert_eq!(partial.borrow().starts, 1);
                let suspended = parent::state(Mode::Complete);
                let captured = suspended.clone();
                self.old_lease = Some(
                    self.registrar
                        .register(stamp, |parent, lease| {
                            Child::create(parent, lease, captured)
                        })
                        .unwrap(),
                );
                self.driver.as_mut().unwrap().destroy_surfaces(event_loop);
                assert_eq!(self.children.phase().unwrap(), WindowPhase::Suspended);
                assert_eq!(self.host_closes.get(), 0);
                assert_eq!(
                    self.runtime.borrow().document().lifecycle(),
                    HostLifecycle::Open
                );
                assert!(!parent::is_window(suspended.borrow().original.unwrap()));
                // Explicit renderer lifecycle only; this is NOT a real OS/system-suspend test.
                self.driver
                    .as_mut()
                    .unwrap()
                    .can_create_surfaces(event_loop);
                self.stage = 1;
            }
            1 if self.registrar.phase().unwrap() == WindowPhase::Ready => {
                let before = self.stamp.unwrap();
                let stamp = self.registrar.stamp().unwrap();
                assert_eq!(stamp.epoch(), before.epoch());
                assert!(stamp.surface_generation() > before.surface_generation());
                assert!(
                    self.children
                        .validate_access(self.old_lease.as_ref().unwrap())
                        .is_err()
                );
                self.stamp = Some(stamp);
                for state in [self.pending.clone(), self.failing.clone()] {
                    self.registrar
                        .register(stamp, |parent, lease| Child::create(parent, lease, state))
                        .unwrap();
                }
                let called = Cell::new(false);
                let rejected = self
                    .registrar
                    .register(stamp, |_, _| {
                        called.set(true);
                        panic!("capacity must reserve before OS creation")
                    })
                    .unwrap_err();
                assert!(matches!(
                    rejected.problem,
                    RegistrationProblem::Lifecycle(LifecycleError::Capacity)
                ));
                assert!(!called.get());
                self.geometry = self.registrar.geometry().unwrap().unwrap().revision;
                parent::resize_parent(&self.pending);
                self.stage = 2;
            }
            2 if self
                .registrar
                .geometry()
                .unwrap()
                .is_some_and(|geometry| geometry.revision > self.geometry) =>
            {
                assert_eq!(self.registrar.stamp().unwrap(), self.stamp.unwrap());
                parent::match_child_bounds(&self.pending);
                self.driver
                    .as_ref()
                    .unwrap()
                    .wake_handle()
                    .request_close()
                    .unwrap();
                self.stage = 3;
            }
            3 if self.runtime.borrow().document().lifecycle() == HostLifecycle::CleanupFailed => {
                assert_eq!(self.pending.borrow().starts, 1);
                assert_eq!(self.failing.borrow().starts, 1);
                assert_eq!(self.host_closes.get(), 0);
                assert!(parent::is_window(self.pending.borrow().parent.unwrap()));
                assert!(
                    self.pending
                        .borrow()
                        .lease
                        .as_ref()
                        .unwrap()
                        .with_parent(|_| ())
                        .is_err()
                );
                self.driver
                    .as_ref()
                    .unwrap()
                    .wake_handle()
                    .request_close()
                    .unwrap(); // explicit failed-attempt retry
                self.stage = 4;
            }
            4 if self.runtime.borrow().document().lifecycle() == HostLifecycle::Closing
                && self.failing.borrow().starts == 2 =>
            {
                assert_eq!(self.pending.borrow().starts, 1); // in-flight cleanup was not restarted
                assert_eq!(self.host_closes.get(), 0);
                assert!(!parent::is_window(self.failing.borrow().original.unwrap()));
                let signal = {
                    let mut state = self.pending.borrow_mut();
                    state.terminal = true;
                    state.signal.clone().unwrap()
                };
                // Real cross-thread terminal notification; only CompletionSignal crosses the boundary.
                std::thread::spawn(move || {
                    assert!(signal.notify());
                    assert!(!signal.notify());
                })
                .join()
                .unwrap();
                self.stage = 5;
            }
            5 if self.registrar.phase().unwrap() == WindowPhase::Closed => {
                assert_eq!(self.host_closes.get(), 1);
                assert_eq!(self.pending.borrow().starts, 1);
                assert_eq!(self.pending.borrow().observations, 1);
                assert_eq!(self.failing.borrow().starts, 2);
                assert_eq!(
                    self.runtime.borrow().document().lifecycle(),
                    HostLifecycle::Closed
                );
                assert!(!parent::is_window(self.pending.borrow().original.unwrap()));
                assert!(!parent::is_window(self.pending.borrow().parent.unwrap()));
                assert!(!self.pending.borrow().signal.as_ref().unwrap().notify());
                assert!(
                    self.pending
                        .borrow()
                        .lease
                        .as_ref()
                        .unwrap()
                        .with_parent(|_| ())
                        .is_err()
                );
                let diagnostics = self.driver.as_ref().unwrap().presentation_errors();
                assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
                assert!(
                    diagnostics[0].contains("native child cleanup: RetryableFailure"),
                    "{diagnostics:?}"
                );
                self.completed.set(true);
                event_loop.exit();
            }
            _ => (),
        }
        // Preserve any earlier driver cleanup deadline; no Poll/busy loop.
        match event_loop.control_flow() {
            ControlFlow::WaitUntil(earlier) if earlier < self.deadline => (),
            _ => event_loop.set_control_flow(ControlFlow::WaitUntil(self.deadline)),
        }
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.dispatch(event_loop, |probe| {
            probe
                .driver
                .as_mut()
                .unwrap()
                .can_create_surfaces(event_loop)
        });
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.dispatch(event_loop, |probe| {
            probe.driver.as_mut().unwrap().destroy_surfaces(event_loop)
        });
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.dispatch(event_loop, |probe| {
            probe
                .driver
                .as_mut()
                .unwrap()
                .window_event(event_loop, id, event)
        });
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.dispatch(event_loop, |probe| {
            probe.driver.as_mut().unwrap().proxy_wake_up(event_loop)
        });
    }
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.dispatch(event_loop, |probe| {
            probe.driver.as_mut().unwrap().about_to_wait(event_loop);
            probe.advance(event_loop);
        });
    }
}

#[test]
#[ignore = "explicit Windows OS-window slot required; not a headless or system-suspend gate"]
fn actual_parent_partial_creation_resize_pending_retry_and_child_first_close() {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    preparation::check(&event_loop);
    let document = document();
    let prepared = NativeWindowPreparation::new(
        &event_loop,
        document.clone(),
        LifecycleLimits::new(2, Duration::from_secs(3)).unwrap(),
        WindowOptions::default(),
        None,
    )
    .unwrap();
    let children = prepared.children();
    let registrar = prepared.registrar();
    let host_closes = Rc::new(Cell::new(0));
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
        document,
        ChildWindowHost::new(
            Host(host_closes.clone(), Rc::new(Cell::new(0))),
            children.clone(),
        ),
    )));
    let driver = match prepared.attach(runtime.clone()) {
        Ok(driver) => driver,
        Err(error) => panic!("{error:?}"),
    };
    let completed = Rc::new(Cell::new(false));
    let failure = Rc::new(RefCell::new(None));
    event_loop
        .run_app(Probe {
            driver: Some(driver),
            runtime,
            children,
            registrar,
            host_closes,
            pending: parent::state(Mode::Pending),
            failing: parent::state(Mode::FailOnce),
            old_lease: None,
            stamp: None,
            geometry: 0,
            stage: 0,
            deadline: Instant::now() + Duration::from_secs(15),
            completed: completed.clone(),
            failure: failure.clone(),
        })
        .unwrap();
    assert!(
        failure.borrow().is_none(),
        "native probe: {:?}",
        failure.borrow()
    );
    assert!(
        completed.get(),
        "native probe exited without normal cleanup evidence"
    );
}
