//! A single-use assembly owner; a failed handoff returns the very same runtime and preparation.
use super::{NativeWindowDriver, NativeWindowWake, driver, forwarding::ForwardingSeed, parent::*};
use crate::{
    application::{WindowOptions, window_lifecycle::*},
    infrastructure::native_host::{
        ChildWindowHost, HostLifecycle, NativeDocumentHandle, NativeHostRuntime, NativeWindowHost,
    },
};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::BlitzApplication;
use std::{cell::RefCell, fmt, rc::Rc, task::Waker, time::Instant};

pub(super) use super::scheduling::NativeClock;

/// Prepare on the caller's event loop, construct ChildWindowHost with children(), then consume attach.
/// Preparing creates no OS window and grants no parent handle access.
/// Every attach attempt consumes this authority, including Validation/Borrowed failures. The failure
/// returns owners for explicit cleanup, not retrying or transplanting the coordinator into a new seed.
///
/// ```compile_fail,E0382
/// use meari_native::{application::window_lifecycle::NativeChild,
///     infrastructure::{native_host::{ChildWindowHost, NativeHostRuntime, NativeWindowHost}, window::NativeWindowPreparation}};
/// use std::{cell::RefCell, rc::Rc};
/// fn twice<H: NativeWindowHost, C: NativeChild>(prepared: NativeWindowPreparation<C>,
///     runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>) {
///     let _ = prepared.attach(runtime.clone());
///     let _ = prepared.attach(runtime); // moved single-use authority
/// }
/// ```
pub struct NativeWindowPreparation<C: NativeChild> {
    pub(super) inner: Option<BlitzApplication<VelloCpuWindowRenderer>>,
    pub(super) document: NativeDocumentHandle,
    pub(super) wake: NativeWindowWake,
    pub(super) children: Rc<WindowLifecycle<C>>,
    pub(super) binding: Rc<ParentBinding>,
    pub(super) clock: NativeClock,
    pub(super) handoff_attempted: bool,
    pub(super) startup_authority: Option<Rc<super::startup::Authority>>,
    pub(super) actions: Rc<super::actions::ActionBinding>,
}

#[derive(Clone, Debug)]
pub enum NativeAttachProblem {
    Borrowed,
    ForeignDocument,
    ForeignCoordinator,
    NotPristine,
    Lifecycle(LifecycleError),
    Host(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAttachStage {
    Validation,
    SinkBinding,
    RuntimeHandoff,
}
impl fmt::Display for NativeAttachProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native attach: {self:?}")
    }
}
impl std::error::Error for NativeAttachProblem {}

/// No cleanup ACK on failure/Drop. Callers recover the same SDK host and reserved child owner.
pub struct NativeAttachFailure<H: NativeWindowHost, C: NativeChild> {
    pub stage: NativeAttachStage,
    pub problem: NativeAttachProblem,
    pub preparation: NativeWindowPreparation<C>,
    pub runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
}
impl<H: NativeWindowHost, C: NativeChild> fmt::Debug for NativeAttachFailure<H, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.problem, f)
    }
}

impl<C: NativeChild> NativeWindowPreparation<C> {
    pub fn new(
        event_loop: &blitz_shell::EventLoop,
        document: NativeDocumentHandle,
        limits: LifecycleLimits,
        options: WindowOptions,
        observer: Option<NativeWindowObserver>,
    ) -> Result<Self, NativeAttachProblem> {
        if document.lifecycle() != HostLifecycle::Open {
            return Err(NativeAttachProblem::NotPristine);
        }
        let (inner, wake) =
            driver::shell(event_loop, &document, options).map_err(NativeAttachProblem::Host)?;
        let clock = NativeClock {
            origin: Instant::now(),
        };
        let seed = ForwardingSeed::new(document.id());
        let children = Rc::new(
            WindowLifecycle::new(limits, clock, Waker::from(seed.clone()))
                .map_err(NativeAttachProblem::Lifecycle)?,
        );
        // Keep early notifications bounded until attach validates the actual runtime identity.
        let binding = ParentBinding::new(document.id(), seed, observer);
        let actions = super::actions::ActionBinding::new(
            document.clone(),
            Rc::downgrade(&children),
            wake.clone(),
        );
        Ok(Self {
            inner: Some(inner),
            document,
            wake,
            children,
            binding,
            clock,
            handoff_attempted: false,
            startup_authority: None,
            actions,
        })
    }
    pub fn children(&self) -> Rc<WindowLifecycle<C>> {
        self.children.clone()
    }
    /// A weak UI-thread port. It is NotReady until this original prepared window is ready.
    pub fn window_actions(&self) -> super::NativeWindowActions {
        self.actions.handle()
    }
    pub fn registrar(&self) -> NativeChildRegistrar<C> {
        NativeChildRegistrar {
            binding: self.binding.clone(),
            coordinator: self.children.clone(),
        }
    }
    /// Only this original preparation can combine its document and parent registration authority.
    pub fn surface_slots(&self) -> super::surface_slots::NativeSurfaceRegistrar<C> {
        super::surface_slots::NativeSurfaceRegistrar::new(self.document.clone(), self.registrar())
    }
    pub fn attach<H: NativeWindowHost>(
        self,
        runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
    ) -> Result<NativeWindowDriver, Box<NativeAttachFailure<H, C>>> {
        NativeWindowDriver::attach_prepared(self, runtime)
    }
    pub(super) fn validate<H: NativeWindowHost>(
        &self,
        runtime: &Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
    ) -> Result<(), NativeAttachProblem> {
        let runtime = runtime
            .try_borrow()
            .map_err(|_| NativeAttachProblem::Borrowed)?;
        validate_identity(
            &self.document,
            &self.children,
            &runtime.document(),
            runtime.host().children(),
        )?;
        if self.inner.is_none() || self.handoff_attempted {
            return Err(NativeAttachProblem::NotPristine);
        }
        self.binding
            .seed
            .require_unbound(self.document.id())
            .map_err(NativeAttachProblem::Lifecycle)
    }
}
pub(super) fn validate_identity<C: NativeChild>(
    expected: &NativeDocumentHandle,
    children: &Rc<WindowLifecycle<C>>,
    actual: &NativeDocumentHandle,
    actual_children: &Rc<WindowLifecycle<C>>,
) -> Result<(), NativeAttachProblem> {
    // NativeDocumentHandle IDs are constructor-minted and immutable, never caller-supplied DTOs.
    if expected.id() != actual.id() {
        return Err(NativeAttachProblem::ForeignDocument);
    }
    if !Rc::ptr_eq(children, actual_children) {
        return Err(NativeAttachProblem::ForeignCoordinator);
    }
    if actual.lifecycle() != HostLifecycle::Open
        || children.phase().map_err(NativeAttachProblem::Lifecycle)? != WindowPhase::NotReady
        || children
            .report()
            .map_err(NativeAttachProblem::Lifecycle)?
            .remaining
            != 0
    {
        return Err(NativeAttachProblem::NotPristine);
    }
    Ok(())
}
impl<C: NativeChild> Drop for NativeWindowPreparation<C> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            self.wake.close();
            self.binding.revoke();
            self.binding.discard_observer();
            let _ = self.children.abandon_window();
        }
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::{domain::Viewport, infrastructure::document::InteractiveDocument};
    use std::time::Duration;
    struct Child;
    impl NativeChild for Child {
        type Error = std::io::Error;
        fn parent_lost(&mut self) {}
        fn begin_cleanup(
            &mut self,
            _: CleanupRequest,
            _: CompletionSignal,
        ) -> Result<CleanupProgress, Self::Error> {
            Ok(CleanupProgress::Complete)
        }
        fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
            panic!("no pending work")
        }
    }
    fn document() -> NativeDocumentHandle {
        let source = meari::parse(
            "type Props={};export function App(props:Props){return <main>Native</main>;}",
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
    fn children() -> Rc<WindowLifecycle<Child>> {
        Rc::new(
            WindowLifecycle::new(
                LifecycleLimits::new(2, Duration::from_secs(1)).unwrap(),
                NativeClock {
                    origin: Instant::now(),
                },
                Waker::from(ForwardingSeed::new(1)),
            )
            .unwrap(),
        )
    }
    #[test]
    fn identity_accepts_only_the_same_document_and_same_rc_coordinator() {
        let document = document();
        let owner = children();
        validate_identity(&document, &owner, &document.clone(), &owner.clone()).unwrap();
        let foreign = children();
        assert!(matches!(
            validate_identity(&document, &owner, &document, &foreign),
            Err(NativeAttachProblem::ForeignCoordinator)
        ));
        assert_eq!(owner.phase().unwrap(), WindowPhase::NotReady);
        assert_eq!(foreign.phase().unwrap(), WindowPhase::NotReady);
    }
    #[test]
    fn foreign_document_is_rejected_without_advancing_either_owner() {
        let expected = document();
        let actual = document();
        let owner = children();
        assert!(matches!(
            validate_identity(&expected, &owner, &actual, &owner),
            Err(NativeAttachProblem::ForeignDocument)
        ));
        assert_eq!(expected.lifecycle(), HostLifecycle::Open);
        assert_eq!(actual.lifecycle(), HostLifecycle::Open);
    }
    #[test]
    fn prematurely_readied_or_abandoned_coordinator_is_not_attachable() {
        let document = document();
        let owner = children();
        owner.ready(ReadyKind::NewWindow).unwrap();
        assert!(matches!(
            validate_identity(&document, &owner, &document, &owner),
            Err(NativeAttachProblem::NotPristine)
        ));
        owner.abandon_window().unwrap();
        assert!(matches!(
            validate_identity(&document, &owner, &document, &owner),
            Err(NativeAttachProblem::NotPristine)
        ));
    }
}
