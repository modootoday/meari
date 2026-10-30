//! Canonical Noto catalog from the verified download cache (fetched here if absent; never
//! skipped: no network and no cache is a failure).
use meari::domain::design::fonts::{FontRequirement, UI_SCRIPTS, select};
use meari_native::{
    application::Renderer,
    domain::Viewport,
    infrastructure::{
        engine::NativeRenderer,
        fonts::{FontCache, prepare_font_catalog, web_slices},
    },
};
use std::collections::BTreeSet;

fn cache() -> FontCache {
    let cache = FontCache::at(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/fonts"));
    cache.ensure(UI_SCRIPTS).unwrap();
    cache
}

#[test]
fn canonical_faces_pass_native_preflight() {
    let catalog = prepare_font_catalog(&cache(), FontRequirement::PinnedAssets).unwrap();
    let pins = select(UI_SCRIPTS).unwrap();
    assert_eq!(catalog.assets.manifest().files.len(), pins.len());
    for descriptor in catalog.assets.manifest().files.values() {
        let output = descriptor.output_path().unwrap();
        let (media_type, bytes) = catalog.assets.resource(&output).unwrap();
        assert_eq!(media_type, "font/ttf");
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        let family = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .find_map(|n| n.to_string())
            .unwrap();
        assert!(
            pins.iter().any(|p| family.starts_with(p.family)),
            "{family}"
        );
    }
    assert_eq!(catalog.licenses.len(), 7); // Noto Sans/Serif/Sans Mono, KR, JP (italics share)
    assert!(
        catalog
            .licenses
            .values()
            .all(|t| t.contains("SIL OPEN FONT LICENSE"))
    );
    let renderer =
        NativeRenderer::from_html_with_assets("<!doctype html><body></body>", catalog.assets)
            .unwrap();
    let report = renderer.asset_report().unwrap().unwrap();
    assert_eq!(report.rejected, 0);
    assert!(report.failures.is_empty());
}

#[test]
fn catalog_css_delivers_all_faces_headlessly() {
    let catalog = prepare_font_catalog(&cache(), FontRequirement::PinnedAssets).unwrap();
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
        #semibold{{font-weight:600}}#italic{{font-style:italic}}
        #jp{{font-family:"Noto Sans JP"}}#serif{{font-family:var(--font-serif)}}
        #serifi{{font-family:var(--font-serif);font-style:italic}}
        #kr{{font-family:"Noto Serif KR"}}#jps{{font-family:"Noto Serif JP"}}
        </style><body><div id="regular">Meari 한글</div><div id="semibold">Meari 한글</div>
        <div id="italic">Meari</div><div id="jp">日本語</div><div id="serif">Meari 명조</div>
        <div id="serifi">Meari</div><div id="kr">명조</div><div id="jps">明朝</div></body>"#,
        catalog.native_css,
    );
    let mut renderer = NativeRenderer::from_html_with_assets(&html, catalog.assets).unwrap();
    renderer
        .layout(
            &Viewport {
                width: 480,
                height: 260,
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
    for id in [
        "regular", "semibold", "italic", "jp", "serif", "serifi", "kr", "jps",
    ] {
        let bounds = renderer.bounds(id).unwrap();
        assert!(bounds.width > 0.0 && bounds.height > 0.0, "{id}");
    }
}

/// The Web slices of each canonical face partition exactly the cmap of the cached TTF native
/// renders, and every slice is a valid WOFF2.
#[test]
fn web_slices_cover_exactly_the_native_cmap() {
    let cache = cache();
    for pin in select(UI_SCRIPTS).unwrap() {
        let bytes = cache.load(&pin).unwrap();
        let parsed = ttf_parser::Face::parse(&bytes, 0).unwrap();
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
        for slice in web_slices(&cache, &pin).unwrap() {
            assert!(std::fs::read(&slice.path).unwrap().starts_with(b"wOF2"));
            for part in slice.unicode_range.split(',') {
                let part = part.strip_prefix("U+").unwrap();
                let (lo, hi) = part.split_once('-').unwrap_or((part, part));
                let lo = u32::from_str_radix(lo, 16).unwrap();
                let hi = u32::from_str_radix(hi, 16).unwrap();
                assert!(sliced.insert(lo), "{} overlaps at {lo:X}", pin.file);
                sliced.extend(lo + 1..=hi);
            }
        }
        assert_eq!(native, sliced, "{}", pin.file);
        if pin.family == "Noto Sans KR" {
            assert!((0xAC00..=0xD7A3).all(|cp| native.contains(&cp)));
        }
    }
}
