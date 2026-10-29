//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::{domain::Viewport, infrastructure::document::InteractiveDocument};
use std::fmt;

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "canonical {}", self.0)
    }
}

impl std::error::Error for Failure {}

impl NativeWindowHost for Host {
    type Error = Failure;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Failure> {
        panic!("no callback")
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Failure> {
        if let Some(value) = self.0.borrow().after_error {
            Err(Failure(value))
        } else {
            Ok(HostEffects::default())
        }
    }
    fn retry_after_dispatch(&self, error: &Failure) -> Option<HostRetry> {
        (error.0 == 41).then_some(HostRetry::OnWake)
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Failure> {
        let mut state = self.0.borrow_mut();
        state.closes += 1;
        if state.unavailable_close {
            CloseDecision::ObservationUnavailable {
                checkpoint: (),
                error: Failure(84),
            }
        } else if state.fail_close {
            CloseDecision::RetryableFailure {
                checkpoint: (),
                error: Failure(82),
            }
        } else if state.wait {
            CloseDecision::Pending { checkpoint: () }
        } else {
            CloseDecision::HostCleanupComplete { checkpoint: () }
        }
    }
    fn next_close_observation(&self, _: &()) -> HostRetry {
        self.0.borrow_mut().hints += 1;
        HostRetry::After(Duration::from_millis(5))
    }
    fn observe_close(&mut self, request: CloseRequest<()>) -> CloseDecision<(), Failure> {
        self.0.borrow_mut().close_observations += 1;
        self.close(request)
    }
}

pub(super) fn document() -> NativeDocumentHandle {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>Schedule</main>;}",
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

pub(super) fn limits() -> HostScheduleLimits {
    HostScheduleLimits::new(Duration::from_millis(2), Duration::from_secs(1), 3).unwrap()
}

impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

impl NativeChild for Child {
    type Error = std::io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, Self::Error> {
        self.0.borrow_mut().signal = Some(signal);
        Ok(CleanupProgress::Pending)
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Self::Error> {
        Ok(if self.0.borrow().done {
            CleanupProgress::Complete
        } else {
            CleanupProgress::Pending
        })
    }
}

impl Drop for ObservationChild {
    fn drop(&mut self) {
        self.state.borrow_mut().dropped += 1;
    }
}

impl NativeChild for ObservationChild {
    type Error = Failure;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        signal: CompletionSignal,
    ) -> Result<CleanupProgress, Failure> {
        let mut state = self.state.borrow_mut();
        state.starts += 1;
        state.signal = Some(signal);
        if state.fail_once {
            state.fail_once = false;
            return Err(Failure(99));
        }
        Ok(if state.pending {
            CleanupProgress::Pending
        } else {
            CleanupProgress::ObservationUnavailable
        })
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, Failure> {
        let mut state = self.state.borrow_mut();
        state.observations += 1;
        Ok(if state.done {
            CleanupProgress::Complete
        } else if state.pending {
            CleanupProgress::Pending
        } else {
            CleanupProgress::ObservationUnavailable
        })
    }
    fn observation_error(&self) -> Option<&Failure> {
        Some(&self.diagnostic)
    }
}
