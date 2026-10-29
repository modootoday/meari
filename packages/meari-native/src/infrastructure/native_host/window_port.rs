use super::*;
use std::{cell::RefCell, rc::Rc};

pub(crate) trait WindowHostPort {
    fn document(&self) -> NativeDocumentHandle;
    fn after_dispatch(&self, reason: HostTurnReason) -> Result<HostTurnReceipt, HostAttemptError>;
    fn close(
        &self,
        cause: CloseCause,
        explicit: bool,
    ) -> Result<(HostCloseStatus, Option<HostRetry>), HostAttemptError>;
    fn schedule_limits(&self) -> Option<HostScheduleLimits>;
    fn completion_idle(&self) -> bool {
        false
    }
    fn removed(&self, forced: bool) -> Result<(), String>;
    fn host_error(&self) -> Option<String>;
}
struct Port<H: NativeWindowHost> {
    runtime: Rc<RefCell<NativeHostRuntime<H>>>,
    document: NativeDocumentHandle,
    limits: Option<HostScheduleLimits>,
    last_scope: RefCell<HostStamp>,
    pending_explicit: RefCell<Option<CloseIntent>>,
}
#[derive(Clone)]
struct CloseIntent {
    cause: CloseCause,
    scope: HostStamp,
}
fn same_close_scope(left: &HostStamp, right: &HostStamp) -> bool {
    left.host_id == right.host_id
        && left.owner == right.owner
        && left.definition_revision == right.definition_revision
}
impl<H: NativeWindowHost> Port<H> {
    fn remember_close(&self, cause: CloseCause) -> Result<(), HostAccessError> {
        let scope = match self.document.stamp() {
            Ok(scope) => {
                *self.last_scope.borrow_mut() = scope.clone();
                scope
            }
            // Retain the last verified binding; execution must revalidate it after the borrow.
            Err(HostAccessError::Busy) => self.last_scope.borrow().clone(),
            Err(error) => return Err(error),
        };
        let mut pending = self.pending_explicit.borrow_mut();
        if pending
            .as_ref()
            .is_some_and(|intent| !same_close_scope(&intent.scope, &scope))
        {
            pending.take();
            return Err(HostAccessError::Stale);
        }
        if pending.is_none() || cause == CloseCause::ForcedOsDestroy {
            *pending = Some(CloseIntent { cause, scope });
        }
        Ok(())
    }
}
impl<H: NativeWindowHost> WindowHostPort for Port<H> {
    fn completion_idle(&self) -> bool {
        self.runtime
            .try_borrow()
            .is_ok_and(|runtime| !runtime.has_pending_turn())
    }
    fn document(&self) -> NativeDocumentHandle {
        self.document.clone()
    }
    fn after_dispatch(&self, reason: HostTurnReason) -> Result<HostTurnReceipt, HostAttemptError> {
        let mut runtime = self
            .runtime
            .try_borrow_mut()
            .map_err(|_| HostAttemptError {
                error: HostAccessError::Busy,
                retry: Some(HostRetry::OnWake),
            })?;
        runtime.after_dispatch(reason).map_err(|error| {
            let retry = if error == HostAccessError::Busy {
                Some(HostRetry::OnWake)
            } else {
                runtime.turn_retry
            };
            HostAttemptError { error, retry }
        })
    }
    fn close(
        &self,
        cause: CloseCause,
        explicit: bool,
    ) -> Result<(HostCloseStatus, Option<HostRetry>), HostAttemptError> {
        if explicit {
            self.remember_close(cause)
                .map_err(|error| HostAttemptError {
                    retry: (error == HostAccessError::Busy).then_some(HostRetry::OnWake),
                    error,
                })?;
        }
        let mut runtime = self
            .runtime
            .try_borrow_mut()
            .map_err(|_| HostAttemptError {
                error: HostAccessError::Busy,
                retry: Some(HostRetry::OnWake),
            })?;
        let intent = self.pending_explicit.borrow().clone();
        let result = if let Some(intent) = intent {
            let current = self.document.stamp().map_err(|error| HostAttemptError {
                retry: (error == HostAccessError::Busy).then_some(HostRetry::OnWake),
                error,
            })?;
            if !same_close_scope(&intent.scope, &current) {
                self.pending_explicit.borrow_mut().take();
                return Err(HostAttemptError {
                    error: HostAccessError::Stale,
                    retry: None,
                });
            }
            let result = runtime.request_close(intent.cause);
            // Busy is returned before consumer close execution; keep that original intent.
            if !matches!(result, Err(HostAccessError::Busy)) {
                self.pending_explicit.borrow_mut().take();
            }
            result
        } else {
            runtime.observe_close(cause)
        };
        result
            .map(|status| (status, runtime.close_retry))
            .map_err(|error| {
                let retry = if error == HostAccessError::Busy {
                    Some(HostRetry::OnWake)
                } else {
                    None
                };
                HostAttemptError { error, retry }
            })
    }
    fn schedule_limits(&self) -> Option<HostScheduleLimits> {
        self.limits
    }
    fn removed(&self, forced: bool) -> Result<(), String> {
        let result = match self.runtime.try_borrow_mut() {
            Ok(mut runtime) => runtime
                .window_removed(forced)
                .map_err(|error| error.to_string()),
            Err(_) if forced => {
                self.document
                    .begin_close(0)
                    .map_err(|error| error.to_string())?;
                self.document
                    .finish_close(true)
                    .map_err(|error| error.to_string())
            }
            Err(_) => Err("native host is already borrowed".into()),
        };
        if result.is_ok() || forced {
            self.pending_explicit.borrow_mut().take();
        }
        result
    }
    fn host_error(&self) -> Option<String> {
        self.runtime
            .try_borrow()
            .ok()
            .and_then(|runtime| runtime.last_host_error().map(ToString::to_string))
    }
}
pub(crate) fn port<H: NativeWindowHost>(
    runtime: Rc<RefCell<NativeHostRuntime<H>>>,
) -> Result<Box<dyn WindowHostPort>, String> {
    let document = runtime
        .try_borrow()
        .map_err(|_| "native host is already borrowed".to_owned())?
        .document();
    let limits = runtime
        .try_borrow()
        .map_err(|_| "native host is already borrowed".to_owned())?
        .schedule_limits;
    let scope = document.stamp().map_err(|error| error.to_string())?;
    runtime
        .try_borrow_mut()
        .map_err(|_| "native host is already borrowed".to_owned())?
        .attach()
        .map_err(|error| error.to_string())?;
    Ok(Box::new(Port {
        runtime,
        document,
        limits,
        last_scope: RefCell::new(scope),
        pending_explicit: RefCell::new(None),
    }))
}

pub(crate) struct StandaloneHost;
impl NativeWindowHost for StandaloneHost {
    type Error = std::io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        IntentAdmission::Rejected(std::io::Error::other(
            "native callbacks require an injected consumer host",
        ))
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Self::Error> {
        Ok(HostEffects::default())
    }
    fn close(
        &mut self,
        _: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error> {
        CloseDecision::HostCleanupComplete { checkpoint: () }
    }
}
