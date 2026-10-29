use meari::{
    application::{Language, binding},
    domain::{NodeKind, Value},
};
use std::collections::BTreeMap;

const SOURCE: &str = "type Props={canSave:boolean;busy:boolean;refreshRequired:boolean};export function Flags(props:Props){return <button disabled={!props.canSave || props.busy || props.refreshRequired} hidden={props.busy && !props.canSave}>save</button>;}";

#[test]
fn predicates_roundtrip_and_bind_the_complete_boolean_truth_table() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 7);
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let tsx = meari::emit(&component, Language::Tsx).unwrap();
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    for can_save in [false, true] {
        for busy in [false, true] {
            for refresh in [false, true] {
                let bound = binding::bind(
                    &component,
                    &BTreeMap::from([
                        ("canSave".into(), Value::Bool(can_save)),
                        ("busy".into(), Value::Bool(busy)),
                        ("refreshRequired".into(), Value::Bool(refresh)),
                    ]),
                )
                .unwrap();
                let NodeKind::Element { attributes, .. } = bound.root.kind else {
                    panic!()
                };
                assert_eq!(
                    attributes[0].value,
                    Value::Bool(!can_save || busy || refresh)
                );
                assert_eq!(attributes[1].value, Value::Bool(busy && !can_save));
            }
        }
    }
}

#[test]
fn original_actions_module_parses_without_rewriting_its_composite_guards() {
    let source = include_str!("../fixtures/array-callback-graph/components/proposal-actions.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
}

#[test]
fn predicates_reject_non_boolean_props_and_side_effects_at_original_ranges() {
    let source = "type Props={label:string};export function Wrong(props:Props){return <button disabled={props.label || true}/>;}";
    let error = meari::parse(source, Language::Tsx).unwrap_err();
    assert_eq!(&source[error.span.start..error.span.end], "props.label");
    let side_effect = "type Props={busy:boolean};export function Wrong(props:Props){return <button disabled={props.busy || fetch('x')}/>;}";
    let error = meari::parse(side_effect, Language::Tsx).unwrap_err();
    assert_eq!(&side_effect[error.span.start..error.span.end], "fetch('x')");
}
