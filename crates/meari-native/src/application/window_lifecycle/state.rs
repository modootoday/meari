use super::{CompletionSignal, NativeChild};
use crate::domain::window_lifecycle::*;
use std::{collections::BTreeMap, time::Duration};

pub(super) struct Entry<C: NativeChild> {
    pub window: WindowStamp,
    pub adapter: Option<C>,
    pub phase: ChildPhase,
    pub revocation: u64,
    pub attempt: u64,
    pub reason: RevocationReason,
    pub deadline: Option<Duration>,
    pub signal: Option<CompletionSignal>,
    pub error: Option<C::Error>,
    pub observation_unavailable: bool,
}
impl<C: NativeChild> Entry<C> {
    pub fn deactivate(&self) {
        if let Some(signal) = &self.signal {
            signal.deactivate();
        }
    }
}

pub(super) struct State<C: NativeChild> {
    pub phase: WindowPhase,
    pub epoch: u64,
    pub surface: u64,
    pub sequence: u64,
    pub revocation: u64,
    pub last_now: Duration,
    pub loss_notification_pending: bool,
    pub entries: BTreeMap<u64, Entry<C>>,
}
impl<C: NativeChild> State<C> {
    pub fn stamp(&self, owner: u64) -> WindowStamp {
        WindowStamp {
            owner,
            epoch: self.epoch,
            surface: self.surface,
        }
    }
    pub fn report(&self) -> CleanupReport {
        let failed = self
            .entries
            .values()
            .filter(|entry| entry.phase == ChildPhase::Failed)
            .count();
        let timed_out = self
            .entries
            .values()
            .filter(|entry| entry.phase == ChildPhase::TimedOut)
            .count();
        let unavailable = self
            .entries
            .values()
            .filter(|entry| entry.observation_unavailable)
            .count();
        let status = if self.phase == WindowPhase::ForcedIncomplete {
            CleanupStatus::ForcedIncomplete
        } else if self.entries.is_empty() {
            CleanupStatus::Complete
        } else if unavailable != 0 {
            CleanupStatus::ObservationUnavailable
        } else if failed != 0 {
            CleanupStatus::RetryableFailure
        } else if timed_out != 0 {
            CleanupStatus::TimedOut
        } else {
            CleanupStatus::Pending
        };
        CleanupReport {
            status,
            remaining: self.entries.len(),
            failed,
            timed_out,
            unavailable,
        }
    }
    pub fn finish_suspend(&mut self) {
        if self.phase == WindowPhase::Suspending && self.entries.is_empty() {
            self.phase = WindowPhase::Suspended;
        }
    }
}
