//! Strict same-loop startup. Only the runner executes deferred attachments.
mod context;
mod owner;
mod runner;
#[cfg(test)]
mod tests;
mod wake;
use super::{NativeAttachProblem, NativeWindowWake};
use crate::infrastructure::native_host::{
    HostAccessError, HostRetry, NativeHostRuntime, NativeWindowHost,
};
pub use context::{NativeAttachment, NativeStartupAttachFailure, NativeStartupContext};
pub(super) use owner::Authority;
pub use owner::{NativeLoopOwner, NativeStartup};
use std::{cell::RefCell, fmt, rc::Rc};
pub use wake::NativeStartupWake;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeStartupStamp {
    owner: u64,
    generation: u64,
}
impl NativeStartupStamp {
    pub fn owner(self) -> u64 {
        self.owner
    }
    pub fn generation(self) -> u64 {
        self.generation
    }
}
#[derive(Debug)]
pub enum NativeStartupError {
    Busy,
    Closed,
    Stale,
    AlreadyIssued,
    CounterExhausted,
    Invalid(String),
    Attach(NativeAttachProblem),
    Loop(winit::error::EventLoopError),
}
impl fmt::Display for NativeStartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native startup: {self:?}")
    }
}
impl std::error::Error for NativeStartupError {}
#[derive(Debug)]
pub enum NativeRunError<E> {
    Startup(NativeStartupError),
    Consumer(E),
    Scheduling(HostAccessError),
    Panicked,
    ForcedIncomplete,
}
pub enum NativeStartupPoll<H: NativeWindowHost> {
    Pending(HostRetry),
    Attach(NativeAttachment<H>),
    Stop,
}
pub trait NativeStartupController: 'static {
    type Host: NativeWindowHost;
    type Error: std::error::Error + 'static;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<Self::Host>, Self::Error>;
    /// Notification handoff only; the driver has not created its actual View yet.
    fn attached(
        &mut self,
        _runtime: Rc<RefCell<NativeHostRuntime<Self::Host>>>,
        _wake: NativeWindowWake,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
/// Retains consumer and runtime authority even after failed/forced native shutdown.
/// Normal attached success waits for owned windows' Destroyed events; Closed notices alone
/// acknowledge logical removal. Failure/Drop does not certify SDK cleanup or native teardown.
pub struct NativeRunOutcome<S: NativeStartupController> {
    pub controller: S,
    pub startup: Option<NativeStartup>,
    pub runtime: Option<Rc<RefCell<NativeHostRuntime<S::Host>>>>,
    pub recovery: Option<NativeAttachment<S::Host>>,
    pub loop_owner: Option<NativeLoopOwner>,
    pub result: Result<(), NativeRunError<S::Error>>,
}

/// The legacy embedding facade re-exports the already pinned version; strict users need no raw loop.
pub mod loop_api {
    pub use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopBuilder},
        window::WindowId,
    };
}
