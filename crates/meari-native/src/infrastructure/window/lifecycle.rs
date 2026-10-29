//! UI-thread lifecycle adapter. Observer receipts never acknowledge native child cleanup.
use super::{parent::*, preparation::NativeClock};
use crate::application::window_lifecycle::*;
use meari::domain::host::HostEnvironment;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};
use winit::window::Window;

pub(super) trait WindowLifecyclePort {
    fn submitted(&self, frame: NativeFrameObservation) -> Result<(), String>;
    fn created(&self, window: Arc<dyn Window>) -> Result<(), String>;
    fn renderer_ready(&self) -> Result<(), String>;
    fn geometry(&self, environment: HostEnvironment) -> Result<(), String>;
    fn suspend(&self) -> Result<(), String>;
    fn poll(&self) -> Result<(), String>;
    fn next_deadline(&self) -> Result<Option<Instant>, String>;
    fn forced(&self) -> Result<(), String>;
    fn closing(&self) -> Result<(), String>;
    fn failed(&self) -> Result<(), String>;
    fn removed(&self, forced: bool) -> Result<(), String>;
    fn abandon(&self);
}
pub(super) struct Lifecycle<C: NativeChild> {
    pub binding: Rc<ParentBinding>,
    pub children: Rc<WindowLifecycle<C>>,
    pub clock: NativeClock,
    renderer_ready: Cell<bool>,
    closing_notified: Cell<bool>,
    failed: Cell<bool>,
    observation_unavailable: Cell<usize>,
}
impl<C: NativeChild> Lifecycle<C> {
    pub fn new(
        binding: Rc<ParentBinding>,
        children: Rc<WindowLifecycle<C>>,
        clock: NativeClock,
    ) -> Self {
        Self {
            binding,
            children,
            clock,
            renderer_ready: Cell::new(false),
            closing_notified: Cell::new(false),
            failed: Cell::new(false),
            observation_unavailable: Cell::new(0),
        }
    }
    fn notice(&self, kind: NativeWindowNoticeKind) -> Result<(), String> {
        self.binding
            .publish(
                kind,
                Some(self.children.stamp().map_err(|error| error.to_string())?),
            )
            .map_err(|error| error.to_string())
    }
    fn ready_if_possible(&self) -> Result<(), String> {
        if !self.renderer_ready.get() {
            return Ok(());
        }
        let kind = match self.children.phase().map_err(|error| error.to_string())? {
            WindowPhase::NotReady => ReadyKind::NewWindow,
            WindowPhase::Suspended => ReadyKind::SameWindow,
            _ => return Ok(()),
        };
        let stamp = self
            .children
            .ready(kind)
            .map_err(|error| error.to_string())?;
        self.binding
            .mark_ready(stamp)
            .map_err(|error| error.to_string())?;
        self.notice(NativeWindowNoticeKind::Ready)
    }
}
impl<C: NativeChild> WindowLifecyclePort for Lifecycle<C> {
    fn submitted(&self, frame: NativeFrameObservation) -> Result<(), String> {
        self.binding
            .submitted(
                frame,
                Some(self.children.stamp().map_err(|error| error.to_string())?),
            )
            .map_err(|error| error.to_string())
    }
    fn created(&self, window: Arc<dyn Window>) -> Result<(), String> {
        self.binding
            .created(window)
            .map_err(|error| error.to_string())?;
        self.notice(NativeWindowNoticeKind::Created)
    }
    fn renderer_ready(&self) -> Result<(), String> {
        if self.failed.get() {
            return Ok(());
        }
        self.renderer_ready.set(true);
        self.ready_if_possible()
    }
    fn geometry(&self, environment: HostEnvironment) -> Result<(), String> {
        if self.binding.observe_geometry(environment)? {
            self.notice(NativeWindowNoticeKind::GeometryChanged)?;
        }
        Ok(())
    }
    fn suspend(&self) -> Result<(), String> {
        self.renderer_ready.set(false);
        self.binding.pause();
        // Notify only after revocation has happened, without an internal coordinator borrow.
        if self.children.phase().map_err(|error| error.to_string())? != WindowPhase::Ready {
            return Ok(());
        }
        self.children.suspend().map_err(|error| error.to_string())?;
        self.notice(
            if self.children.phase().map_err(|error| error.to_string())? == WindowPhase::Suspended {
                NativeWindowNoticeKind::Suspended
            } else {
                NativeWindowNoticeKind::Suspending
            },
        )
    }
    fn poll(&self) -> Result<(), String> {
        self.binding
            .seed
            .take(self.binding.host)
            .map_err(|error| error.to_string())?;
        let before = self.children.phase().map_err(|error| error.to_string())?;
        let progress = self.children.poll().map_err(|error| error.to_string())?;
        let notice = if before == WindowPhase::Suspending
            && self.children.phase().map_err(|error| error.to_string())? == WindowPhase::Suspended
        {
            self.notice(NativeWindowNoticeKind::Suspended)
        } else {
            Ok(())
        };
        let ready = self.ready_if_possible();
        let flush = match self.binding.flush_notices() {
            // Failed metadata observers require explicit retry, not automatic replay or loss of readiness.
            Ok(()) | Err(NativeNoticeError::Paused) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        notice.and(ready).and(flush)?;
        let previous = self.observation_unavailable.replace(progress.unavailable);
        if progress.unavailable != 0 && progress.unavailable != previous {
            Err(format!(
                "native child cleanup observation unavailable: {} retained",
                progress.unavailable
            ))
        } else {
            Ok(())
        }
    }
    fn next_deadline(&self) -> Result<Option<Instant>, String> {
        self.children
            .next_deadline()
            .map_err(|error| error.to_string())?
            .map(|deadline| {
                self.clock
                    .origin
                    .checked_add(deadline)
                    .ok_or_else(|| "native deadline overflow".to_owned())
            })
            .transpose()
    }
    fn forced(&self) -> Result<(), String> {
        // Invalidate OS access even if child/host checkpoint validation later fails.
        self.binding.revoke();
        self.renderer_ready.set(false);
        self.children
            .force_loss()
            .map_err(|error| error.to_string())?;
        self.notice(NativeWindowNoticeKind::ForcedIncomplete)
    }
    fn closing(&self) -> Result<(), String> {
        self.binding.pause();
        self.renderer_ready.set(false);
        if !self.closing_notified.replace(true) {
            self.notice(NativeWindowNoticeKind::Revoking)?;
        }
        Ok(())
    }
    fn failed(&self) -> Result<(), String> {
        self.failed.set(true);
        self.renderer_ready.set(false);
        self.binding.pause();
        self.notice(NativeWindowNoticeKind::Failed)
    }
    fn removed(&self, forced: bool) -> Result<(), String> {
        self.binding.revoke();
        self.renderer_ready.set(false);
        let notice = if forced {
            self.children
                .abandon_window()
                .map_err(|error| error.to_string())?;
            self.notice(NativeWindowNoticeKind::ForcedIncomplete)
        } else {
            if let Err(error) = self.children.window_removed() {
                // Physical resources are already gone. A broken barrier cannot remain Ready/Closing.
                let _ = self.children.abandon_window();
                self.binding.discard_observer();
                return Err(error.to_string());
            }
            self.notice(NativeWindowNoticeKind::Closed)
        };
        self.binding.discard_observer();
        notice
    }
    fn abandon(&self) {
        self.binding.revoke();
        self.binding.discard_observer();
        self.renderer_ready.set(false);
        let _ = self.children.abandon_window();
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::super::forwarding::ForwardingSeed;
    use super::*;
    use std::{task::Waker, time::Duration};
    struct Child(Rc<Cell<usize>>);
    impl NativeChild for Child {
        type Error = std::io::Error;
        fn parent_lost(&mut self) {
            self.0.set(self.0.get() + 1);
        }
        fn begin_cleanup(
            &mut self,
            _: CleanupRequest,
            _: CompletionSignal,
        ) -> Result<CleanupProgress, Self::Error> {
            self.0.set(self.0.get() + 1);
            Ok(CleanupProgress::Complete)
        }
        fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
            panic!("no pending work")
        }
    }
    fn fixture() -> (Lifecycle<Child>, Rc<Cell<usize>>) {
        let seed = ForwardingSeed::new(31);
        let clock = NativeClock {
            origin: Instant::now(),
        };
        let children = Rc::new(
            WindowLifecycle::new(
                LifecycleLimits::new(2, Duration::from_secs(1)).unwrap(),
                clock,
                Waker::from(seed.clone()),
            )
            .unwrap(),
        );
        (
            Lifecycle::new(ParentBinding::new(31, seed, None), children, clock),
            Rc::new(Cell::new(0)),
        )
    }
    #[test]
    fn abandon_revokes_without_backend_or_observer_callbacks_and_retains_residual_owner() {
        let (lifecycle, calls) = fixture();
        let stamp = lifecycle.children.ready(ReadyKind::NewWindow).unwrap();
        let lease = lifecycle
            .children
            .register(stamp, |_| Ok(Child(calls.clone())))
            .unwrap();
        lifecycle.abandon();
        assert_eq!(calls.get(), 0);
        assert_eq!(
            lifecycle.children.phase().unwrap(),
            WindowPhase::ForcedIncomplete
        );
        assert_eq!(
            lifecycle.children.validate_access(&lease),
            Err(LifecycleError::Closed)
        );
        assert_eq!(lifecycle.children.report().unwrap().remaining, 1);
        lifecycle.children.force_loss().unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(lifecycle.children.report().unwrap().remaining, 0);
        assert_eq!(
            lifecycle.children.phase().unwrap(),
            WindowPhase::ForcedIncomplete
        );
    }
    #[test]
    fn normal_close_before_actual_creation_can_finish_but_failure_is_never_ready() {
        let (lifecycle, calls) = fixture();
        lifecycle.failed().unwrap();
        lifecycle.renderer_ready().unwrap();
        assert_eq!(lifecycle.children.phase().unwrap(), WindowPhase::NotReady);
        lifecycle.children.begin_close().unwrap();
        lifecycle.removed(false).unwrap();
        assert_eq!(lifecycle.children.phase().unwrap(), WindowPhase::Closed);
        assert_eq!(calls.get(), 0);
        let notices = lifecycle.binding.take_notices().unwrap();
        assert!(
            notices
                .iter()
                .any(|notice| notice.kind == NativeWindowNoticeKind::Failed)
        );
        assert!(
            notices
                .iter()
                .any(|notice| notice.kind == NativeWindowNoticeKind::Closed)
        );
        assert!(
            !notices
                .iter()
                .any(|notice| notice.kind == NativeWindowNoticeKind::Ready)
        );
    }
    #[test]
    fn retained_early_terminal_and_duplicate_seed_flush_do_not_restart_cleanup() {
        use std::{
            sync::atomic::{AtomicUsize, Ordering},
            task::Wake,
        };
        struct Count(AtomicUsize);
        impl Wake for Count {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        struct Early(Rc<Cell<usize>>);
        impl NativeChild for Early {
            type Error = std::io::Error;
            fn parent_lost(&mut self) {
                panic!("not a forced-loss scenario")
            }
            fn begin_cleanup(
                &mut self,
                _: CleanupRequest,
                signal: CompletionSignal,
            ) -> Result<CleanupProgress, Self::Error> {
                self.0.set(self.0.get() + 1);
                assert!(signal.notify());
                assert!(!signal.notify());
                Ok(CleanupProgress::Pending)
            }
            fn observe_cleanup(
                &mut self,
                _: CleanupRequest,
            ) -> Result<CleanupProgress, Self::Error> {
                self.0.set(self.0.get() + 10);
                Ok(CleanupProgress::Complete)
            }
        }
        let seed = ForwardingSeed::new(32);
        let count = Arc::new(Count(AtomicUsize::new(0)));
        seed.bind_once(32, Waker::from(count.clone())).unwrap();
        let clock = NativeClock {
            origin: Instant::now(),
        };
        let children = Rc::new(
            WindowLifecycle::new(
                LifecycleLimits::new(1, Duration::from_secs(1)).unwrap(),
                clock,
                Waker::from(seed.clone()),
            )
            .unwrap(),
        );
        let stamp = children.ready(ReadyKind::NewWindow).unwrap();
        let calls = Rc::new(Cell::new(0));
        children
            .register(stamp, |_| Ok(Early(calls.clone())))
            .unwrap();
        let lifecycle = Lifecycle::new(ParentBinding::new(32, seed, None), children, clock);
        lifecycle.suspend().unwrap();
        assert_eq!(calls.get(), 11);
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        lifecycle.poll().unwrap();
        lifecycle.poll().unwrap();
        assert_eq!(calls.get(), 11);
        assert_eq!(lifecycle.children.phase().unwrap(), WindowPhase::Suspended);
        assert!(lifecycle.next_deadline().unwrap().is_none());
    }
}
