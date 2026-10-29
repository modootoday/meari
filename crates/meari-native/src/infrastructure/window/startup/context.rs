use super::super::{
    NativeWindowDriver, NativeWindowObserver, driver,
    forwarding::ForwardingSeed,
    parent::{NativeWindowObservation, ParentBinding},
    plain_lifecycle::PlainLifecycle,
    preparation::NativeWindowPreparation,
};
use super::*;
use crate::{
    application::{WindowOptions, window_lifecycle::*},
    infrastructure::native_host::{
        self, ChildWindowHost, HostLifecycle, HostScheduleLimits, NativeDocumentHandle,
    },
};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::BlitzApplication;
use std::task::Waker;

trait AttachJob<H: NativeWindowHost> {
    fn run(
        &mut self,
        limits: HostScheduleLimits,
        authority: &Rc<Authority>,
    ) -> Result<NativeWindowDriver, NativeAttachProblem>;
}
type PreparedAttachmentResult<H, C> =
    Result<NativeAttachment<ChildWindowHost<H, C>>, Box<super::super::NativeAttachFailure<H, C>>>;
pub struct NativeStartupAttachFailure<H: NativeWindowHost> {
    pub problem: NativeStartupError,
    pub runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    pub observer: Option<NativeWindowObserver>,
}
impl<H: NativeWindowHost> std::fmt::Debug for NativeStartupAttachFailure<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.problem, f)
    }
}
/// Opaque deferred work, not an already attached driver. No caller factory/driver constructor.
/// ```compile_fail,E0308
/// use meari_native::infrastructure::{native_host::NativeWindowHost,window::{NativeStartupPoll,NativeWindowDriver}};
/// fn foreign<H:NativeWindowHost>(driver:NativeWindowDriver)->NativeStartupPoll<H>{NativeStartupPoll::Attach(driver)}
/// ```
#[must_use = "Keep the attachment or return it to the same runner; Drop is not cleanup ACK"]
pub struct NativeAttachment<H: NativeWindowHost> {
    pub(super) authority: Rc<Authority>,
    pub(super) runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    job: Box<dyn AttachJob<H>>,
    attempted: bool,
    observation: Option<NativeWindowObservation>,
}
impl<H: NativeWindowHost> NativeAttachment<H> {
    pub fn observation(&self) -> Option<NativeWindowObservation> {
        self.observation.clone()
    }
    pub fn stamp(&self) -> NativeStartupStamp {
        self.authority.stamp
    }
    pub(super) fn run(
        &mut self,
        authority: &Rc<Authority>,
        limits: HostScheduleLimits,
    ) -> Result<NativeWindowDriver, NativeStartupError> {
        if std::mem::replace(&mut self.attempted, true) {
            return Err(NativeStartupError::AlreadyIssued);
        }
        if !Rc::ptr_eq(&self.authority, authority) {
            return Err(NativeStartupError::Stale);
        }
        self.job
            .run(limits, authority)
            .map_err(NativeStartupError::Attach)
    }
}
struct Plain<H: NativeWindowHost> {
    inner: Option<BlitzApplication<VelloCpuWindowRenderer>>,
    runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    document: NativeDocumentHandle,
    options: WindowOptions,
    observation: Option<NativeWindowObservation>,
}
impl<H: NativeWindowHost> AttachJob<H> for Plain<H> {
    fn run(
        &mut self,
        limits: HostScheduleLimits,
        _: &Rc<Authority>,
    ) -> Result<NativeWindowDriver, NativeAttachProblem> {
        {
            let mut runtime = self
                .runtime
                .try_borrow_mut()
                .map_err(|_| NativeAttachProblem::Borrowed)?;
            if runtime.document().id() != self.document.id() {
                return Err(NativeAttachProblem::ForeignDocument);
            }
            if self.document.lifecycle() != HostLifecycle::Open {
                return Err(NativeAttachProblem::NotPristine);
            }
            if runtime
                .schedule_limits
                .is_some_and(|existing| existing != limits)
            {
                return Err(NativeAttachProblem::Host(
                    "runtime and strict loop schedule limits differ".into(),
                ));
            }
            runtime.schedule_limits = Some(limits);
        }
        let inner = self
            .inner
            .as_mut()
            .ok_or(NativeAttachProblem::NotPristine)?;
        let wake = driver::configure_shell(inner, &self.document, self.options.clone())
            .map_err(NativeAttachProblem::Host)?;
        if let Some(observation) = &self.observation {
            observation
                .binding
                .seed
                .bind_once(self.document.id(), wake.lifecycle_waker())
                .map_err(|error| NativeAttachProblem::Host(error.to_string()))?;
        }
        let port = native_host::window_port::port(self.runtime.clone())
            .map_err(NativeAttachProblem::Host)?;
        let mut driver = NativeWindowDriver::from_shell(
            self.inner.take().expect("validated native shell"),
            wake,
            port,
        );
        if let Some(observation) = &self.observation {
            driver.application.lifecycles.insert(
                self.document.id(),
                Box::new(PlainLifecycle::new(observation.binding.clone())),
            );
        }
        Ok(driver)
    }
}
struct Prepared<H: NativeWindowHost, C: NativeChild> {
    preparation: Option<NativeWindowPreparation<C>>,
    runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
    failure: Option<Box<super::super::NativeAttachFailure<H, C>>>,
}
impl<H: NativeWindowHost, C: NativeChild> AttachJob<ChildWindowHost<H, C>> for Prepared<H, C> {
    fn run(
        &mut self,
        limits: HostScheduleLimits,
        authority: &Rc<Authority>,
    ) -> Result<NativeWindowDriver, NativeAttachProblem> {
        {
            let mut runtime = self
                .runtime
                .try_borrow_mut()
                .map_err(|_| NativeAttachProblem::Borrowed)?;
            if runtime
                .schedule_limits
                .is_some_and(|existing| existing != limits)
            {
                return Err(NativeAttachProblem::Host(
                    "runtime and strict loop schedule limits differ".into(),
                ));
            }
            runtime.schedule_limits = Some(limits);
        }
        let preparation = self
            .preparation
            .take()
            .ok_or(NativeAttachProblem::NotPristine)?;
        match NativeWindowDriver::attach_prepared_owned(
            preparation,
            self.runtime.clone(),
            Some(authority),
        ) {
            Ok(driver) => Ok(driver),
            Err(failure) => {
                let problem = failure.problem.clone();
                self.failure = Some(failure);
                Err(problem)
            }
        }
    }
}
/// A scoped, non-cloneable view of this runner's startup authority. No OS callback/raw loop.
pub struct NativeStartupContext<'a> {
    pub(super) startup: &'a mut NativeStartup,
}
impl NativeStartupContext<'_> {
    pub fn stamp(&self) -> NativeStartupStamp {
        self.startup.stamp()
    }
    pub fn wake_handle(&self) -> NativeStartupWake {
        self.startup.wake_handle()
    }
    pub fn prepare<C: NativeChild>(
        &mut self,
        document: NativeDocumentHandle,
        limits: LifecycleLimits,
        options: WindowOptions,
        observer: Option<NativeWindowObserver>,
    ) -> Result<NativeWindowPreparation<C>, NativeStartupError> {
        if std::mem::replace(&mut self.startup.handoff_attempted, true)
            || self.startup.inner.is_none()
        {
            return Err(NativeStartupError::AlreadyIssued);
        }
        self.startup.document = Some(document.clone());
        if document.lifecycle() != HostLifecycle::Open {
            return Err(NativeStartupError::Attach(NativeAttachProblem::NotPristine));
        }
        let inner = self
            .startup
            .inner
            .as_mut()
            .ok_or(NativeStartupError::AlreadyIssued)?;
        let seed = ForwardingSeed::new(document.id());
        let clock = self.startup.clock;
        let children = Rc::new(
            WindowLifecycle::new(limits, clock, Waker::from(seed.clone())).map_err(|error| {
                NativeStartupError::Attach(NativeAttachProblem::Lifecycle(error))
            })?,
        );
        let wake = driver::configure_shell(inner, &document, options)
            .map_err(NativeStartupError::Invalid)?;
        self.startup.signal.preparing()?;
        let host = document.id();
        let actions = super::super::actions::ActionBinding::new(
            document.clone(),
            Rc::downgrade(&children),
            wake.clone(),
        );
        Ok(NativeWindowPreparation {
            inner: self.startup.inner.take(),
            document,
            wake,
            children,
            binding: ParentBinding::new(host, seed, observer),
            clock,
            handoff_attempted: false,
            startup_authority: Some(self.startup.authority.clone()),
            actions,
        })
    }
    pub fn attach<H: NativeWindowHost>(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<H>>>,
        options: WindowOptions,
    ) -> Result<NativeAttachment<H>, Box<NativeStartupAttachFailure<H>>> {
        self.attach_with_observer(runtime, options, None)
    }
    pub fn attach_observed<H: NativeWindowHost>(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<H>>>,
        options: WindowOptions,
        observer: NativeWindowObserver,
    ) -> Result<NativeAttachment<H>, Box<NativeStartupAttachFailure<H>>> {
        self.attach_with_observer(runtime, options, Some(observer))
    }
    fn attach_with_observer<H: NativeWindowHost>(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<H>>>,
        options: WindowOptions,
        observer: Option<NativeWindowObserver>,
    ) -> Result<NativeAttachment<H>, Box<NativeStartupAttachFailure<H>>> {
        let checked = (|| -> Result<NativeDocumentHandle, NativeStartupError> {
            if std::mem::replace(&mut self.startup.handoff_attempted, true)
                || self.startup.inner.is_none()
            {
                return Err(NativeStartupError::AlreadyIssued);
            }
            let document = runtime
                .try_borrow()
                .map_err(|_| NativeStartupError::Busy)?
                .document();
            self.startup.document = Some(document.clone());
            if document.lifecycle() != HostLifecycle::Open {
                return Err(NativeStartupError::Attach(NativeAttachProblem::NotPristine));
            }
            options.validate().map_err(NativeStartupError::Invalid)?;
            self.startup.signal.preparing()?;
            Ok(document)
        })();
        let document = match checked {
            Ok(document) => document,
            Err(problem) => {
                return Err(Box::new(NativeStartupAttachFailure {
                    problem,
                    runtime,
                    observer,
                }));
            }
        };
        let inner = self.startup.inner.take();
        let observation = observer.map(|observer| NativeWindowObservation {
            binding: ParentBinding::new(
                document.id(),
                ForwardingSeed::new(document.id()),
                Some(observer),
            ),
        });
        Ok(NativeAttachment {
            authority: self.startup.authority.clone(),
            runtime: runtime.clone(),
            job: Box::new(Plain {
                inner,
                runtime,
                document,
                options,
                observation: observation.clone(),
            }),
            attempted: false,
            observation,
        })
    }
    pub fn attach_prepared<H: NativeWindowHost, C: NativeChild>(
        &mut self,
        preparation: NativeWindowPreparation<C>,
        runtime: Rc<RefCell<NativeHostRuntime<ChildWindowHost<H, C>>>>,
    ) -> PreparedAttachmentResult<H, C> {
        let validation = if preparation
            .startup_authority
            .as_ref()
            .is_some_and(|authority| Rc::ptr_eq(authority, &self.startup.authority))
        {
            preparation.validate(&runtime)
        } else {
            Err(NativeAttachProblem::NotPristine)
        };
        if let Err(problem) = validation {
            return Err(Box::new(super::super::NativeAttachFailure {
                stage: super::super::NativeAttachStage::Validation,
                problem,
                preparation,
                runtime,
            }));
        }
        let observation = Some(NativeWindowObservation {
            binding: preparation.binding.clone(),
        });
        Ok(NativeAttachment {
            authority: self.startup.authority.clone(),
            runtime: runtime.clone(),
            job: Box::new(Prepared {
                preparation: Some(preparation),
                runtime,
                failure: None,
            }),
            attempted: false,
            observation,
        })
    }
}
