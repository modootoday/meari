//! Scoped OS access. The actual window Arc never leaves this infrastructure owner.
#[cfg(feature = "test-support")]
pub mod test_support;
use super::forwarding::ForwardingSeed;
use crate::application::window_lifecycle::*;
use meari::domain::host::HostEnvironment;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::{Rc, Weak},
    sync::Arc,
};
use winit::{
    raw_window_handle::{HandleError, HasWindowHandle, WindowHandle},
    window::Window,
};

#[derive(Clone, Debug)]
pub struct NativeWindowGeometry {
    pub revision: u64,
    pub environment: HostEnvironment,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeWindowNoticeKind {
    Created,
    Ready,
    GeometryChanged,
    Suspending,
    Suspended,
    Revoking,
    /// Host/lease ownership was removed. This is not a physical OS-window destruction receipt.
    Closed,
    ForcedIncomplete,
    Failed,
    FrameSubmitted,
}
/// Render-input identity paired with a checked CPU submission, never an OS compositor ACK.
#[derive(Clone, Debug)]
pub struct NativeFrameObservation {
    pub document: crate::infrastructure::native_host::HostStamp,
    pub submission: super::NativeFrameSubmission,
    /// Only the checked submission path can issue this; synthetic notices use None.
    pub retained: Option<crate::infrastructure::native_host::RetainedFrameProof>,
}
#[derive(Clone, Debug)]
pub struct NativeWindowNotice {
    pub revision: u64,
    pub host_id: u64,
    pub kind: NativeWindowNoticeKind,
    pub window: Option<WindowStamp>,
    pub geometry: Option<NativeWindowGeometry>,
    pub frame: Option<NativeFrameObservation>,
}
pub type NativeWindowObserver = Box<dyn FnMut(NativeWindowNotice) -> Result<(), String>>;

#[derive(Debug)]
pub enum NativeNoticeError {
    Busy,
    Paused,
    Revoked,
    CounterExhausted,
    Callback(String),
    Panicked,
}
impl fmt::Display for NativeNoticeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native window notice: {self:?}")
    }
}
impl std::error::Error for NativeNoticeError {}
struct FlushGuard<'a>(&'a Cell<bool>);
impl Drop for FlushGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct ParentState {
    window: Option<Arc<dyn Window>>,
    epoch: u64,
    ready: bool,
    dead: bool,
    geometry: Option<NativeWindowGeometry>,
    geometry_bytes: Vec<u8>,
    notice_sequence: u64,
    notices: VecDeque<NativeWindowNotice>,
    frame: Option<NativeFrameObservation>,
}
pub(super) struct ParentBinding {
    pub(super) surfaces: Rc<RefCell<crate::application::surface_slot::SurfaceRegistry>>,
    pub host: u64,
    pub seed: Arc<ForwardingSeed>,
    state: RefCell<ParentState>,
    observer: RefCell<Option<NativeWindowObserver>>,
    flushing: Cell<bool>,
    observer_failed: Cell<bool>,
    observer_discarded: Cell<bool>,
}
impl ParentBinding {
    pub fn new(
        host: u64,
        seed: Arc<ForwardingSeed>,
        observer: Option<NativeWindowObserver>,
    ) -> Rc<Self> {
        Rc::new(Self {
            surfaces: Rc::new(RefCell::new(
                crate::application::surface_slot::SurfaceRegistry::default(),
            )),
            host,
            seed,
            state: RefCell::new(ParentState {
                window: None,
                epoch: 0,
                ready: false,
                dead: false,
                geometry: None,
                geometry_bytes: Vec::new(),
                notice_sequence: 0,
                notices: VecDeque::new(),
                frame: None,
            }),
            observer: RefCell::new(observer),
            flushing: Cell::new(false),
            observer_failed: Cell::new(false),
            observer_discarded: Cell::new(false),
        })
    }
    pub fn created(&self, window: Arc<dyn Window>) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_some() {
            return Err(LifecycleError::WrongState);
        }
        state.window = Some(window);
        Ok(())
    }
    pub fn mark_ready(&self, stamp: WindowStamp) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_none() {
            return Err(LifecycleError::NotReady);
        }
        state.epoch = stamp.epoch();
        state.ready = true;
        Ok(())
    }
    pub fn pause(&self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.ready = false;
        }
    }
    pub fn observe_geometry(&self, environment: HostEnvironment) -> Result<bool, String> {
        environment.validate()?;
        let bytes = serde_json::to_vec(&environment).map_err(|error| error.to_string())?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| "native geometry is borrowed")?;
        if state.dead {
            return Err("native parent is revoked".into());
        }
        if state.geometry_bytes == bytes {
            return Ok(false);
        }
        let revision = state.geometry.as_ref().map_or(Ok(1), |last| {
            last.revision
                .checked_add(1)
                .ok_or("geometry revision exhausted")
        })?;
        state.geometry = Some(NativeWindowGeometry {
            revision,
            environment,
        });
        state.geometry_bytes = bytes;
        Ok(true)
    }
    pub fn geometry(&self) -> Result<Option<NativeWindowGeometry>, LifecycleError> {
        Ok(self
            .state
            .try_borrow()
            .map_err(|_| LifecycleError::Busy)?
            .geometry
            .clone())
    }
    pub fn window(&self) -> Result<Arc<dyn Window>, LifecycleError> {
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        state.window.clone().ok_or(LifecycleError::NotReady)
    }
    fn check(&self, epoch: u64, ready: bool) -> Result<(), LifecycleError> {
        self.seed.require_bound(self.host)?;
        let state = self.state.try_borrow().map_err(|_| LifecycleError::Busy)?;
        if state.dead {
            return Err(LifecycleError::Closed);
        }
        if state.window.is_none() {
            return Err(LifecycleError::NotReady);
        }
        if state.epoch != epoch {
            return Err(LifecycleError::Stale);
        }
        if ready && !state.ready {
            return Err(LifecycleError::Suspended);
        }
        Ok(())
    }
    pub fn publish(
        &self,
        kind: NativeWindowNoticeKind,
        window: Option<WindowStamp>,
    ) -> Result<(), NativeNoticeError> {
        {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| NativeNoticeError::Busy)?;
            if state.dead
                && !matches!(
                    kind,
                    NativeWindowNoticeKind::Closed
                        | NativeWindowNoticeKind::ForcedIncomplete
                        | NativeWindowNoticeKind::Failed
                )
            {
                return Err(NativeNoticeError::Revoked);
            }
            let revision = state
                .notice_sequence
                .checked_add(1)
                .ok_or(NativeNoticeError::CounterExhausted)?;
            state.notice_sequence = revision;
            // At most one latest notice per enum kind (ten); no unbounded event or payload queue.
            state.notices.retain(|notice| notice.kind != kind);
            let geometry = state.geometry.clone();
            let frame = if kind == NativeWindowNoticeKind::FrameSubmitted {
                state.frame.clone()
            } else {
                None
            };
            state.notices.push_back(NativeWindowNotice {
                revision,
                host_id: self.host,
                kind,
                window,
                geometry,
                frame,
            });
        }
        if self.flushing.get() {
            let _ = self.seed.notify();
            return Err(NativeNoticeError::Busy);
        }
        self.flush_notices()
    }
    pub fn flush_notices(&self) -> Result<(), NativeNoticeError> {
        if self.observer_failed.get() {
            return Err(NativeNoticeError::Paused);
        }
        if self.flushing.replace(true) {
            return Err(NativeNoticeError::Busy);
        }
        let _flush = FlushGuard(&self.flushing);
        let observer = self
            .observer
            .try_borrow_mut()
            .map_err(|_| NativeNoticeError::Busy)?
            .take();
        let Some(mut observer) = observer else {
            return Ok(());
        };
        // One bounded batch only. Reentrant updates remain for the next native wake.
        let batch = self
            .state
            .borrow()
            .notices
            .iter()
            .map(|notice| notice.revision)
            .collect::<Vec<_>>();
        let mut failure = None;
        for revision in batch {
            let notice = self
                .state
                .borrow()
                .notices
                .iter()
                .find(|notice| notice.revision == revision)
                .cloned();
            let Some(notice) = notice else {
                continue;
            };
            match catch_unwind(AssertUnwindSafe(|| observer(notice))) {
                Ok(Ok(())) => self
                    .state
                    .borrow_mut()
                    .notices
                    .retain(|notice| notice.revision != revision),
                Ok(Err(error)) => {
                    failure = Some(NativeNoticeError::Callback(error));
                    break;
                }
                Err(_) => {
                    failure = Some(NativeNoticeError::Panicked);
                    break;
                }
            }
        }
        if !self.observer_discarded.get() {
            *self.observer.borrow_mut() = Some(observer);
        }
        if let Some(error) = failure {
            self.observer_failed.set(true);
            return Err(error);
        }
        Ok(())
    }
    pub fn take_notices(&self) -> Result<Vec<NativeWindowNotice>, NativeNoticeError> {
        if self.flushing.get() {
            return Err(NativeNoticeError::Busy);
        }
        Ok(self
            .state
            .try_borrow_mut()
            .map_err(|_| NativeNoticeError::Busy)?
            .notices
            .drain(..)
            .collect())
    }
    pub fn retry_observer(&self) -> Result<(), NativeNoticeError> {
        if self.observer_discarded.get() {
            return Err(NativeNoticeError::Revoked);
        }
        if self.flushing.get() {
            return Err(NativeNoticeError::Busy);
        }
        self.observer_failed.set(false);
        self.flush_notices()
    }
    /// Revoke immediately, then drop native ownership outside the RefCell guard. Never run observers.
    pub fn revoke(&self) {
        self.seed.close();
        let window = {
            let mut state = self.state.borrow_mut();
            state.dead = true;
            state.ready = false;
            state.frame = None;
            state.window.take()
        };
        drop(window);
    }
    pub fn discard_observer(&self) {
        self.observer_discarded.set(true);
        let observer = self.observer.borrow_mut().take();
        drop(observer);
    }
    pub fn submitted(
        &self,
        frame: NativeFrameObservation,
        window: Option<WindowStamp>,
    ) -> Result<(), NativeNoticeError> {
        {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| NativeNoticeError::Busy)?;
            if state.dead {
                return Err(NativeNoticeError::Revoked);
            }
            state.frame = Some(frame);
        }
        self.publish(NativeWindowNoticeKind::FrameSubmitted, window)
    }
}

/// UI-thread metadata observer handle. It grants no native handle/child registration authority.
#[derive(Clone)]
pub struct NativeWindowObservation {
    pub(super) binding: Rc<ParentBinding>,
}
impl NativeWindowObservation {
    pub fn geometry(&self) -> Result<Option<NativeWindowGeometry>, LifecycleError> {
        self.binding.geometry()
    }
    pub fn take_notices(&self) -> Result<Vec<NativeWindowNotice>, NativeNoticeError> {
        self.binding.take_notices()
    }
    pub fn retry_observer(&self) -> Result<(), NativeNoticeError> {
        self.binding.retry_observer()
    }
}

/// No Clone/raw-owner getter. Only borrowed inside an authorized registration/access callback.
pub struct NativeScopedParent<'a> {
    window: &'a dyn Window,
    validate: &'a dyn Fn() -> Result<(), LifecycleError>,
}
impl HasWindowHandle for NativeScopedParent<'_> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        (self.validate)().map_err(|_| HandleError::Unavailable)?;
        self.window.window_handle()
    }
}
impl NativeScopedParent<'_> {
    pub(super) fn validate_live(&self) -> Result<(), LifecycleError> {
        (self.validate)()
    }
}
pub(super) trait ParentAccess {
    fn visit(
        &self,
        cleanup: Option<CleanupRequest>,
        read: &mut dyn FnMut(&NativeScopedParent<'_>),
    ) -> Result<(), LifecycleError>;
}
/// Derived authority retained by the native child, with weak owners to avoid an ownership cycle.
pub struct NativeParentLease {
    access: Rc<dyn ParentAccess>,
    window: WindowStamp,
    child: u64,
}
impl NativeParentLease {
    pub(super) fn surface_access(&self) -> Rc<dyn ParentAccess> {
        self.access.clone()
    }
    pub fn window(&self) -> WindowStamp {
        self.window
    }
    pub fn child(&self) -> u64 {
        self.child
    }
    pub fn with_parent<R>(
        &self,
        read: impl for<'a> FnOnce(&NativeScopedParent<'a>) -> R,
    ) -> Result<R, LifecycleError> {
        self.visit(None, read)
    }
    pub fn with_cleanup_parent<R>(
        &self,
        request: CleanupRequest,
        read: impl for<'a> FnOnce(&NativeScopedParent<'a>) -> R,
    ) -> Result<R, LifecycleError> {
        self.visit(Some(request), read)
    }
    fn visit<R>(
        &self,
        cleanup: Option<CleanupRequest>,
        read: impl for<'a> FnOnce(&NativeScopedParent<'a>) -> R,
    ) -> Result<R, LifecycleError> {
        let mut read = Some(read);
        let mut result = None;
        self.access.visit(cleanup, &mut |parent| {
            result = Some(read.take().expect("one parent visit")(parent));
        })?;
        result.ok_or(LifecycleError::WrongState)
    }
}
struct ChildParentAccess<C: NativeChild> {
    binding: Weak<ParentBinding>,
    coordinator: Weak<WindowLifecycle<C>>,
    authority: ChildLease,
}
impl<C: NativeChild> ParentAccess for ChildParentAccess<C> {
    fn visit(
        &self,
        cleanup: Option<CleanupRequest>,
        read: &mut dyn FnMut(&NativeScopedParent<'_>),
    ) -> Result<(), LifecycleError> {
        let binding = self.binding.upgrade().ok_or(LifecycleError::Closed)?;
        let coordinator = self.coordinator.upgrade().ok_or(LifecycleError::Closed)?;
        let validate = || {
            if let Some(request) = cleanup {
                coordinator.validate_parent_cleanup(&self.authority, request)?;
            } else {
                coordinator.validate_access(&self.authority)?;
            }
            binding.check(self.authority.window().epoch(), cleanup.is_none())
        };
        validate()?;
        let window = binding.window()?;
        read(&NativeScopedParent {
            window: window.as_ref(),
            validate: &validate,
        });
        Ok(())
    }
}

pub struct NativeChildRegistrar<C: NativeChild> {
    pub(super) binding: Rc<ParentBinding>,
    pub(super) coordinator: Rc<WindowLifecycle<C>>,
}
impl<C: NativeChild> Clone for NativeChildRegistrar<C> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            coordinator: self.coordinator.clone(),
        }
    }
}
impl<C: NativeChild> NativeChildRegistrar<C> {
    pub fn phase(&self) -> Result<WindowPhase, LifecycleError> {
        self.coordinator.phase()
    }
    pub fn stamp(&self) -> Result<WindowStamp, LifecycleError> {
        self.coordinator.stamp()
    }
    pub fn geometry(&self) -> Result<Option<NativeWindowGeometry>, LifecycleError> {
        self.binding.geometry()
    }
    /// Notices are observation metadata, never child cleanup acknowledgement.
    pub fn take_notices(&self) -> Result<Vec<NativeWindowNotice>, NativeNoticeError> {
        self.binding.take_notices()
    }
    /// Explicit caller recovery only; failed observers are never replayed automatically.
    pub fn retry_observer(&self) -> Result<(), NativeNoticeError> {
        self.binding.retry_observer()
    }
    pub fn register(
        &self,
        expected: WindowStamp,
        create: impl for<'a> FnOnce(
            &NativeScopedParent<'a>,
            NativeParentLease,
        ) -> Result<C, ChildCreationFailure<C>>,
    ) -> Result<ChildLease, RegistrationFailure<C::Error>> {
        let fail = |error| RegistrationFailure {
            problem: RegistrationProblem::Lifecycle(error),
            cleanup: None,
        };
        self.binding.check(expected.epoch(), true).map_err(fail)?;
        let window = self.binding.window().map_err(fail)?;
        self.coordinator.register(expected, |lease| {
            let access = Rc::new(ChildParentAccess {
                binding: Rc::downgrade(&self.binding),
                coordinator: Rc::downgrade(&self.coordinator),
                authority: lease.for_parent(),
            });
            let parent = NativeParentLease {
                access,
                window: lease.window(),
                child: lease.id(),
            };
            let validate = || {
                self.coordinator.validate_parent_creation(lease)?;
                self.binding.check(lease.window().epoch(), true)
            };
            create(
                &NativeScopedParent {
                    window: window.as_ref(),
                    validate: &validate,
                },
                parent,
            )
        })
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        task::{Wake, Waker},
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn reentrant_notice_is_busy_but_retained_for_the_next_turn() {
        let slot: Rc<RefCell<Option<Weak<ParentBinding>>>> = Rc::new(RefCell::new(None));
        let target = slot.clone();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let out = calls.clone();
        let seed = ForwardingSeed::new(11);
        let wakes = Arc::new(Count(AtomicUsize::new(0)));
        seed.bind_once(11, Waker::from(wakes.clone())).unwrap();
        let binding = ParentBinding::new(
            11,
            seed.clone(),
            Some(Box::new(move |notice| {
                out.borrow_mut().push(notice.kind);
                if notice.kind == NativeWindowNoticeKind::Ready {
                    let binding = target.borrow().as_ref().unwrap().upgrade().unwrap();
                    assert!(matches!(
                        binding.publish(NativeWindowNoticeKind::GeometryChanged, None),
                        Err(NativeNoticeError::Busy)
                    ));
                    assert!(matches!(
                        binding.take_notices(),
                        Err(NativeNoticeError::Busy)
                    ));
                }
                Ok(())
            })),
        );
        *slot.borrow_mut() = Some(Rc::downgrade(&binding));
        binding
            .publish(NativeWindowNoticeKind::Ready, None)
            .unwrap();
        assert_eq!(*calls.borrow(), [NativeWindowNoticeKind::Ready]);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert!(seed.take(11).unwrap());
        binding.flush_notices().unwrap();
        assert_eq!(
            *calls.borrow(),
            [
                NativeWindowNoticeKind::Ready,
                NativeWindowNoticeKind::GeometryChanged
            ]
        );
        assert!(binding.take_notices().unwrap().is_empty());
    }
    #[test]
    fn failed_callback_is_not_replayed_until_explicit_recovery() {
        let calls = Rc::new(Cell::new(0));
        let out = calls.clone();
        let binding = ParentBinding::new(
            12,
            ForwardingSeed::new(12),
            Some(Box::new(move |_| {
                out.set(out.get() + 1);
                if out.get() == 1 {
                    Err("technical observer failure".into())
                } else {
                    Ok(())
                }
            })),
        );
        assert!(matches!(
            binding.publish(NativeWindowNoticeKind::Created, None),
            Err(NativeNoticeError::Callback(_))
        ));
        assert!(matches!(
            binding.flush_notices(),
            Err(NativeNoticeError::Paused)
        ));
        assert_eq!(calls.get(), 1);
        binding.retry_observer().unwrap();
        assert_eq!(calls.get(), 2);
        assert!(binding.take_notices().unwrap().is_empty());
    }
    #[test]
    fn polling_observer_keeps_only_latest_notice_per_kind() {
        let binding = ParentBinding::new(13, ForwardingSeed::new(13), None);
        for _ in 0..100 {
            binding
                .publish(NativeWindowNoticeKind::GeometryChanged, None)
                .unwrap();
        }
        let notices = binding.take_notices().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].revision, 100);
    }
}
