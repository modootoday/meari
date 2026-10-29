use meari::{
    application::{Language, binding},
    domain::source_schema::SourceSchema,
};
use std::collections::BTreeMap;

#[test]
fn original_hook_order_remains_distinct_after_runtime_binding_erases_it() {
    let source = meari::parse(include_str!("../fixtures/mixed-hooks.tsx"), Language::Tsx).unwrap();
    let mut reordered = source.clone();
    reordered.hook_order.swap(0, 1);
    assert_eq!(
        binding::bind(&source, &BTreeMap::new()).unwrap(),
        binding::bind(&reordered, &BTreeMap::new()).unwrap()
    );
    assert_ne!(
        SourceSchema::from_component(&source)
            .unwrap()
            .fingerprint()
            .unwrap(),
        SourceSchema::from_component(&reordered)
            .unwrap()
            .fingerprint()
            .unwrap()
    );
}

#[test]
fn source_schema_ignores_body_initializers_and_spans_but_tracks_client_and_props() {
    let text = include_str!("../fixtures/state-counter.tsx");
    let source = meari::parse(text, Language::Tsx).unwrap();
    let next = meari::parse(
        &text
            .replace("useState(0)", "useState(100)")
            .replace("previous + 1", "previous + 10"),
        Language::Tsx,
    )
    .unwrap();
    let original = SourceSchema::from_component(&source).unwrap();
    assert_eq!(original, SourceSchema::from_component(&next).unwrap());
    let mut changed = source.clone();
    changed.client = !changed.client;
    assert_ne!(original, SourceSchema::from_component(&changed).unwrap());
    let mut changed = source;
    changed.props.push(meari::domain::Prop {
        name: "caption".into(),
        kind: meari::domain::PropType::String,
    });
    assert_ne!(original, SourceSchema::from_component(&changed).unwrap());
}
