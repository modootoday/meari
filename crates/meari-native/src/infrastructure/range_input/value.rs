//! HTML range value rules, independent of playback and SDK policies.
#[derive(Clone, Copy, Debug)]
pub(super) struct RangeValue {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    step: Option<f64>,
    base: f64,
}
fn number(value: Option<&str>) -> Option<f64> {
    value?.parse::<f64>().ok().filter(|v| v.is_finite())
}
impl RangeValue {
    pub fn read(
        min: Option<&str>,
        max: Option<&str>,
        step: Option<&str>,
        value: Option<&str>,
    ) -> Result<Self, String> {
        let minimum = number(min).unwrap_or(0.0);
        let maximum = number(max).unwrap_or(100.0).max(minimum);
        if !(maximum - minimum).is_finite() {
            return Err("range interval exceeds finite arithmetic budget".into());
        }
        let mut range = Self {
            min: minimum,
            max: maximum,
            value: minimum,
            step: if step == Some("any") {
                None
            } else {
                Some(number(step).filter(|v| *v > 0.0).unwrap_or(1.0))
            },
            base: number(min).or_else(|| number(value)).unwrap_or(0.0),
        };
        let candidate = number(value).unwrap_or(minimum + (maximum - minimum) / 2.0);
        range.value = range.sanitize(candidate)?;
        Ok(range)
    }
    pub fn sanitize(self, candidate: f64) -> Result<f64, String> {
        if !candidate.is_finite() {
            return Err("range candidate is not finite".into());
        }
        let value = candidate.clamp(self.min, self.max);
        let Some(step) = self.step else {
            return Ok(value);
        };
        let ticks = (value - self.base) / step;
        if !ticks.is_finite() || ticks.abs() > 4_503_599_627_370_496.0 {
            return Err("range step exceeds exact tick budget".into());
        }
        let lower = self.base + ticks.floor() * step;
        let upper = self.base + ticks.ceil() * step;
        let selected = [lower, upper]
            .into_iter()
            .filter(|v| v.is_finite() && *v >= self.min && *v <= self.max)
            .min_by(|a, b| {
                (a - value)
                    .abs()
                    .total_cmp(&(b - value).abs())
                    .then_with(|| b.total_cmp(a))
            });
        selected.ok_or_else(|| "range has no representable aligned value".into())
    }
    pub fn fraction(self) -> f64 {
        if self.max == self.min {
            0.0
        } else {
            (self.value - self.min) / (self.max - self.min)
        }
    }
    pub fn pointer(self, fraction: f64) -> Result<f64, String> {
        self.sanitize(self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min))
    }
    pub fn key(self, key: &str) -> Result<Option<f64>, String> {
        let step = self
            .step
            .unwrap_or_else(|| ((self.max - self.min) / 100.0).max(f64::EPSILON));
        let candidate = match key {
            "ArrowRight" | "ArrowUp" => self.value + step,
            "ArrowLeft" | "ArrowDown" => self.value - step,
            "PageUp" => self.value + step * 10.0,
            "PageDown" => self.value - step * 10.0,
            "Home" => self.min,
            "End" => self.max,
            _ => return Ok(None),
        };
        self.sanitize(candidate).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_invalid_values_and_positive_ties_follow_range_rules() {
        assert_eq!(
            RangeValue::read(None, None, None, None).unwrap().value,
            50.0
        );
        assert_eq!(
            RangeValue::read(Some("0"), Some("100"), Some("20"), Some("50"))
                .unwrap()
                .value,
            60.0
        );
        assert_eq!(
            RangeValue::read(Some("4"), Some("2"), None, Some("NaN"))
                .unwrap()
                .value,
            4.0
        );
        assert_eq!(
            RangeValue::read(Some("0"), Some("9"), Some("2"), Some("100"))
                .unwrap()
                .value,
            8.0
        );
    }
    #[test]
    fn any_steps_pointer_and_keyboard_are_finite_and_bounded() {
        let range = RangeValue::read(Some("0"), Some("1"), Some("any"), Some("0.4")).unwrap();
        assert_eq!(range.pointer(0.25).unwrap(), 0.25);
        assert_eq!(range.key("Home").unwrap(), Some(0.0));
        assert_eq!(range.key("End").unwrap(), Some(1.0));
        assert_eq!(range.key("Tab").unwrap(), None);
        assert!(range.sanitize(f64::NAN).is_err());
    }
}
