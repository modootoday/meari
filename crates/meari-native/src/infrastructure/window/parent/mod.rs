//! Scoped OS access. The actual window Arc never leaves this infrastructure owner.
#[cfg(feature = "test-support")]
pub mod test_support;
use super::forwarding::ForwardingSeed;
use crate::application::window_lifecycle::*;
use meari::domain::host::HostEnvironment;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::{Rc, Weak},
    sync::Arc,
};
use winit::window::Window;

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
struct FlushGuard<'a>(&'a Cell<bool>);

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

pub struct NativeChildRegistrar<C: NativeChild> {
    pub(super) binding: Rc<ParentBinding>,
    pub(super) coordinator: Rc<WindowLifecycle<C>>,
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

mod observer;
#[cfg(test)]
mod waker;
