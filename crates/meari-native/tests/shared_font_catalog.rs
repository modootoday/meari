use meari::{
    application::fonts::prepare_font_catalog,
    domain::{
        build::BuildPath,
        design::fonts::{FontRequirement, UI_FACES},
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
        assert_eq!(media_type, "font/ttf");
        let face = ttf_parser::Face::parse(bytes, 0).unwrap();
        assert_eq!(face.weight().to_number(), expected.weight());
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
        </style><body><div id="regular">Meari 한글</div>
        <div id="semibold">Meari 한글</div><div id="jp">Meari 日本語</div></body>"#,
        catalog.css,
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
    for id in ["regular", "semibold", "jp"] {
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
