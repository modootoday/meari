//! One pre-ready owner, one native target, one pending notification; no payload or window handle.
use crate::application::window_lifecycle::LifecycleError;
use std::{
    sync::{Arc, Mutex},
    task::{Wake, Waker},
};

struct State {
    target: Option<Waker>,
    bound: bool,
    pending: bool,
    closed: bool,
}
pub(super) struct ForwardingSeed {
    host: u64,
    state: Mutex<State>,
}
impl ForwardingSeed {
    pub fn new(host: u64) -> Arc<Self> {
        Arc::new(Self {
            host,
            state: Mutex::new(State {
                target: None,
                bound: false,
                pending: false,
                closed: false,
            }),
        })
    }
    pub fn bind_once(&self, host: u64, target: Waker) -> Result<(), LifecycleError> {
        let flush = {
            let mut state = self.state.lock().map_err(|_| LifecycleError::WrongState)?;
            if self.host != host {
                return Err(LifecycleError::Stale);
            }
            if state.closed {
                return Err(LifecycleError::Closed);
            }
            if state.bound {
                return Err(LifecycleError::WrongState);
            }
            state.bound = true;
            state.target = Some(target.clone());
            state.pending
        };
        // Never call a wake target while holding the seed mutex.
        if flush {
            target.wake_by_ref();
        }
        Ok(())
    }
    pub fn require_bound(&self, host: u64) -> Result<(), LifecycleError> {
        let state = self.state.lock().map_err(|_| LifecycleError::WrongState)?;
        if self.host != host {
            return Err(LifecycleError::Stale);
        }
        if state.closed {
            return Err(LifecycleError::Closed);
        }
        if !state.bound {
            return Err(LifecycleError::NotReady);
        }
        Ok(())
    }
    pub fn require_unbound(&self, host: u64) -> Result<(), LifecycleError> {
        let state = self.state.lock().map_err(|_| LifecycleError::WrongState)?;
        if self.host != host {
            return Err(LifecycleError::Stale);
        }
        if state.closed {
            return Err(LifecycleError::Closed);
        }
        if state.bound {
            return Err(LifecycleError::WrongState);
        }
        Ok(())
    }
    pub fn notify(&self) -> Result<bool, LifecycleError> {
        let target = {
            let mut state = self.state.lock().map_err(|_| LifecycleError::WrongState)?;
            if state.closed {
                return Err(LifecycleError::Closed);
            }
            if state.pending {
                return Ok(false);
            }
            state.pending = true;
            state.target.clone()
        };
        if let Some(target) = target {
            target.wake_by_ref();
        }
        Ok(true)
    }
    /// Called before observing retained terminal state, so concurrent notifications queue another turn.
    pub fn take(&self, host: u64) -> Result<bool, LifecycleError> {
        let mut state = self.state.lock().map_err(|_| LifecycleError::WrongState)?;
        if self.host != host {
            return Err(LifecycleError::Stale);
        }
        if state.closed {
            return Err(LifecycleError::Closed);
        }
        if !state.bound {
            return Err(LifecycleError::NotReady);
        }
        Ok(std::mem::replace(&mut state.pending, false))
    }
    pub fn close(&self) {
        let target = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.closed = true;
            state.pending = false;
            state.target.take()
        };
        drop(target);
    }
}
impl Wake for ForwardingSeed {
    fn wake(self: Arc<Self>) {
        let _ = self.notify();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn target() -> (Arc<Count>, Waker) {
        let count = Arc::new(Count(AtomicUsize::new(0)));
        (count.clone(), Waker::from(count))
    }
    #[test]
    fn early_notifications_flush_once_and_require_bound_before_ready() {
        let seed = ForwardingSeed::new(7);
        let (count, wake) = target();
        assert_eq!(seed.require_bound(7), Err(LifecycleError::NotReady));
        assert!(seed.notify().unwrap());
        for _ in 0..32 {
            assert!(!seed.notify().unwrap());
        }
        assert_eq!(seed.take(7), Err(LifecycleError::NotReady));
        seed.bind_once(7, wake).unwrap();
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        seed.require_bound(7).unwrap();
        assert!(seed.take(7).unwrap());
        assert!(!seed.take(7).unwrap());
        assert!(seed.notify().unwrap());
        assert_eq!(count.0.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn foreign_rebinding_and_closed_late_binding_preserve_owner() {
        let seed = ForwardingSeed::new(8);
        let (first, wake) = target();
        assert_eq!(seed.bind_once(9, wake.clone()), Err(LifecycleError::Stale));
        seed.bind_once(8, wake.clone()).unwrap();
        assert_eq!(seed.bind_once(8, wake), Err(LifecycleError::WrongState));
        let (other, wake) = target();
        assert_eq!(seed.bind_once(8, wake), Err(LifecycleError::WrongState));
        seed.notify().unwrap();
        assert_eq!(first.0.load(Ordering::SeqCst), 1);
        assert_eq!(other.0.load(Ordering::SeqCst), 0);
        seed.close();
        assert_eq!(seed.notify(), Err(LifecycleError::Closed));
        assert_eq!(seed.bind_once(8, target().1), Err(LifecycleError::Closed));
    }
    #[test]
    fn simultaneous_binding_and_notification_never_lose_the_pending_turn() {
        let seed = ForwardingSeed::new(9);
        let (count, wake) = target();
        let start = Arc::new(Barrier::new(2));
        let worker_seed = seed.clone();
        let worker_start = start.clone();
        let worker = thread::spawn(move || {
            worker_start.wait();
            worker_seed.notify().unwrap();
        });
        start.wait();
        seed.bind_once(9, wake).unwrap();
        worker.join().unwrap();
        assert!(seed.take(9).unwrap());
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert!(!seed.take(9).unwrap());
    }
    #[test]
    fn closed_unbound_seed_never_flushes_into_a_later_target() {
        let seed = ForwardingSeed::new(10);
        seed.notify().unwrap();
        seed.close();
        let (count, wake) = target();
        assert_eq!(seed.bind_once(10, wake), Err(LifecycleError::Closed));
        assert_eq!(count.0.load(Ordering::SeqCst), 0);
    }
}
