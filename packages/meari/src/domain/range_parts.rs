//! Opt-in visual geometry. A source profile is not callback or SDK authority.
pub const ATTRIBUTE: &str = "data-meari-range-parts";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeParts {
    Classic,
    Compact,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeGeometry {
    pub thumb_px: u8,
    pub track_px: u8,
    pub hit_px: u8,
}
impl RangeParts {
    pub fn parse(profile: Option<&str>) -> Result<Self, String> {
        match profile {
            None | Some("classic") => Ok(Self::Classic),
            Some("compact") => Ok(Self::Compact),
            _ => Err("unsupported range parts profile".into()),
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Compact => "compact",
        }
    }
    pub const fn geometry(self) -> RangeGeometry {
        match self {
            Self::Classic => RangeGeometry {
                thumb_px: 16,
                track_px: 4,
                hit_px: 24,
            },
            Self::Compact => RangeGeometry {
                thumb_px: 10,
                track_px: 3,
                hit_px: 32,
            },
        }
    }
}
impl RangeGeometry {
    pub fn pointer_fraction(self, width: f64, x: f64) -> Result<f64, String> {
        let travel = width - f64::from(self.thumb_px);
        if !width.is_finite() || !x.is_finite() || travel <= 0.0 {
            return Err("range track has no finite thumb travel".into());
        }
        Ok(((x - f64::from(self.thumb_px) / 2.0) / travel).clamp(0.0, 1.0))
    }
}
pub fn fraction_from_actual(value: f64, min: f64, max: f64) -> Result<f64, String> {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() {
        return Err("range actual value/bounds are not finite".into());
    }
    if max <= min {
        return Ok(0.0);
    }
    let scale = value.abs().max(min.abs()).max(max.abs()).max(1.0);
    let fraction = (value / scale - min / scale) / (max / scale - min / scale);
    if !fraction.is_finite() {
        return Err("range fraction is not finite".into());
    }
    Ok(fraction.clamp(0.0, 1.0))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_and_compact_geometry_share_visual_and_pointer_endpoints() {
        let default = RangeParts::parse(None).unwrap().geometry();
        assert_eq!(default.thumb_px, 16);
        assert_eq!(default.track_px, 4);
        assert_eq!(default.hit_px, 24);
        let compact = RangeParts::parse(Some("compact")).unwrap().geometry();
        assert_eq!(
            compact,
            RangeGeometry {
                thumb_px: 10,
                track_px: 3,
                hit_px: 32
            }
        );
        for (x, expected) in [(5.0, 0.0), (36.0, 0.5), (67.0, 1.0)] {
            assert_eq!(compact.pointer_fraction(72.0, x).unwrap(), expected);
        }
        assert_eq!(default.pointer_fraction(72.0, 8.0).unwrap(), 0.0);
        assert_eq!(default.pointer_fraction(72.0, 64.0).unwrap(), 1.0);
    }
    #[test]
    fn invalid_profiles_and_nonfinite_geometry_are_rejected() {
        assert!(RangeParts::parse(Some("10px")).is_err());
        let geometry = RangeParts::Compact.geometry();
        for (width, x) in [(10.0, 5.0), (f64::INFINITY, 0.0), (72.0, f64::NAN)] {
            assert!(geometry.pointer_fraction(width, x).is_err());
        }
    }
    #[test]
    fn actual_fraction_is_finite_bounded_and_handles_large_intervals() {
        assert_eq!(fraction_from_actual(50.0, 0.0, 100.0).unwrap(), 0.5);
        assert_eq!(fraction_from_actual(0.0, -1e308, 1e308).unwrap(), 0.5);
        assert_eq!(fraction_from_actual(3.0, 5.0, 5.0).unwrap(), 0.0);
        assert!(fraction_from_actual(f64::NAN, 0.0, 1.0).is_err());
    }
}
