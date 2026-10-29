//! Window ownership values; independent of operating systems, renderers and SDKs.
use std::{fmt, num::NonZeroUsize, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowStamp {
    pub(crate) owner: u64,
    pub(crate) epoch: u64,
    pub(crate) surface: u64,
}
impl WindowStamp {
    pub fn owner(self) -> u64 {
        self.owner
    }
    pub fn epoch(self) -> u64 {
        self.epoch
    }
    pub fn surface_generation(self) -> u64 {
        self.surface
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupStamp {
    pub(crate) window: WindowStamp,
    pub(crate) child: u64,
    pub(crate) revocation: u64,
    pub(crate) attempt: u64,
}
impl CleanupStamp {
    pub fn window(self) -> WindowStamp {
        self.window
    }
    pub fn child(self) -> u64 {
        self.child
    }
    pub fn revocation(self) -> u64 {
        self.revocation
    }
    pub fn attempt(self) -> u64 {
        self.attempt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowPhase {
    NotReady,
    Ready,
    Suspending,
    Suspended,
    Closing,
    Closed,
    ForcedIncomplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyKind {
    NewWindow,
    SameWindow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevocationReason {
    Suspend,
    Close,
    ForcedLoss,
    RegistrationFailed,
    ExplicitRelease,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildPhase {
    Creating,
    Active,
    NeedsCleanup,
    Waiting,
    TimedOut,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupProgress {
    Complete,
    Pending,
    /// Original work remains owned, but its terminal state cannot currently be observed.
    ObservationUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStatus {
    Complete,
    Pending,
    ObservationUnavailable,
    RetryableFailure,
    TimedOut,
    ForcedIncomplete,
}

/// Diagnostic barrier identity; only the coordinator can mint it, never a cleanup authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarrierStamp {
    pub(crate) window: WindowStamp,
    pub(crate) revocation: u64,
    pub(crate) phase: WindowPhase,
}
impl BarrierStamp {
    pub fn window(self) -> WindowStamp {
        self.window
    }
    pub fn revocation(self) -> u64 {
        self.revocation
    }
    pub fn phase(self) -> WindowPhase {
        self.phase
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupReport {
    pub status: CleanupStatus,
    pub remaining: usize,
    pub failed: usize,
    pub timed_out: usize,
    pub unavailable: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct LifecycleLimits {
    pub(crate) children: NonZeroUsize,
    pub(crate) timeout: Duration,
}
impl LifecycleLimits {
    pub fn new(children: usize, cleanup_timeout: Duration) -> Result<Self, LifecycleError> {
        let children = NonZeroUsize::new(children).ok_or(LifecycleError::InvalidLimits)?;
        if cleanup_timeout.is_zero() {
            return Err(LifecycleError::InvalidLimits);
        }
        Ok(Self {
            children,
            timeout: cleanup_timeout,
        })
    }
    pub fn max_children(self) -> usize {
        self.children.get()
    }
    pub fn cleanup_timeout(self) -> Duration {
        self.timeout
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidLimits,
    Capacity,
    NotReady,
    Suspended,
    Stale,
    Busy,
    Closing,
    Closed,
    WrongState,
    CounterExhausted,
    ClockWentBackwards,
    DeadlineOverflow,
}
impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native window lifecycle: {self:?}")
    }
}
impl std::error::Error for LifecycleError {}
