use super::*;
use crate::net::{FontFaceOverrides, Resource, ResourceLoadResponse};

/// Regression-pin for the `@font-face` descriptor-honouring fix.
///
/// The bug was that `Resource::Font` carried only the raw font bytes,
/// so `load_resource` registered fonts with `info_override = None` and
/// parley fell back to the TTF's internal `name` table. After the fix,
/// `Resource::Font` carries `FontFaceOverrides` and `load_resource`
/// builds a `FontInfoOverride` from them — meaning a CSS-declared
/// `font-family` alias wins over the file's own metadata.
///
/// We drive `load_resource` directly with a fabricated response rather
/// than go through HTML parsing → `fetch_font_face`, because the
/// downstream HTML parser lives in `blitz-html` (would be a circular
/// crate dependency). The mapping from `@font-face` descriptors into
/// `FontFaceOverrides` is covered by the unit tests in `net.rs`; this
/// test pins the load-side of the pipeline.
#[test]
fn font_face_overrides_alias_family_name() {
    const ALIAS: &str = "AliasedFamily";

    let mut document = BaseDocument::new(DocumentConfig::default());

    // Sanity: the alias name is not registered before we feed the font.
    {
        let mut ctx = document.font_ctx.lock().unwrap();
        assert!(
            ctx.collection.family_id(ALIAS).is_none(),
            "alias must not exist before registration",
        );
    }

    // Drive `load_resource` with a `Resource::Font` whose overrides
    // assert the CSS-side family name. We use the bullet font as a
    // valid font payload — its internal `name` table is irrelevant to
    // the assertion; what matters is whether the override wins.
    let response = ResourceLoadResponse {
        request_id: 0,
        node_id: None,
        resolved_url: Some(String::from("test://aliased-family")),
        result: Ok(Resource::Font(
            blitz_traits::net::Bytes::from_static(crate::BULLET_FONT),
            FontFaceOverrides {
                family_name: Some(String::from(ALIAS)),
                weight: Some(800.0),
                style: Some(parley::fontique::FontStyle::Italic),
            },
        )),
    };
    document.load_resource(response);

    // The override must have taken effect: parley's `Collection` now
    // resolves the CSS-declared alias to a registered family.
    let mut ctx = document.font_ctx.lock().unwrap();
    let family_id = ctx
        .collection
        .family_id(ALIAS)
        .expect("CSS-declared family name should be registered as a family alias");
    let resolved_name = ctx
        .collection
        .family_name(family_id)
        .expect("family id should resolve back to a name");
    assert_eq!(
        resolved_name, ALIAS,
        "registered family should report the CSS-declared name, \
         not the font file's internal `name` table entry",
    );
}
