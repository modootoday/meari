//! A single GPU submission slot. Completion, not present-return, releases it.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(super) struct Flight {
    done: Arc<AtomicBool>,
    submitted: Option<Instant>,
}
impl Default for Flight {
    fn default() -> Self {
        Self {
            done: Arc::new(AtomicBool::new(true)),
            submitted: None,
        }
    }
}
impl Flight {
    pub fn busy(&self) -> bool {
        !self.done.load(Ordering::Acquire)
    }
    pub fn ready(&mut self, now: Instant) -> Result<bool, &'static str> {
        if !self.busy() {
            self.submitted = None;
            return Ok(true);
        }
        if self
            .submitted
            .is_some_and(|start| now.saturating_duration_since(start) > Duration::from_secs(2))
        {
            return Err("GPU completion timeout; renderer stopped (no CPU fallback)");
        }
        Ok(false)
    }
    pub fn begin(&mut self, now: Instant) -> Result<Arc<AtomicBool>, &'static str> {
        if self.busy() {
            return Err("GPU frame still in flight");
        }
        self.submitted = Some(now);
        self.done.store(false, Ordering::Release);
        Ok(self.done.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn present_return_does_not_release_the_single_slot() {
        let mut flight = Flight::default();
        let now = Instant::now();
        let callback = flight.begin(now).unwrap();
        assert!(!flight.ready(now).unwrap());
        assert!(flight.begin(now).is_err());
        callback.store(true, Ordering::Release);
        assert!(flight.ready(now).unwrap());
        assert!(flight.begin(now).is_ok());
    }
    #[test]
    fn timeout_is_an_error_and_never_authorizes_reuse() {
        let mut flight = Flight::default();
        let now = Instant::now();
        let _callback = flight.begin(now).unwrap();
        assert!(flight.ready(now + Duration::from_secs(3)).is_err());
        assert!(flight.busy());
        assert!(flight.begin(now).is_err());
    }
    #[test]
    fn callback_keeps_a_resource_alive_until_completion() {
        let mut flight = Flight::default();
        let done = flight.begin(Instant::now()).unwrap();
        let resource = Arc::new(());
        let retained = resource.clone();
        let callback = move || {
            drop(retained);
            done.store(true, Ordering::Release);
        };
        assert_eq!(Arc::strong_count(&resource), 2);
        assert!(flight.busy());
        callback();
        assert_eq!(Arc::strong_count(&resource), 1);
        assert!(!flight.busy());
    }
}
