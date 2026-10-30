use super::*;
use style::values::generics::font::{FeatureTagValue, FontSettings, FontTag};

/// Collect the (tag, value) pairs from a feature list for easy assertions.
fn pairs(features: &[parley::FontFeature]) -> Vec<([u8; 4], u16)> {
    features
        .iter()
        .map(|f| (f.tag.to_bytes(), f.value))
        .collect()
}

fn feature_settings(tags: &[(&[u8; 4], i32)]) -> stylo::FontFeatureSettings {
    FontSettings(
        tags.iter()
            .map(|(tag, value)| FeatureTagValue {
                tag: FontTag(u32::from_be_bytes(**tag)),
                value: *value,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

#[test]
fn ligatures_none_disables_all() {
    let mut out = Vec::new();
    font_variant_ligatures(stylo::FontVariantLigatures::NONE, &mut out);
    assert_eq!(
        pairs(&out),
        vec![
            (*b"liga", 0),
            (*b"clig", 0),
            (*b"dlig", 0),
            (*b"hlig", 0),
            (*b"calt", 0),
        ]
    );
}

#[test]
fn ligatures_normal_emits_nothing() {
    let mut out = Vec::new();
    font_variant_ligatures(stylo::FontVariantLigatures::NORMAL, &mut out);
    assert!(out.is_empty());
}

#[test]
fn ligatures_mixed_toggles() {
    let mut out = Vec::new();
    let input = stylo::FontVariantLigatures::COMMON_LIGATURES
        | stylo::FontVariantLigatures::NO_DISCRETIONARY_LIGATURES
        | stylo::FontVariantLigatures::HISTORICAL_LIGATURES;
    font_variant_ligatures(input, &mut out);
    assert_eq!(
        pairs(&out),
        vec![(*b"liga", 1), (*b"clig", 1), (*b"dlig", 0), (*b"hlig", 1)]
    );
}

#[test]
fn caps_small_caps() {
    let mut out = Vec::new();
    font_variant_caps(stylo::FontVariantCaps::SmallCaps, &mut out);
    assert_eq!(pairs(&out), vec![(*b"smcp", 1)]);

    let mut out = Vec::new();
    font_variant_caps(stylo::FontVariantCaps::Normal, &mut out);
    assert!(out.is_empty());
}

#[test]
fn position_sub_and_super() {
    let mut out = Vec::new();
    font_variant_position(stylo::FontVariantPosition::Sub, &mut out);
    assert_eq!(pairs(&out), vec![(*b"subs", 1)]);

    let mut out = Vec::new();
    font_variant_position(stylo::FontVariantPosition::Super, &mut out);
    assert_eq!(pairs(&out), vec![(*b"sups", 1)]);
}

#[test]
fn numeric_features() {
    let mut out = Vec::new();
    let input = stylo::FontVariantNumeric::OLDSTYLE_NUMS
        | stylo::FontVariantNumeric::TABULAR_NUMS
        | stylo::FontVariantNumeric::SLASHED_ZERO;
    font_variant_numeric(input, &mut out);
    assert_eq!(
        pairs(&out),
        vec![(*b"onum", 1), (*b"tnum", 1), (*b"zero", 1)]
    );
}

#[test]
fn east_asian_features() {
    let mut out = Vec::new();
    let input =
        stylo::FontVariantEastAsian::JIS04 | stylo::FontVariantEastAsian::PROPORTIONAL_WIDTH;
    font_variant_east_asian(input, &mut out);
    assert_eq!(pairs(&out), vec![(*b"jp04", 1), (*b"pwid", 1)]);
}

#[test]
fn feature_settings_map_directly() {
    let mut out = Vec::new();
    font_feature_settings(&feature_settings(&[(b"smcp", 1), (b"tnum", 0)]), &mut out);
    assert_eq!(pairs(&out), vec![(*b"smcp", 1), (*b"tnum", 0)]);
}

/// `font-feature-settings` has a higher precedence than the `font-variant-*`
/// properties, so its entries must be appended *after* the variant-derived ones.
/// Parley stably sorts by tag and HarfBuzz applies the last of any duplicate tags,
/// so the trailing `font-feature-settings` value wins.
#[test]
fn feature_settings_override_variant_features() {
    let mut out = Vec::new();
    // font-variant-numeric: tabular-nums => tnum=1
    font_variant_numeric(stylo::FontVariantNumeric::TABULAR_NUMS, &mut out);
    // font-feature-settings: "tnum" 0 (should override the variant)
    font_feature_settings(&feature_settings(&[(b"tnum", 0)]), &mut out);

    let flat = pairs(&out);
    // Both are present, and the feature-settings value comes last.
    assert_eq!(flat, vec![(*b"tnum", 1), (*b"tnum", 0)]);
    let last_tnum = flat.iter().rev().find(|(tag, _)| tag == b"tnum").unwrap();
    assert_eq!(last_tnum.1, 0);
}
