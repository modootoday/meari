#![cfg(feature = "bundled-fonts")]

use meari::{
    application::fonts::{FontCatalogError, face_bytes, license_text, prepare_font_catalog},
    domain::{
        build::{BuildPath, ContentHash, InputRole},
        design::fonts::{
            FontFormat, FontRequirement, FontSupport, LICENSES, SANS_FONT_FAMILY,
            SERIF_FONT_FAMILY, UI_FACES, UI_FONT_FAMILY,
        },
    },
};
use std::path::Path;

const OFL: &str = "SIL OPEN FONT LICENSE Version 1.1";

#[test]
fn shared_catalog_supplies_exact_faces_and_content_addressed_css() {
    let prepared = prepare_font_catalog(FontRequirement::PinnedAssets).unwrap();
    assert_eq!(prepared.support, FontSupport::PinnedAssetsOnly);
    assert_eq!(prepared.assets.manifest().files.len(), 3);
    assert_eq!(prepared.web_assets.manifest().files.len(), 3);
    let mut total = 0;
    for face in UI_FACES {
        for format in [FontFormat::Truetype, FontFormat::Woff2] {
            let file = face.file(format);
            let source = BuildPath::try_from(file.path.to_owned()).unwrap();
            let assets = match format {
                FontFormat::Woff2 => &prepared.web_assets,
                FontFormat::Truetype => &prepared.assets,
            };
            let descriptor = &assets.manifest().files[&source];
            assert_eq!(descriptor.role, InputRole::Font);
            assert_eq!(descriptor.length, file.length);
            assert_eq!(descriptor.content.as_str(), file.sha256);
            let output = descriptor.output_path().unwrap();
            let (mime, bytes) = assets.resource(&output).unwrap();
            assert_eq!(mime, format.media_type());
            assert_eq!(ContentHash::of(bytes), descriptor.content);
            let css = match format {
                FontFormat::Woff2 => &prepared.css,
                FontFormat::Truetype => &prepared.native_css,
            };
            assert!(css.contains(&format!(
                "font-family:\"{}\";src:url(\"/{}\") format(\"{}\");font-style:normal;font-weight:{};font-display:swap;",
                face.family(), output.as_str(), format.css(), face.weight(),
            )));
            total += bytes.len();
        }
    }
    assert_eq!(total, 17_911_940);
    // The Web stylesheet is WOFF2-only; TTF never reaches a browser.
    assert!(!prepared.css.contains("truetype"));
    assert!(!prepared.native_css.contains("woff2\")"));
    for css in [&prepared.css, &prepared.native_css] {
        assert!(css.contains(UI_FONT_FAMILY));
        assert!(css.contains(&format!("--font-sans:{SANS_FONT_FAMILY}")));
        assert!(css.contains(&format!("--font-serif:{SERIF_FONT_FAMILY}")));
        assert!(!css.contains("file:"));
    }
}

#[test]
fn canonical_stacks_resolve_pretendard_and_source_serif_first() {
    assert!(
        SANS_FONT_FAMILY
            .starts_with("\"Pretendard Variable\", Pretendard, \"Pretendard JP\", Arial")
    );
    assert!(SANS_FONT_FAMILY.ends_with("sans-serif"));
    let serif = [
        "\"Source Serif 4 Variable\"",
        "\"Source Han Serif KR\"",
        "\"Noto Serif KR\"",
        "serif",
    ]
    .map(|name| SERIF_FONT_FAMILY.find(name).unwrap());
    assert!(
        serif.windows(2).all(|pair| pair[0] < pair[1]),
        "{SERIF_FONT_FAMILY}"
    );
    for stack in [SANS_FONT_FAMILY, SERIF_FONT_FAMILY] {
        for excluded in ["Arimo", "Tinos", "Liberation"] {
            assert!(
                !stack.contains(excluded),
                "{excluded} belongs to the Chrome comparison path only"
            );
        }
    }
}

#[test]
fn woff2_headers_match_their_upstream_truetype() {
    for face in UI_FACES {
        let woff2 = face_bytes(face, FontFormat::Woff2);
        let ttf = face_bytes(face, FontFormat::Truetype);
        assert_eq!(&woff2[0..4], b"wOF2", "{face:?}");
        assert_eq!(&woff2[4..8], &ttf[0..4], "flavor {face:?}");
        let be32 = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
        let be16 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        assert_eq!(be32(woff2, 8) as usize, woff2.len(), "length {face:?}");
        let tables = be16(woff2, 12);
        // Upstream drops only DSIG (signature) when building WOFF2.
        assert!(tables > 0 && tables <= be16(ttf, 4), "{face:?}");
        assert!(woff2.len() < ttf.len());
    }
}

/// Every file under assets/fonts is a catalogued face or an OFL-1.1 notice; anything else
/// (a non-OFL license, an unlisted font such as Arimo/Tinos/Liberation) fails.
#[test]
fn every_bundled_font_is_ofl_and_catalogued() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let known_fonts = UI_FACES
        .iter()
        .flat_map(|face| [FontFormat::Truetype, FontFormat::Woff2].map(|f| face.file(f).path))
        .collect::<Vec<_>>();
    let known_licenses = LICENSES.map(|l| l.path());
    let mut seen = 0;
    for dir in std::fs::read_dir(root.join("assets/fonts")).unwrap() {
        for entry in std::fs::read_dir(dir.unwrap().path()).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let lower = relative.to_ascii_lowercase();
            for banned in ["arimo", "tinos", "liberation"] {
                assert!(!lower.contains(banned), "{relative} must not be bundled");
            }
            if known_fonts.contains(&relative.as_str()) {
                seen += 1;
            } else if known_licenses.contains(&relative.as_str()) {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(text.contains(OFL), "{relative} is not OFL-1.1");
                for other in [
                    "Apache License",
                    "GNU GENERAL PUBLIC",
                    "Ubuntu Font Licence",
                    "Bitstream",
                ] {
                    assert!(!text.contains(other), "{relative} carries {other}");
                }
            } else {
                panic!("{relative}: not a catalogued OFL face or notice");
            }
        }
    }
    assert_eq!(seen, known_fonts.len());
    for license in LICENSES {
        let text = license_text(license);
        assert_eq!(ContentHash::of(text.as_bytes()).as_str(), license.sha256());
        for name in license.reserved_names() {
            assert!(
                text.contains(&format!("Reserved Font Name {name}."))
                    || text.contains(&format!("Reserved Font Name ‘{name}’")),
                "{license:?} RFN {name}"
            );
        }
    }
    // One place collects holders and RFNs.
    let notice = std::fs::read_to_string(root.join("../../THIRD_PARTY_NOTICES.md")).unwrap();
    for needle in [
        "Kil Hyung-jin",
        "Adobe",
        "Reserved Font Name",
        "Pretendard JP",
        "‘Source’",
        OFL,
    ] {
        assert!(
            notice.contains(needle),
            "THIRD_PARTY_NOTICES.md lacks {needle}"
        );
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
