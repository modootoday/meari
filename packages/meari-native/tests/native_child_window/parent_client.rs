//! Allocated OS gate only. This is not a DPI, focus or compositor proof.
use super::*;
use windows::Win32::{Foundation::RECT, Graphics::Gdi::InvalidateRect, UI::WindowsAndMessaging::*};

type Runtime = Rc<RefCell<NativeHostRuntime<ChildWindowHost<Host, parent::Child>>>>;
struct Probe {
    driver: Option<NativeWindowDriver>,
    runtime: Runtime,
    children: Rc<WindowLifecycle<parent::Child>>,
    registrar: NativeChildRegistrar<parent::Child>,
    surfaces: NativeSurfaceRegistrar<parent::Child>,
    video: String,
    slot: Option<NativeSurfaceSlot>,
    state: Rc<RefCell<parent::State>>,
    stage: u8,
    deadline: Instant,
    failure: Rc<RefCell<Option<String>>>,
    completed: Rc<Cell<bool>>,
}
impl Probe {
    fn dispatch(&mut self, event_loop: &dyn ActiveEventLoop, run: impl FnOnce(&mut Self)) {
        if self.driver.is_none() {
            return;
        }
        if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(self))) {
            let message = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|v| (*v).into()))
                .unwrap_or_else(|| "parent client probe panicked".into());
            *self.failure.borrow_mut() = Some(message);
            drop(self.driver.take());
            event_loop.exit();
        }
    }
    fn advance(&mut self, event_loop: &dyn ActiveEventLoop) {
        assert!(
            Instant::now() < self.deadline,
            "parent client stage {} timed out",
            self.stage
        );
        if self.stage == 0 && self.registrar.phase().unwrap() == WindowPhase::Ready {
            let doc = self.runtime.borrow().document().clone();
            let reservation = self
                .surfaces
                .reserve_video(&doc.stamp().unwrap(), &self.video)
                .unwrap();
            let state = self.state.clone();
            let mut slot = None;
            self.surfaces
                .register_reserved(reservation, |parent, lease, surface| {
                    let child = parent::Child::create(parent, lease, state)?;
                    slot = Some(surface);
                    Ok(child)
                })
                .unwrap();
            self.slot = slot;
            // Registration invalidates the prior trace: request a real subsequent window paint.
            assert!(
                unsafe { InvalidateRect(Some(self.state.borrow().parent.unwrap()), None, true) }
                    .as_bool()
            );
            self.stage = 1;
        }
        if self.stage == 1 {
            let slot = self.slot.as_ref().unwrap();
            let observed = slot.with_parent_client(|checked| {
                checked
                    .with_parent(|_, snapshot| {
                        assert_eq!(
                            snapshot.focus,
                            meari_native::domain::surface_slot::SurfaceFocus::Unknown
                        );
                        assert!(snapshot.physical_client_rect.unwrap().width > 0.0);
                        assert!(snapshot.require_rectangular_child_window().is_err());
                    })
                    .unwrap();
                let hwnd = self.state.borrow().parent.unwrap();
                let mut rect = RECT::default();
                unsafe { GetWindowRect(hwnd, &mut rect) }.unwrap();
                unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        rect.left + 19,
                        rect.top + 23,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                }
                .unwrap();
                let called = Cell::new(false);
                assert!(matches!(
                    checked.with_parent(|_, _| called.set(true)),
                    Err(NativeParentClientError::PlacementMismatch)
                ));
                assert!(!called.get());
            });
            match observed {
                Ok(()) => self.stage = 2,
                Err(NativeParentClientError::Access(NativeSurfaceAccessError::Slot(
                    meari_native::domain::surface_slot::SurfaceError::NotReady,
                ))) => (),
                other => panic!("unexpected client observation: {other:?}"),
            }
        } else if self.stage == 2 {
            self.slot.as_ref().unwrap().revoke();
            let called = Cell::new(false);
            assert!(
                self.slot
                    .as_ref()
                    .unwrap()
                    .with_parent_client(|_| called.set(true))
                    .is_err()
            );
            assert!(!called.get());
            self.driver
                .as_ref()
                .unwrap()
                .wake_handle()
                .request_close()
                .unwrap();
            self.stage = 3;
        } else if self.stage == 3 && self.registrar.phase().unwrap() == WindowPhase::Closed {
            assert_eq!(self.children.report().unwrap().remaining, 0);
            self.completed.set(true);
            event_loop.exit();
        }
        let tick = (Instant::now() + Duration::from_millis(20)).min(self.deadline);
        match event_loop.control_flow() {
            ControlFlow::WaitUntil(earlier) if earlier < tick => (),
            _ => event_loop.set_control_flow(ControlFlow::WaitUntil(tick)),
        }
    }
}
impl ApplicationHandler for Probe {
    fn can_create_surfaces(&mut self, e: &dyn ActiveEventLoop) {
        self.dispatch(e, |s| s.driver.as_mut().unwrap().can_create_surfaces(e));
    }
    fn destroy_surfaces(&mut self, e: &dyn ActiveEventLoop) {
        self.dispatch(e, |s| s.driver.as_mut().unwrap().destroy_surfaces(e));
    }
    fn window_event(&mut self, e: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.dispatch(e, |s| s.driver.as_mut().unwrap().window_event(e, id, event));
    }
    fn proxy_wake_up(&mut self, e: &dyn ActiveEventLoop) {
        self.dispatch(e, |s| s.driver.as_mut().unwrap().proxy_wake_up(e));
    }
    fn about_to_wait(&mut self, e: &dyn ActiveEventLoop) {
        self.dispatch(e, |s| {
            s.driver.as_mut().unwrap().about_to_wait(e);
            s.advance(e);
        });
    }
}

#[test]
#[ignore = "requires allocated Windows window slot; no DPI or focus claim"]
fn original_frame_parent_move_and_revoked_slot_reject_stale_client_use() {
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <video />;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "video{display:block;width:120px;height:60px}",
        Viewport {
            width: 360,
            height: 240,
            dark: false,
        },
    )
    .unwrap();
    let video = document
        .elements()
        .iter()
        .find(|e| e.dom_tag == "video")
        .unwrap()
        .id
        .clone();
    let document = NativeDocumentHandle::new(document, 1).unwrap();
    let prepared = NativeWindowPreparation::<parent::Child>::new(
        &event_loop,
        document.clone(),
        LifecycleLimits::new(2, Duration::from_secs(3)).unwrap(),
        WindowOptions::default(),
        None,
    )
    .unwrap();
    let children = prepared.children();
    let registrar = prepared.registrar();
    let surfaces = prepared.surface_slots();
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
        document,
        ChildWindowHost::new(
            Host(Rc::new(Cell::new(0)), Rc::new(Cell::new(0))),
            children.clone(),
        ),
    )));
    let driver = prepared.attach(runtime.clone()).unwrap();
    let completed = Rc::new(Cell::new(false));
    let failure = Rc::new(RefCell::new(None));
    event_loop
        .run_app(Probe {
            driver: Some(driver),
            runtime,
            children,
            registrar,
            surfaces,
            video,
            slot: None,
            state: parent::state(parent::Mode::Complete),
            stage: 0,
            deadline: Instant::now() + Duration::from_secs(15),
            failure: failure.clone(),
            completed: completed.clone(),
        })
        .unwrap();
    assert!(failure.borrow().is_none(), "{:?}", failure.borrow());
    assert!(completed.get());
}
