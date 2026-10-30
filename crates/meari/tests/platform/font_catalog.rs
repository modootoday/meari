#![cfg(feature = "bundled-fonts")]

use meari::{
    application::fonts::{
        FontCatalogError, face_bytes, han_serif_kr_slices, license_text, prepare_font_catalog,
        web_files,
    },
    domain::{
        build::{BuildPath, ContentHash, InputRole},
        design::fonts::{
            FontFace, FontFormat, FontRequirement, FontSupport, LICENSES, SANS_FONT_FAMILY,
            SERIF_FONT_FAMILY, UI_FACES, UI_FONT_FAMILY,
        },
    },
};
use std::path::Path;

const OFL: &str = "SIL OPEN FONT LICENSE Version 1.1";
const NATIVE_TOTAL: usize = 26_487_492;
const WEB_TOTAL: usize = 12_133_768;

#[test]
fn shared_catalog_supplies_exact_faces_and_content_addressed_css() {
    let prepared = prepare_font_catalog(FontRequirement::PinnedAssets).unwrap();
    assert_eq!(prepared.support, FontSupport::PinnedAssetsOnly);
    assert_eq!(prepared.assets.manifest().files.len(), UI_FACES.len());
    let web_count: usize = UI_FACES.iter().map(|face| web_files(*face).len()).sum();
    assert_eq!(prepared.web_assets.manifest().files.len(), web_count);
    let (mut native_total, mut web_total) = (0, 0);
    for face in UI_FACES {
        let native = face.native_file();
        let mut deliveries = vec![(
            native,
            None,
            face.family(),
            &prepared.assets,
            &prepared.native_css,
        )];
        for web in web_files(face) {
            deliveries.push((
                web.file,
                web.unicode_range,
                face.web_family(),
                &prepared.web_assets,
                &prepared.css,
            ));
        }
        for (file, range, family, assets, css) in deliveries {
            let source = BuildPath::try_from(file.path.to_owned()).unwrap();
            let descriptor = &assets.manifest().files[&source];
            assert_eq!(descriptor.role, InputRole::Font);
            assert_eq!(descriptor.length, file.length);
            assert_eq!(descriptor.content.as_str(), file.sha256);
            let output = descriptor.output_path().unwrap();
            let (mime, bytes) = assets.resource(&output).unwrap();
            assert_eq!(mime, file.format.media_type());
            assert_eq!(ContentHash::of(bytes), descriptor.content);
            let range = range
                .map(|r| format!("unicode-range:{r};"))
                .unwrap_or_default();
            assert!(css.contains(&format!(
                "font-family:\"{family}\";src:url(\"/{}\") format(\"{}\");font-style:normal;font-weight:{};{range}font-display:swap;",
                output.as_str(), file.format.css(), face.weight(),
            )), "{}", file.path);
            if file.format == FontFormat::Woff2 {
                web_total += bytes.len();
            } else {
                native_total += bytes.len();
            }
        }
    }
    assert_eq!(native_total, NATIVE_TOTAL);
    assert_eq!(web_total, WEB_TOTAL);
    // The Web stylesheet is WOFF2-only; TTF/OTF never reaches a browser.
    assert!(!prepared.css.contains("truetype") && !prepared.css.contains("opentype"));
    assert!(!prepared.native_css.contains("woff2\")"));
    // Modified slices never carry the reserved name 'Source' as their family.
    assert!(!prepared.css.contains("font-family:\"Source Han"));
    for css in [&prepared.css, &prepared.native_css] {
        assert!(css.contains(UI_FONT_FAMILY));
        assert!(css.contains(&format!("--font-sans:{SANS_FONT_FAMILY}")));
        assert!(css.contains(&format!("--font-serif:{SERIF_FONT_FAMILY}")));
        assert!(!css.contains("file:"));
    }
}

/// Slices partition the 23,126 cmap code points of the upstream OTF without overlap.
#[test]
fn han_serif_slices_partition_the_upstream_cmap() {
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0;
    for slice in han_serif_kr_slices() {
        let mut count = 0;
        for part in slice.unicode_range.split(',') {
            let part = part.strip_prefix("U+").unwrap();
            let (lo, hi) = part.split_once('-').unwrap_or((part, part));
            let (lo, hi) = (
                u32::from_str_radix(lo, 16).unwrap(),
                u32::from_str_radix(hi, 16).unwrap(),
            );
            for cp in lo..=hi {
                assert!(seen.insert(cp), "U+{cp:X} in two slices");
                count += 1;
            }
        }
        assert_eq!(count, slice.codepoints, "{}", slice.name);
        total += count;
    }
    assert_eq!(total, 23_126);
    assert_eq!(seen.range(0xAC00..=0xD7A3).count(), 11_172);
    for web in web_files(FontFace::SourceHanSerifKrRegular) {
        assert_eq!(&web.bytes[0..4], b"wOF2");
        assert_eq!(&web.bytes[4..8], b"OTTO");
    }
}

#[test]
fn canonical_stacks_resolve_pretendard_and_source_serif_first() {
    assert!(
        SANS_FONT_FAMILY.starts_with("\"Pretendard Variable\", Pretendard, \"Pretendard JP\", ")
    );
    assert!(SANS_FONT_FAMILY.ends_with("sans-serif"));
    let serif = [
        "\"Source Serif 4 Variable\"",
        "\"Source Han Serif KR\"",
        "\"Meari Serif KR\"",
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
fn woff2_headers_match_their_upstream_native_file() {
    for face in UI_FACES {
        let Some(file) = face.web_woff2() else {
            continue;
        };
        let woff2 = web_files(face)[0].bytes;
        assert_eq!(woff2.len() as u64, file.length);
        let native = face_bytes(face);
        assert_eq!(&woff2[0..4], b"wOF2", "{face:?}");
        assert_eq!(&woff2[4..8], &native[0..4], "flavor {face:?}");
        let be32 = |b: &[u8], at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
        let be16 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        assert_eq!(be32(woff2, 8) as usize, woff2.len(), "length {face:?}");
        let tables = be16(woff2, 12);
        // Upstream drops only DSIG (signature) when building WOFF2.
        assert!(tables > 0 && tables <= be16(native, 4), "{face:?}");
        assert!(woff2.len() < native.len());
    }
}

/// Every file under assets/fonts is a catalogued face or an OFL-1.1 notice; anything else
/// (a non-OFL license, an unlisted font such as Arimo/Tinos/Liberation) fails.
#[test]
fn every_bundled_font_is_ofl_and_catalogued() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let known_fonts = UI_FACES
        .iter()
        .flat_map(|face| {
            std::iter::once(face.native_file().path)
                .chain(web_files(*face).into_iter().map(|web| web.file.path))
        })
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
                .replace('\\', "/");
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
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        for name in license.reserved_names() {
            assert!(
                [format!("{name}."), format!("‘{name}’"), format!("'{name}'")]
                    .iter()
                    .any(|n| flat.contains(&format!("Reserved Font Name {n}"))),
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
        "Source Han Serif KR",
        "Meari Serif KR",
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

/// Every per-OS host fallback the platform adapter may choose is also named in the CSS stack,
/// so the Web (where the browser picks) and native agree. Execution on Windows/macOS is
/// unverified; this checks the code-level definition only.
#[test]
fn host_fallbacks_are_named_in_the_css_stacks() {
    use meari::domain::design::fonts::{SANS_FONT_FAMILY, SERIF_FONT_FAMILY};
    let named = |stack: &str, family: &str| {
        stack
            .split(',')
            .map(|item| item.trim().trim_matches('"'))
            .any(|item| item == family)
    };
    for host in meari_platform::all_adapters().map(|host| host.font_fallbacks()) {
        for family in host.sans {
            assert!(named(SANS_FONT_FAMILY, family), "sans stack lacks {family}");
        }
        for family in host.serif {
            assert!(
                named(SERIF_FONT_FAMILY, family),
                "serif stack lacks {family}"
            );
        }
    }
}
