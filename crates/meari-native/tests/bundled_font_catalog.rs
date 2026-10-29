use meari::{
    application::fonts::prepare_font_catalog,
    domain::{
        build::BuildPath,
        design::fonts::{FontFace, FontRequirement, UI_FACES},
    },
};
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::engine::NativeRenderer,
};
use std::collections::BTreeSet;

#[test]
fn common_font_faces_pass_native_preflight() {
    let catalog = prepare_font_catalog(FontRequirement::PinnedAssets).unwrap();
    assert_eq!(catalog.assets.manifest().files.len(), UI_FACES.len());
    for expected in UI_FACES {
        let source = BuildPath::try_from(expected.source_path().to_owned()).unwrap();
        let descriptor = &catalog.assets.manifest().files[&source];
        let output = descriptor.output_path().unwrap();
        let (media_type, bytes) = catalog.assets.resource(&output).unwrap();
        assert_eq!(media_type, expected.native_file().format.media_type());
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        assert_eq!(face.weight().to_number(), expected.default_weight());
        assert!(
            face.names().into_iter().any(|name| {
                matches!(
                    name.name_id,
                    ttf_parser::name_id::FAMILY | ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                ) && name.to_string().as_deref() == Some(expected.family())
            }),
            "missing family metadata: {}",
            expected.family()
        );
    }
    // The public constructor preflights every declared resource, even without CSS requests.
    let renderer =
        NativeRenderer::from_html_with_assets("<!doctype html><body></body>", catalog.assets)
            .unwrap();
    let report = renderer.asset_report().unwrap().unwrap();
    assert_eq!(report.rejected, 0);
    assert!(report.failures.is_empty());
}

#[test]
fn common_font_catalog_css_delivers_all_faces_headlessly() {
    let catalog = prepare_font_catalog(FontRequirement::PinnedAssets).unwrap();
    let expected = catalog
        .assets
        .manifest()
        .files
        .values()
        .map(|descriptor| descriptor.output_path().unwrap().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let html = format!(
        r#"<!doctype html><style>{}
        body{{margin:0;font-family:var(--meari-font-ui)}}
        #regular{{font-weight:400}}#semibold{{font-weight:600}}
        #jp{{font-family:"Pretendard JP";font-weight:400}}
        #jpbold{{font-family:"Pretendard JP";font-weight:700}}
        #serif{{font-family:var(--font-serif)}}
        </style><body><div id="regular">Meari 한글</div>
        <div id="semibold">Meari 한글</div><div id="jp">Meari 日本語</div>
        <div id="jpbold">Meari 日本語</div><div id="serif">Meari 명조 한글</div></body>"#,
        catalog.native_css,
    );
    let mut renderer = NativeRenderer::from_html_with_assets(&html, catalog.assets).unwrap();
    renderer
        .layout(
            &Viewport {
                width: 480,
                height: 160,
                dark: false,
            },
            0.0,
        )
        .unwrap();
    let report = renderer.asset_report().unwrap().unwrap();
    assert_eq!(report.rejected, 0);
    assert!(report.failures.is_empty());
    assert_eq!(
        report.delivered.keys().cloned().collect::<BTreeSet<_>>(),
        expected
    );
    assert!(report.delivered.values().all(|count| *count > 0));
    assert!(!renderer.document.has_pending_critical_resources());
    for id in ["regular", "semibold", "jp", "jpbold", "serif"] {
        let bounds = renderer.bounds(id).unwrap();
        assert!(
            [bounds.x, bounds.y, bounds.width, bounds.height]
                .iter()
                .all(|value| value.is_finite())
        );
        assert!(bounds.width > 0.0 && bounds.height > 0.0);
    }
    // Delivery and nonempty layout do not identify the selected font or certify glyphs.
}

/// Hangul, Latin and Japanese coverage of the canonical faces, read from the upstream TTF
/// (the WOFF2 of each face is checked against it by meari's font_catalog header test).
#[test]
fn canonical_faces_cover_hangul_latin_and_japanese() {
    let glyphs = |face: FontFace| {
        let bytes = meari::application::fonts::face_bytes(face);
        let parsed = ttf_parser::Face::parse(bytes, 0).unwrap();
        let count = parsed.number_of_glyphs();
        let covers = |text: &str| text.chars().all(|ch| parsed.glyph_index(ch).is_some());
        (
            count,
            covers("가힣한글"),
            covers("AZaz09"),
            covers("あんアン"),
            covers("日本語"),
        )
    };
    assert_eq!(
        glyphs(FontFace::PretendardVariable),
        (14_757, true, true, true, false)
    );
    assert_eq!(
        glyphs(FontFace::PretendardJpRegular),
        (22_936, true, true, true, true)
    );
    assert_eq!(
        glyphs(FontFace::PretendardJpBold),
        (22_936, true, true, true, true)
    );
    assert_eq!(
        glyphs(FontFace::SourceHanSerifKrRegular),
        (24_910, true, true, true, true)
    );
    assert_eq!(
        glyphs(FontFace::SourceSerif4Variable),
        (1_464, false, true, false, false)
    );
    // All 11,172 precomposed Hangul syllables in the sans canonical face.
    let bytes = meari::application::fonts::face_bytes(FontFace::PretendardVariable);
    let parsed = ttf_parser::Face::parse(bytes, 0).unwrap();
    assert!((0xAC00..=0xD7A3).all(|cp| parsed.glyph_index(char::from_u32(cp).unwrap()).is_some()));
    assert!(parsed.is_variable());
}

/// The renamed Web slices partition exactly the cmap of the upstream Source Han Serif KR OTF
/// that native renders, so Chrome (slices) and native (OTF) draw the same outlines' code points.
#[test]
fn han_serif_slices_cover_exactly_the_native_cmap() {
    let bytes = meari::application::fonts::face_bytes(FontFace::SourceHanSerifKrRegular);
    let parsed = ttf_parser::Face::parse(bytes, 0).unwrap();
    let mut native = BTreeSet::new();
    for table in parsed.tables().cmap.unwrap().subtables {
        if table.is_unicode() {
            table.codepoints(|cp| {
                if table.glyph_index(cp).is_some_and(|g| g.0 != 0) {
                    native.insert(cp);
                }
            });
        }
    }
    let mut sliced = BTreeSet::new();
    for slice in meari::application::fonts::han_serif_kr_slices() {
        for part in slice.unicode_range.split(',') {
            let part = part.strip_prefix("U+").unwrap();
            let (lo, hi) = part.split_once('-').unwrap_or((part, part));
            let lo = u32::from_str_radix(lo, 16).unwrap();
            let hi = u32::from_str_radix(hi, 16).unwrap();
            sliced.extend(lo..=hi);
        }
    }
    assert_eq!(native, sliced);
}
