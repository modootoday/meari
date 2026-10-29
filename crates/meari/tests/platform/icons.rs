use meari::{
    application::{Language, icons},
    domain::{
        Value,
        design::icons::{IconSize, IconSpec, IconStroke, IconToken, IconTone},
    },
};
use std::collections::BTreeMap;

#[test]
fn shared_registry_preserves_pinned_assets_geometry_and_semantic_attributes() {
    for (token, revision) in [
        (
            IconToken::SelectionCheck,
            "948fffb81c57cd3268aa4eb8fea1b4999468e52f833d39417149825a748d5273",
        ),
        (
            IconToken::ToggleThumb,
            "90dc73d64c56bab39a7892808e732ee707ce03fe20c2023573e180ef41fcd39a",
        ),
    ] {
        assert_eq!(icons::asset_revision(token), revision);
    }
    let rendered = icons::render(IconSpec {
        token: IconToken::SelectionCheck,
        size: IconSize::Lg,
        tone: IconTone::Primary,
        stroke: IconStroke::Bold,
    })
    .unwrap();
    assert_eq!(
        rendered,
        "<svg data-icon=\"selection-check\" class=\"size-6 text-primary\" xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 16 16\" stroke-width=\"3\" aria-hidden=\"true\" focusable=\"false\"><path d=\"m3 8 3 3 7-7\" fill=\"none\" stroke=\"currentColor\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>\n"
    );
    // These packaged core files are the registry's source, independent of the Native crate.
    use sha2::{Digest, Sha256};
    for (token, bytes) in [
        (
            IconToken::SelectionCheck,
            include_bytes!("../../assets/icons/selection-check.svg").as_slice(),
        ),
        (
            IconToken::ToggleThumb,
            include_bytes!("../../assets/icons/toggle-thumb.svg").as_slice(),
        ),
    ] {
        assert_eq!(
            icons::asset_revision(token),
            format!("{:x}", Sha256::digest(bytes))
        );
    }
    assert!(rendered.contains("viewBox=\"0 0 16 16\""));
    assert!(rendered.contains("stroke=\"currentColor\""));
    let mut filled = IconSpec::new(IconToken::ToggleThumb);
    filled.stroke = IconStroke::Bold;
    assert_eq!(
        icons::render(filled).unwrap_err(),
        "filled icon does not accept outline stroke tokens"
    );

    let source = meari::parse(
        "type Props={};export function Semantic(props:Props){return <Icon name=\"selection-check\" decorative={false} label=\"Selected\"/>;}",
        Language::Tsx,
    ).unwrap();
    let schema = meari::emit(&source, Language::Tsx).unwrap().schema_version;
    let mut attrs = BTreeMap::new();
    let (_, classes) = icons::from_node(&source.root, &mut attrs).unwrap();
    assert_eq!(attrs["role"], "img");
    assert_eq!(attrs["aria-label"], "Selected");
    assert!(!attrs.contains_key("aria-hidden"));
    assert_eq!(attrs["data-icon-token"], "selection-check");
    assert!(classes.contains("size-4"));
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&source, language).unwrap();
        assert_eq!(output.schema_version, schema);
        assert!(source.semantic_eq(&meari::parse(&output.code, language).unwrap()));
    }
    let mut invalid = source.clone();
    if let meari::domain::NodeKind::Element { attributes, .. } = &mut invalid.root.kind {
        attributes
            .iter_mut()
            .find(|attr| attr.name == "name")
            .unwrap()
            .value = Value::String("../../external.svg".into());
    }
    assert!(icons::from_node(&invalid.root, &mut BTreeMap::new()).is_err());
}
