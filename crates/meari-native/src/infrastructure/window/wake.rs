use crate::infrastructure::native_host::HostAccessError;
use blitz_shell::{BlitzShellEvent, BlitzShellProxy};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

pub(super) struct HostWakeEvent {
    pub host: u64,
}
pub(super) const COMPLETE: u8 = 1;
pub(super) const CLOSE: u8 = 2;
pub(super) const LIFECYCLE: u8 = 4;
pub(super) const RETRY: u8 = 8;
pub(super) const ACTIONS: u8 = 16;
pub(super) const TIMER_UPDATE: u8 = 32;
pub(super) const USER_CLOSE: u8 = 64;

struct LifecycleWake(NativeWindowWake);
impl std::task::Wake for LifecycleWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(LIFECYCLE);
    }
}

/// Only wake bits and an owner ID cross threads; no document, registry or GL object is sent.
#[derive(Clone)]
pub struct NativeWindowWake {
    pub(super) host: u64,
    pub(super) proxy: BlitzShellProxy,
    pub(super) pending: Arc<AtomicU8>,
    pub(super) closed: Arc<AtomicBool>,
    pub(super) timers: Arc<std::sync::Mutex<super::timers::TimerBook>>,
    pub(super) user_close: Arc<super::user_close::Requests>,
}
impl NativeWindowWake {
    pub(super) fn request_actions(&self) -> Result<bool, HostAccessError> {
        self.send(ACTIONS)
    }
    pub(super) fn lifecycle_waker(&self) -> std::task::Waker {
        std::task::Waker::from(Arc::new(LifecycleWake(self.clone())))
    }
    pub fn wake(&self) -> Result<bool, HostAccessError> {
        self.send(COMPLETE)
    }
    pub fn retry_failed_completion(&self) -> Result<bool, HostAccessError> {
        self.send(RETRY)
    }
    pub fn request_close(&self) -> Result<bool, HostAccessError> {
        self.send(CLOSE)
    }
    /// Scoped caption/user intent. Notification is not close or cleanup acknowledgement.
    /// The original UI loop validates the window scope again before can_close(User).
    pub fn request_user_close(
        &self,
        expected: crate::application::window_actions::ActionScope,
    ) -> Result<bool, crate::application::window_actions::ActionError> {
        use crate::application::window_actions::ActionError;
        if !self.user_close.admit(self.host, &self.closed, expected)? {
            return Ok(false);
        }
        self.send(USER_CLOSE).map_err(|_| ActionError::Closed)
    }
    pub(super) fn send(&self, bit: u8) -> Result<bool, HostAccessError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(HostAccessError::Closed);
        }
        let previous = self.pending.fetch_or(bit, Ordering::AcqRel);
        if previous == 0 {
            self.proxy
                .send_event(BlitzShellEvent::embedder_event(HostWakeEvent {
                    host: self.host,
                }));
        }
        Ok(previous == 0)
    }
    pub(super) fn take(&self) -> u8 {
        self.pending.swap(0, Ordering::AcqRel)
    }
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.user_close.clear();
        if let Ok(mut timers) = self.timers.lock() {
            timers.close();
        }
        self.pending.store(0, Ordering::Release);
    }
}
