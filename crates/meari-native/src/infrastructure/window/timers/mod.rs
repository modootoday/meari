//! Owned one-shot completion deadlines, distinct from failed-turn retry.
use super::NativeWindowWake;
use crate::infrastructure::native_host::{
    HostAccessError, HostLifecycle, HostStamp, NativeDocumentHandle,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

struct Entry {
    #[cfg(test)]
    owner: u64,
    scope: HostStamp,
    sequence: u64,
    deadline: Option<Instant>,
}
#[derive(Default)]
pub(super) struct TimerBook {
    #[cfg(test)]
    pub(super) trace: testing::Trace,
    next: u64,
    entries: BTreeMap<u64, Entry>,
    closed: bool,
}
fn same_owner(a: &HostStamp, b: &HostStamp) -> bool {
    a.host_id == b.host_id && a.owner == b.owner && a.definition_revision == b.definition_revision
}
impl TimerBook {
    fn register(&mut self, scope: HostStamp) -> Result<u64, HostAccessError> {
        if self.closed {
            return Err(HostAccessError::Closed);
        }
        if self.entries.len() >= 64 {
            return Err(HostAccessError::Busy);
        }
        let key = self
            .next
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        self.next = key;
        self.entries.insert(
            key,
            Entry {
                #[cfg(test)]
                owner: key,
                scope,
                sequence: 0,
                deadline: None,
            },
        );
        Ok(key)
    }
    fn arm(
        &mut self,
        key: u64,
        current: &HostStamp,
        now: Instant,
        delay: Duration,
    ) -> Result<u64, HostAccessError> {
        if self.closed {
            return Err(HostAccessError::Closed);
        }
        if delay < Duration::from_millis(1) || delay > Duration::from_secs(86400) {
            return Err(HostAccessError::Invalid(
                "timer delay must be within 1ms..=24h".into(),
            ));
        }
        let entry = self.entries.get_mut(&key).ok_or(HostAccessError::Stale)?;
        if !same_owner(&entry.scope, current) {
            entry.deadline = None;
            return Err(HostAccessError::Stale);
        }
        let deadline = now
            .checked_add(delay)
            .ok_or(HostAccessError::CounterExhausted)?;
        let sequence = entry
            .sequence
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        entry.sequence = sequence;
        entry.deadline = Some(deadline);
        Ok(sequence)
    }
    fn cancel(&mut self, key: u64, sequence: u64) -> bool {
        self.entries
            .get_mut(&key)
            .is_some_and(|entry| entry.sequence == sequence && entry.deadline.take().is_some())
    }
    pub(super) fn cancel_all(&mut self) {
        for entry in self.entries.values_mut() {
            entry.deadline = None;
        }
    }
    pub(super) fn close(&mut self) {
        self.closed = true;
        self.entries.clear();
    }
    /// Busy leaves an expired deadline retained without creating a past-deadline spin.
    pub(super) fn poll(
        &mut self,
        now: Instant,
        current: &HostStamp,
        busy: bool,
    ) -> (bool, Option<Instant>) {
        let mut fired = false;
        let mut next = None;
        for entry in self.entries.values_mut() {
            if !same_owner(&entry.scope, current) {
                entry.deadline = None;
                continue;
            }
            if let Some(deadline) = entry.deadline {
                if deadline <= now {
                    if !busy {
                        #[cfg(test)]
                        self.trace.fired(testing::Firing {
                            owner: entry.owner,
                            sequence: entry.sequence,
                            deadline,
                            now,
                            scope: current.clone(),
                        });
                        entry.deadline = None;
                        fired = true;
                    }
                } else {
                    next = Some(next.map_or(deadline, |old: Instant| old.min(deadline)));
                }
            }
        }
        (fired, next)
    }
}

/// Opaque cancellation identity for one arm; cannot cancel a replacement or another owner.
#[derive(Debug)]
pub struct NativeHostTimerToken {
    authority: Arc<()>,
    sequence: u64,
}
/// UI-thread owner of one original document/window timer; Drop cancels, never joins SDK work.
#[must_use = "Retain the original timer owner; dropping it cancels its deadline"]
pub struct NativeHostTimer {
    wake: NativeWindowWake,
    document: NativeDocumentHandle,
    key: u64,
    authority: Arc<()>,
}
impl NativeWindowWake {
    pub fn timer(
        &self,
        document: &NativeDocumentHandle,
    ) -> Result<NativeHostTimer, HostAccessError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(HostAccessError::Closed);
        }
        if self.host != document.id() {
            return Err(HostAccessError::Stale);
        }
        let scope = document.stamp()?;
        let key = self
            .timers
            .lock()
            .map_err(|_| HostAccessError::Busy)?
            .register(scope)?;
        Ok(NativeHostTimer {
            wake: self.clone(),
            document: document.clone(),
            key,
            authority: Arc::new(()),
        })
    }
}
impl NativeHostTimer {
    /// Replace only this owner's one-shot deadline. No callback/SDK operation runs here.
    pub fn schedule_after(&self, delay: Duration) -> Result<NativeHostTimerToken, HostAccessError> {
        if self.wake.closed.load(Ordering::Acquire) {
            return Err(HostAccessError::Closed);
        }
        if self.document.lifecycle() != HostLifecycle::Open {
            return Err(HostAccessError::Closing);
        }
        let scope = self.document.stamp()?;
        let sequence = self
            .wake
            .timers
            .lock()
            .map_err(|_| HostAccessError::Busy)?
            .arm(self.key, &scope, Instant::now(), delay)?;
        if let Err(error) = self.wake.send(super::wake::TIMER_UPDATE) {
            if let Ok(mut book) = self.wake.timers.lock() {
                book.cancel(self.key, sequence);
            }
            return Err(error);
        }
        Ok(NativeHostTimerToken {
            authority: self.authority.clone(),
            sequence,
        })
    }
    pub fn cancel(&self, token: &NativeHostTimerToken) -> Result<bool, HostAccessError> {
        if !Arc::ptr_eq(&self.authority, &token.authority) {
            return Err(HostAccessError::Stale);
        }
        let cancelled = self
            .wake
            .timers
            .lock()
            .map_err(|_| HostAccessError::Busy)?
            .cancel(self.key, token.sequence);
        if cancelled && !self.wake.closed.load(Ordering::Acquire) {
            self.wake.send(super::wake::TIMER_UPDATE)?;
        }
        Ok(cancelled)
    }
}
impl Drop for NativeHostTimer {
    fn drop(&mut self) {
        if let Ok(mut book) = self.wake.timers.lock() {
            book.entries.remove(&self.key);
        }
        if !self.wake.closed.load(Ordering::Acquire) {
            let _ = self.wake.send(super::wake::TIMER_UPDATE);
        }
    }
}
#[cfg(test)]
pub(super) mod testing;
#[cfg(test)]
mod tests;
