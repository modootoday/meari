use crate::domain::window_lifecycle::CleanupStamp;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Waker,
};

struct SignalState {
    active: AtomicBool,
    dirty: AtomicBool,
    wake: Waker,
}

/// A cross-thread notification only. It cannot acknowledge cleanup or access a native handle.
#[derive(Clone)]
pub struct CompletionSignal {
    stamp: CleanupStamp,
    state: Arc<SignalState>,
}
impl CompletionSignal {
    pub(super) fn new(stamp: CleanupStamp, wake: Waker) -> Self {
        Self {
            stamp,
            state: Arc::new(SignalState {
                active: AtomicBool::new(true),
                dirty: AtomicBool::new(false),
                wake,
            }),
        }
    }
    pub fn stamp(&self) -> CleanupStamp {
        self.stamp
    }
    /// The adapter must retain terminal state until the UI-thread observer consumes it.
    pub fn notify(&self) -> bool {
        if !self.state.active.load(Ordering::Acquire) {
            return false;
        }
        if self.state.dirty.swap(true, Ordering::AcqRel) {
            return false;
        }
        self.state.wake.wake_by_ref();
        true
    }
    pub(super) fn take(&self) -> bool {
        self.state.dirty.swap(false, Ordering::AcqRel)
    }
    pub(super) fn deactivate(&self) {
        self.state.active.store(false, Ordering::Release);
    }
}
