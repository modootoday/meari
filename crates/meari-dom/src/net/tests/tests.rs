use super::*;
use parley::fontique::FontStyle as Fq;
use style::values::specified::Angle;

fn oblique(min_deg: f32, max_deg: f32) -> FontStyleRange {
    FontStyleRange::Oblique(Angle::from_degrees(min_deg), Angle::from_degrees(max_deg))
}

#[test]
fn italic_maps_to_italic() {
    assert_eq!(stylo_to_fontique_style(&FontStyleRange::Italic), Fq::Italic,);
}

#[test]
fn oblique_zero_zero_maps_to_normal() {
    // Stylo parses bare CSS `normal` as `Oblique(0deg, 0deg)`; the
    // helper must round-trip that back to `FontStyle::Normal` so
    // parley's matching doesn't misclassify upright fonts.
    assert_eq!(stylo_to_fontique_style(&oblique(0.0, 0.0)), Fq::Normal);
}

#[test]
fn oblique_single_angle_maps_to_oblique_with_min() {
    assert_eq!(
        stylo_to_fontique_style(&oblique(14.0, 14.0)),
        Fq::Oblique(Some(14.0)),
    );
}

#[test]
fn oblique_range_uses_min_angle() {
    // For a range, fontique's single-angle representation takes the
    // lower bound — confirm `min` (not `max`) is what gets through.
    assert_eq!(
        stylo_to_fontique_style(&oblique(10.0, 20.0)),
        Fq::Oblique(Some(10.0)),
    );
}
