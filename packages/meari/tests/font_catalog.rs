#![cfg(feature = "bundled-fonts")]

use meari::{
    application::fonts::{FontCatalogError, prepare_font_catalog},
    domain::{
        build::{BuildPath, ContentHash, InputRole},
        design::fonts::{FontLicense, FontRequirement, FontSupport, UI_FACES, UI_FONT_FAMILY},
    },
};

#[test]
fn shared_catalog_supplies_exact_faces_and_content_addressed_css() {
    let prepared = prepare_font_catalog(FontRequirement::PinnedAssets).unwrap();
    assert_eq!(prepared.support, FontSupport::PinnedAssetsOnly);
    assert_eq!(prepared.assets.manifest().files.len(), 3);
    let mut total = 0;
    for face in UI_FACES {
        let source = BuildPath::try_from(face.source_path().to_owned()).unwrap();
        let descriptor = &prepared.assets.manifest().files[&source];
        assert_eq!(descriptor.role, InputRole::Font);
        assert_eq!(descriptor.length, face.length());
        assert_eq!(descriptor.content.as_str(), face.sha256());
        let output = descriptor.output_path().unwrap();
        let (mime, bytes) = prepared.assets.resource(&output).unwrap();
        assert_eq!(mime, "font/ttf");
        assert_eq!(ContentHash::of(bytes), descriptor.content);
        assert_eq!(bytes.len() as u64, face.length());
        assert!(prepared.css.contains(&format!(
            "font-family:\"{}\";src:url(\"/{}\") format(\"truetype\");font-style:normal;font-weight:{};",
            face.family(), output.as_str(), face.weight(),
        )));
        total += bytes.len();
    }
    assert_eq!(total, 10_859_564);
    assert!(prepared.css.contains(UI_FONT_FAMILY));
    assert!(!prepared.css.contains("file:"));
    for license in [FontLicense::Pretendard, FontLicense::PretendardJp] {
        let text = prepared.license(license);
        assert_eq!(ContentHash::of(text.as_bytes()).as_str(), license.sha256());
        assert!(text.contains("Reserved Font Name"));
        assert!(text.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }
}

#[test]
fn unsupported_fallback_coverage_and_raster_guarantees_fail_explicitly() {
    for requirement in [
        FontRequirement::NoSystemFallback,
        FontRequirement::VerifiedGlyphCoverage,
        FontRequirement::DeterministicRaster,
    ] {
        assert!(matches!(
            prepare_font_catalog(requirement),
            Err(FontCatalogError::UnsupportedRequirement(actual)) if actual == requirement
        ));
    }
}
