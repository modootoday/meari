//! Private fake loop backend: these tests do not initialize an OS event loop or window.
use super::super::{NativeWindowWake, scheduling::NativeClock};
use super::*;
use crate::{
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*},
};
use blitz_shell::{BlitzApplication, BlitzShellProxy};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
pub(super) struct Count(pub AtomicUsize);
impl winit::event_loop::EventLoopProxyProvider for Count {
    fn wake_up(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
pub(super) fn proxy() -> (
    BlitzShellProxy,
    std::sync::mpsc::Receiver<blitz_shell::BlitzShellEvent>,
    Arc<Count>,
) {
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let (proxy, events) =
        BlitzShellProxy::new(winit::event_loop::EventLoopProxy::new(count.clone()));
    (proxy, events, count)
}
pub(super) fn startup() -> NativeStartup {
    let (proxy, events, _) = proxy();
    NativeStartup {
        authority: Authority::new().unwrap(),
        inner: Some(BlitzApplication::new(proxy.clone(), events)),
        signal: wake::Shared::new(proxy),
        clock: NativeClock::new(),
        document: None,
        handoff_attempted: false,
    }
}
pub(super) fn target() -> (NativeWindowWake, Arc<Count>) {
    let (proxy, _events, count) = proxy();
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
pub(super) fn document() -> NativeDocumentHandle {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>Startup</main>;}",
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
pub(super) struct Host;
impl NativeWindowHost for Host {
    type Error = std::io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        panic!("no callback in fake-loop fixture")
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Self::Error> {
        Ok(HostEffects::default())
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Self::Error> {
        CloseDecision::HostCleanupComplete { checkpoint: () }
    }
}
pub(super) struct Child;
impl crate::application::window_lifecycle::NativeChild for Child {
    type Error = std::io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: crate::application::window_lifecycle::CleanupRequest,
        _: crate::application::window_lifecycle::CompletionSignal,
    ) -> Result<crate::application::window_lifecycle::CleanupProgress, Self::Error> {
        panic!("no child created")
    }
    fn observe_cleanup(
        &mut self,
        _: crate::application::window_lifecycle::CleanupRequest,
    ) -> Result<crate::application::window_lifecycle::CleanupProgress, Self::Error> {
        panic!("no child created")
    }
}
#[test]
fn early_dirty_is_coalesced_flushed_and_native_target_continues_after_bind() {
    let startup = startup();
    let notify = startup.wake_handle();
    assert!(notify.notify().unwrap());
    assert!(!notify.notify().unwrap());
    let (target, count) = target();
    startup.signal.preparing().unwrap();
    startup.signal.bind(target.clone()).unwrap();
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert_eq!(target.take(), super::super::wake::COMPLETE);
    assert!(notify.notify().unwrap());
    assert_eq!(count.0.load(Ordering::SeqCst), 2);
    assert!(matches!(
        startup.signal.bind(target),
        Err(NativeStartupError::Stale)
    ));
    startup.signal.close();
    assert!(matches!(notify.notify(), Err(NativeStartupError::Closed)));
}
#[test]
fn binding_race_never_loses_the_retained_observation() {
    use std::{sync::Barrier, thread};
    let startup = startup();
    let wake = startup.wake_handle();
    let (target, _) = target();
    let barrier = Arc::new(Barrier::new(2));
    let other = barrier.clone();
    let worker = thread::spawn(move || {
        other.wait();
        for _ in 0..64 {
            wake.notify().unwrap();
        }
    });
    startup.signal.preparing().unwrap();
    barrier.wait();
    startup.signal.bind(target.clone()).unwrap();
    worker.join().unwrap();
    assert_eq!(target.take(), super::super::wake::COMPLETE);
}
#[test]
fn failed_second_prepare_and_attach_cannot_replace_the_first_document() {
    use crate::{
        application::{WindowOptions, window_lifecycle::LifecycleLimits},
        infrastructure::native_host::ChildWindowHost,
    };
    let mut startup = startup();
    let first = document();
    let foreign = document();
    let authority = startup.authority.clone();
    let mut context = NativeStartupContext {
        startup: &mut startup,
    };
    let prepared = context
        .prepare::<Child>(
            first.clone(),
            LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
            WindowOptions::default(),
            None,
        )
        .unwrap();
    let children = prepared.children();
    assert!(matches!(
        context.prepare::<Child>(
            foreign.clone(),
            LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
            WindowOptions::default(),
            None
        ),
        Err(NativeStartupError::AlreadyIssued)
    ));
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(foreign.clone(), Host)));
    let failure = match context.attach(runtime.clone(), WindowOptions::default()) {
        Err(error) => error,
        Ok(_) => panic!("second attachment accepted"),
    };
    assert!(Rc::ptr_eq(&failure.runtime, &runtime));
    assert_eq!(
        context.startup.retained_document().unwrap().id(),
        first.id()
    );
    assert!(Rc::ptr_eq(&context.startup.authority, &authority));
    assert_eq!(foreign.lifecycle(), HostLifecycle::Open);
    let owned = Rc::new(RefCell::new(NativeHostRuntime::new(
        first.clone(),
        ChildWindowHost::new(Host, children),
    )));
    let token = context.attach_prepared(prepared, owned.clone()).unwrap();
    let impostor = Rc::new(Authority {
        stamp: authority.stamp,
    });
    let mut token = token;
    assert!(matches!(
        token.run(
            &impostor,
            HostScheduleLimits::new(Duration::from_millis(2), Duration::from_secs(1), 3).unwrap()
        ),
        Err(NativeStartupError::Stale)
    ));
    assert_eq!(first.lifecycle(), HostLifecycle::Open);
    assert!(Rc::ptr_eq(&token.runtime, &owned));
    owned.borrow_mut().request_close(CloseCause::Host).unwrap();
    owned.borrow_mut().finish_headless_close().unwrap();
}

#[cfg(windows)]
mod timer_os;
mod timer_seams;
