//! Generic observation hints; never SDK work, terminal acknowledgement or cleanup receipts.
use super::HostAccessError;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRetry {
    OnWake,
    After(Duration),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostScheduleLimits {
    min: Duration,
    max: Duration,
    observations: usize,
}
impl HostScheduleLimits {
    pub fn new(
        min_delay: Duration,
        max_delay: Duration,
        max_observations: usize,
    ) -> Result<Self, HostAccessError> {
        if min_delay.is_zero() || max_delay < min_delay || max_observations == 0 {
            return Err(HostAccessError::Invalid(
                "positive bounded host observation limits required".into(),
            ));
        }
        Ok(Self {
            min: min_delay,
            max: max_delay,
            observations: max_observations,
        })
    }
    pub fn min_delay(self) -> Duration {
        self.min
    }
    pub fn max_delay(self) -> Duration {
        self.max
    }
    pub fn max_observations(self) -> usize {
        self.observations
    }
    #[cfg(feature = "window")]
    pub(crate) fn validate(self, delay: Duration) -> Result<(), HostAccessError> {
        if delay < self.min || delay > self.max {
            return Err(HostAccessError::Invalid(
                "host retry delay outside explicit limits".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(feature = "window")]
#[derive(Debug)]
pub(crate) struct HostAttemptError {
    pub error: HostAccessError,
    pub retry: Option<HostRetry>,
}
#[cfg(feature = "window")]
impl std::fmt::Display for HostAttemptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
#[cfg(test)]
mod tests;
