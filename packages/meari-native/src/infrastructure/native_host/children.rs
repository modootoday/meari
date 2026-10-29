//! Compose the existing host protocol with a child-first barrier; do not duplicate its runtime.
use super::*;
use crate::application::window_lifecycle::{
    BarrierStamp, CleanupReport, CleanupStatus, LifecycleError, NativeChild, WindowLifecycle,
};
use meari::application::host_callbacks::PendingCallback;
use std::{fmt, rc::Rc};

#[derive(Debug)]
pub enum ChildHostError<E> {
    Host(E),
    Lifecycle(LifecycleError),
    Children(CleanupReport),
    HostKeptOpenAfterRevocation,
}
impl<E: fmt::Display> fmt::Display for ChildHostError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => write!(f, "{error}"),
            Self::Lifecycle(error) => write!(f, "{error}"),
            Self::Children(report) => write!(
                f,
                "native child cleanup: {:?} ({} remaining)",
                report.status, report.remaining
            ),
            Self::HostKeptOpenAfterRevocation => f.write_str(
                "host returned KeepOpen after irreversible revocation; normal retry is blocked",
            ),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for ChildHostError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Lifecycle(error) => Some(error),
            Self::Children(_) | Self::HostKeptOpenAfterRevocation => None,
        }
    }
}

pub struct ChildCloseCheckpoint<C> {
    host_id: u64,
    child_owner: u64,
    barrier: Option<BarrierStamp>,
    children_complete: bool,
    retry_children: bool,
    host_refused: bool,
    host: Option<C>,
}

/// The driver must bind this same coordinator to its actual window before ready/registration.
/// Until driver integration exists this is a headless application barrier, not a parent-handle API.
pub struct ChildWindowHost<H: NativeWindowHost, C: NativeChild> {
    host: H,
    children: Rc<WindowLifecycle<C>>,
}
impl<H: NativeWindowHost, C: NativeChild> ChildWindowHost<H, C> {
    pub fn new(host: H, children: Rc<WindowLifecycle<C>>) -> Self {
        Self { host, children }
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
    pub fn children(&self) -> &Rc<WindowLifecycle<C>> {
        &self.children
    }
}
impl<H: NativeWindowHost, C: NativeChild> NativeWindowHost for ChildWindowHost<H, C> {
    type Error = ChildHostError<H::Error>;
    type CloseCheckpoint = ChildCloseCheckpoint<H::CloseCheckpoint>;
    fn admit_callback(
        &mut self,
        stamp: &HostStamp,
        callback: &PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        match self.host.admit_callback(stamp, callback) {
            IntentAdmission::Accepted => IntentAdmission::Accepted,
            IntentAdmission::Capacity => IntentAdmission::Capacity,
            IntentAdmission::Rejected(error) => {
                IntentAdmission::Rejected(ChildHostError::Host(error))
            }
        }
    }
    fn after_dispatch(&mut self, turn: HostTurn) -> Result<HostEffects, Self::Error> {
        self.host.after_dispatch(turn).map_err(ChildHostError::Host)
    }
    fn retry_after_dispatch(&self, error: &Self::Error) -> Option<HostRetry> {
        match error {
            ChildHostError::Host(error) => self.host.retry_after_dispatch(error),
            _ => None,
        }
    }
    fn next_close_observation(&self, checkpoint: &Self::CloseCheckpoint) -> HostRetry {
        if checkpoint.children_complete {
            checkpoint.host.as_ref().map_or(HostRetry::OnWake, |host| {
                self.host.next_close_observation(host)
            })
        } else {
            HostRetry::OnWake
        }
    }
    fn can_close(&mut self, stamp: &HostStamp, cause: CloseCause) -> Result<bool, Self::Error> {
        self.host
            .can_close(stamp, cause)
            .map_err(ChildHostError::Host)
    }
    fn close(
        &mut self,
        request: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error> {
        self.close_inner(request, true)
    }
    fn observe_close(
        &mut self,
        request: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error> {
        self.close_inner(request, false)
    }
}
type ChildDecision<H> = CloseDecision<
    ChildCloseCheckpoint<<H as NativeWindowHost>::CloseCheckpoint>,
    ChildHostError<<H as NativeWindowHost>::Error>,
>;
impl<H: NativeWindowHost, C: NativeChild> ChildWindowHost<H, C> {
    fn close_inner(
        &mut self,
        request: CloseRequest<ChildCloseCheckpoint<H::CloseCheckpoint>>,
        explicit: bool,
    ) -> ChildDecision<H> {
        let owner = self.children.owner_id();
        let mut checkpoint = request.checkpoint.unwrap_or(ChildCloseCheckpoint {
            host_id: request.stamp.host_id,
            child_owner: owner,
            barrier: None,
            children_complete: false,
            retry_children: false,
            host_refused: false,
            host: None,
        });
        if checkpoint.host_id != request.stamp.host_id || checkpoint.child_owner != owner {
            return CloseDecision::RetryableFailure {
                checkpoint,
                error: ChildHostError::Lifecycle(LifecycleError::Stale),
            };
        }
        let forced = request.cause == CloseCause::ForcedOsDestroy;
        let current = match self.children.barrier_stamp() {
            Ok(stamp) => stamp,
            Err(error) => {
                return CloseDecision::RetryableFailure {
                    checkpoint,
                    error: ChildHostError::Lifecycle(error),
                };
            }
        };
        let stale = checkpoint.barrier.is_some_and(|stamp| stamp != current);
        if forced {
            let progress = self.children.force_loss();
            if let Ok(stamp) = self.children.barrier_stamp() {
                checkpoint.barrier = Some(stamp);
            }
            let error = if stale {
                ChildHostError::Lifecycle(LifecycleError::Stale)
            } else {
                match progress {
                    Ok(report) => ChildHostError::Children(report),
                    Err(error) => ChildHostError::Lifecycle(error),
                }
            };
            // Retain the business host for explicit residual cleanup; never certify forced success.
            return CloseDecision::ForcedIncomplete {
                checkpoint: Some(checkpoint),
                error: Some(error),
            };
        }
        if stale {
            return CloseDecision::RetryableFailure {
                checkpoint,
                error: ChildHostError::Lifecycle(LifecycleError::Stale),
            };
        }
        if checkpoint.host_refused {
            return CloseDecision::RetryableFailure {
                checkpoint,
                error: ChildHostError::HostKeptOpenAfterRevocation,
            };
        }
        if !checkpoint.children_complete {
            let progress = if explicit && checkpoint.retry_children {
                self.children.retry_failed()
            } else {
                self.children
                    .begin_close()
                    .and_then(|_| self.children.poll())
            };
            match self.children.barrier_stamp() {
                Ok(stamp) => checkpoint.barrier = Some(stamp),
                Err(error) => {
                    return CloseDecision::RetryableFailure {
                        checkpoint,
                        error: ChildHostError::Lifecycle(error),
                    };
                }
            }
            match progress {
                Err(error) => {
                    return CloseDecision::RetryableFailure {
                        checkpoint,
                        error: ChildHostError::Lifecycle(error),
                    };
                }
                Ok(report) if report.status == CleanupStatus::Complete => {
                    checkpoint.children_complete = true
                }
                Ok(report) if report.status == CleanupStatus::RetryableFailure => {
                    checkpoint.retry_children = true;
                    return CloseDecision::RetryableFailure {
                        checkpoint,
                        error: ChildHostError::Children(report),
                    };
                }
                Ok(report) if report.status == CleanupStatus::ObservationUnavailable => {
                    checkpoint.retry_children = report.failed != 0;
                    return CloseDecision::ObservationUnavailable {
                        checkpoint,
                        error: ChildHostError::Children(report),
                    };
                }
                Ok(_) => {
                    checkpoint.retry_children = false;
                    return CloseDecision::Pending { checkpoint };
                }
            }
        }
        let request = CloseRequest {
            stamp: request.stamp,
            cause: request.cause,
            callbacks: request.callbacks,
            checkpoint: checkpoint.host.take(),
        };
        let decision = if explicit {
            self.host.close(request)
        } else {
            self.host.observe_close(request)
        };
        match decision {
            CloseDecision::KeepOpen => {
                // Neither revoked exports nor destroyed children can be generically restored.
                // Keep the outer checkpoint and never replay this ambiguous host operation.
                checkpoint.host_refused = true;
                CloseDecision::RetryableFailure {
                    checkpoint,
                    error: ChildHostError::HostKeptOpenAfterRevocation,
                }
            }
            CloseDecision::Pending { checkpoint: host } => {
                checkpoint.host = Some(host);
                CloseDecision::Pending { checkpoint }
            }
            CloseDecision::RetryableFailure {
                checkpoint: host,
                error,
            } => {
                checkpoint.host = Some(host);
                CloseDecision::RetryableFailure {
                    checkpoint,
                    error: ChildHostError::Host(error),
                }
            }
            CloseDecision::ObservationUnavailable {
                checkpoint: host,
                error,
            } => {
                checkpoint.host = Some(host);
                CloseDecision::ObservationUnavailable {
                    checkpoint,
                    error: ChildHostError::Host(error),
                }
            }
            CloseDecision::HostCleanupComplete { checkpoint: host } => {
                checkpoint.host = Some(host);
                CloseDecision::HostCleanupComplete { checkpoint }
            }
            CloseDecision::ForcedIncomplete {
                checkpoint: host,
                error,
            } => {
                checkpoint.host = host;
                CloseDecision::ForcedIncomplete {
                    checkpoint: Some(checkpoint),
                    error: error.map(ChildHostError::Host),
                }
            }
        }
    }
}
