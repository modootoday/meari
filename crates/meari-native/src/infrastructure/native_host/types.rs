use meari::{application::host_callbacks::PendingCallback, domain::plugins::PluginScope};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostLifecycle {
    Open,
    Closing,
    CleanupFailed,
    HostCleaned,
    Closed,
    ForcedIncomplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostStamp {
    pub host_id: u64,
    pub owner: PluginScope,
    pub definition_revision: u64,
    pub document_revision: u64,
    pub turn_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAccessError {
    Busy,
    Closing,
    Closed,
    Stale,
    CounterExhausted,
    Invalid(String),
}
impl fmt::Display for HostAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            other => write!(f, "native host access: {other:?}"),
        }
    }
}
impl std::error::Error for HostAccessError {}

#[derive(Debug)]
pub enum HostUpdateError<E> {
    Access(HostAccessError),
    Apply(E),
}
impl<E: fmt::Display> fmt::Display for HostUpdateError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(error) => error.fmt(f),
            Self::Apply(error) => error.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for HostUpdateError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Access(error) => error,
            Self::Apply(error) => error,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommitReceipt {
    pub(super) before: HostStamp,
    pub(super) after: HostStamp,
}
impl HostCommitReceipt {
    pub fn before(&self) -> &HostStamp {
        &self.before
    }
    pub fn after(&self) -> &HostStamp {
        &self.after
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTurnReason {
    Input,
    Accessibility,
    ResumePending,
    Completion,
}
#[derive(Clone, Debug)]
pub struct HostTurn {
    pub stamp: HostStamp,
    pub reason: HostTurnReason,
    pub accepted_callbacks: usize,
    pub rejected_callbacks: usize,
    pub remaining_callbacks: usize,
}

/// Admission must not execute SDK mutations. Accepted work belongs to the consumer's queue.
pub enum IntentAdmission<E> {
    Accepted,
    Capacity,
    Rejected(E),
}

#[derive(Default)]
pub struct HostEffects {
    /// Minted by the same handle after the consumer's synchronous reproject succeeds.
    pub committed: Option<HostCommitReceipt>,
    pub redraw: bool,
}

#[derive(Clone, Debug)]
pub struct HostTurnReceipt {
    pub stamp: HostStamp,
    pub accepted_callbacks: usize,
    pub rejected_callbacks: usize,
    pub remaining_callbacks: usize,
    pub redraw: bool,
    pub committed: Option<HostCommitReceipt>,
    pub admitted_this_pass: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseCause {
    User,
    Host,
    SourceTransition,
    ForcedOsDestroy,
}
pub struct CloseRequest<C> {
    pub stamp: HostStamp,
    pub cause: CloseCause,
    pub callbacks: CallbackCloseReceipt,
    pub checkpoint: Option<C>,
}
pub enum CloseDecision<C, E> {
    KeepOpen,
    Pending {
        checkpoint: C,
    },
    /// Retain in-flight work and the diagnostic while permitting same-attempt wake observation.
    ObservationUnavailable {
        checkpoint: C,
        error: E,
    },
    RetryableFailure {
        checkpoint: C,
        error: E,
    },
    HostCleanupComplete {
        checkpoint: C,
    },
    ForcedIncomplete {
        checkpoint: Option<C>,
        error: Option<E>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallbackCloseReceipt {
    pub stamp: HostStamp,
    pub discarded_callbacks: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCloseStatus {
    KeepOpen,
    Pending,
    ObservationUnavailable,
    RetryableFailure,
    HostCleanupComplete,
    ForcedIncomplete,
}

pub(super) struct PendingTurn {
    pub stamp: HostStamp,
    pub callbacks: std::collections::VecDeque<PendingCallback>,
    pub accepted: usize,
    pub rejected: usize,
}
