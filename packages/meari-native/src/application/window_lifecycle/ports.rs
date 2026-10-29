use super::CompletionSignal;
use crate::domain::window_lifecycle::{CleanupProgress, CleanupStamp, RevocationReason};
use std::time::Duration;

/// Elapsed monotonic time from a stable, host-owned origin; injectable without OS sleeps.
pub trait MonotonicClock: 'static {
    fn now(&self) -> Duration;
}

#[derive(Clone, Copy, Debug)]
pub struct CleanupRequest {
    pub stamp: CleanupStamp,
    pub reason: RevocationReason,
}

/// An infrastructure child adapter. No Send requirement or SDK policy in this port.
pub trait NativeChild: 'static {
    type Error: std::error::Error + 'static;
    /// Revoke parent access without restarting an in-flight cleanup. No SDK mutation here.
    fn parent_lost(&mut self);
    /// Err means terminal retryable failure, with no cleanup work still in flight.
    /// Pending means the original work remains owned here; notify only after state changes.
    fn begin_cleanup(
        &mut self,
        request: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, Self::Error>;
    /// Observe the same attempt; never restart teardown, discard terminal state or run SDK work.
    /// ObservationUnavailable preserves the work and signal; it is neither Err nor a join proof.
    fn observe_cleanup(&mut self, request: CleanupRequest) -> Result<CleanupProgress, Self::Error>;
    /// Borrow the original observation diagnostic, never a joined/terminal proof.
    fn observation_error(&self) -> Option<&Self::Error> {
        None
    }
}

/// Failed native creation may still own resources, retaining its reserved cleanup slot.
pub struct ChildCreationFailure<C: NativeChild> {
    pub error: C::Error,
    pub partial: Option<C>,
}
