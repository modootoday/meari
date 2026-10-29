use crate::infrastructure::native_host::{HostAccessError, HostRetry, HostScheduleLimits};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct HostSchedule {
    pub dirty: bool,
    pub failed: bool,
    pub deadline: Option<Duration>,
    attempts: usize,
}
impl HostSchedule {
    pub fn absolute_deadline(
        &mut self,
        origin: std::time::Instant,
    ) -> Result<Option<std::time::Instant>, HostAccessError> {
        let Some(relative) = self.deadline else {
            return Ok(None);
        };
        match origin.checked_add(relative) {
            Some(deadline) => Ok(Some(deadline)),
            None => {
                self.fail();
                Err(HostAccessError::CounterExhausted)
            }
        }
    }
    pub fn defer_after(
        &mut self,
        clock: &dyn crate::application::window_lifecycle::MonotonicClock,
        retry: HostRetry,
        limits: Option<HostScheduleLimits>,
    ) -> Result<(), HostAccessError> {
        self.defer(clock.now(), retry, limits)
    }
    pub fn ready_at(&self, now: Duration, explicit: bool) -> bool {
        explicit || (!self.failed && self.deadline.is_none_or(|deadline| deadline <= now))
    }
    pub fn begin(&mut self, explicit: bool) -> bool {
        if explicit {
            self.failed = false;
            self.attempts = 0;
        }
        if self.failed {
            return false;
        }
        self.dirty = true;
        self.deadline = None;
        true
    }
    pub fn complete(&mut self) {
        *self = Self::default();
    }
    pub fn fail(&mut self) {
        self.dirty = true;
        self.failed = true;
        self.deadline = None;
    }
    pub fn defer(
        &mut self,
        now: Duration,
        retry: HostRetry,
        limits: Option<HostScheduleLimits>,
    ) -> Result<(), HostAccessError> {
        self.dirty = true;
        self.deadline = None;
        if let HostRetry::After(delay) = retry {
            let result = (|| {
                let limits = limits.ok_or_else(|| {
                    HostAccessError::Invalid(
                        "timed host retry requires explicit schedule limits".into(),
                    )
                })?;
                limits.validate(delay)?;
                if self.attempts >= limits.max_observations() {
                    return Err(HostAccessError::Invalid(
                        "host observation budget exhausted; owner retained".into(),
                    ));
                }
                let deadline = now
                    .checked_add(delay)
                    .ok_or(HostAccessError::CounterExhausted)?;
                self.attempts += 1;
                Ok(deadline)
            })();
            match result {
                Ok(deadline) => self.deadline = Some(deadline),
                Err(error) => {
                    self.fail();
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub fn due(&self, now: Duration) -> bool {
        !self.failed && self.deadline.is_some_and(|deadline| deadline <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_instant_overflow_does_not_silently_discard_a_retained_deadline() {
        let mut state = HostSchedule::default();
        let limits = HostScheduleLimits::new(Duration::from_millis(1), Duration::MAX, 2).unwrap();
        state
            .defer(
                Duration::ZERO,
                HostRetry::After(Duration::MAX),
                Some(limits),
            )
            .unwrap();
        assert!(state.absolute_deadline(std::time::Instant::now()).is_err());
        assert!(state.failed && state.dirty);
        assert!(state.deadline.is_none());
    }
    #[test]
    fn delay_is_measured_after_callback_work_not_from_its_start() {
        use std::{cell::Cell, rc::Rc};
        #[derive(Clone)]
        struct Clock(Rc<Cell<Duration>>);
        impl crate::application::window_lifecycle::MonotonicClock for Clock {
            fn now(&self) -> Duration {
                self.0.get()
            }
        }
        let clock = Clock(Rc::new(Cell::new(Duration::ZERO)));
        let mut state = HostSchedule::default();
        let work = || {
            clock.0.set(Duration::from_secs(5));
            HostRetry::After(Duration::from_millis(2))
        };
        state.defer_after(&clock, work(), Some(limits())).unwrap();
        assert_eq!(state.deadline, Some(Duration::from_millis(5002)));
        assert!(!state.due(Duration::from_secs(5)));
    }
    fn limits() -> HostScheduleLimits {
        HostScheduleLimits::new(Duration::from_millis(2), Duration::from_secs(1), 2).unwrap()
    }
    #[test]
    fn future_retry_is_bounded_and_duplicate_hints_do_not_reset_budget() {
        let mut state = HostSchedule::default();
        assert!(state.begin(false));
        state
            .defer(
                Duration::ZERO,
                HostRetry::After(Duration::from_millis(2)),
                Some(limits()),
            )
            .unwrap();
        assert!(!state.due(Duration::from_millis(1)));
        assert!(state.due(Duration::from_millis(2)));
        state.begin(false);
        state
            .defer(
                Duration::from_millis(2),
                HostRetry::After(Duration::from_millis(2)),
                Some(limits()),
            )
            .unwrap();
        state.begin(false);
        assert!(
            state
                .defer(
                    Duration::from_millis(4),
                    HostRetry::After(Duration::from_millis(2)),
                    Some(limits())
                )
                .is_err()
        );
        assert!(state.failed);
        assert!(!state.begin(false));
        assert!(state.begin(true));
    }
    #[test]
    fn busy_retains_dirty_without_timer_and_true_failure_requires_explicit_retry() {
        let mut state = HostSchedule::default();
        state.begin(false);
        state
            .defer(Duration::ZERO, HostRetry::OnWake, None)
            .unwrap();
        assert!(state.dirty);
        assert_eq!(state.deadline, None);
        state.fail();
        assert!(!state.begin(false));
        assert!(state.begin(true));
        state.complete();
        assert!(!state.dirty);
    }
    #[test]
    fn zero_out_of_budget_and_overflow_are_not_acknowledgements() {
        for delay in [Duration::ZERO, Duration::from_secs(2)] {
            let mut state = HostSchedule::default();
            assert!(
                state
                    .defer(Duration::ZERO, HostRetry::After(delay), Some(limits()))
                    .is_err()
            );
            assert!(state.dirty && state.failed);
        }
        let mut state = HostSchedule::default();
        assert!(
            state
                .defer(
                    Duration::MAX,
                    HostRetry::After(Duration::from_millis(2)),
                    Some(limits())
                )
                .is_err()
        );
    }
}
