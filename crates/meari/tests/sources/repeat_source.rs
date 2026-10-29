use meari::{
    application::{Language, binding},
    domain::{NodeKind, Value},
    runtime::IntoValue,
};
use std::collections::BTreeMap;
const SOURCE: &str = "type Props={refs:string[]};export function List(props:Props){return <ul>{props.refs.map(reference => <li key={reference}>{reference}</li>)}</ul>;}";

#[test]
fn keyed_map_roundtrips_without_flattening_the_authoring_source() {
    let source = meari::parse(SOURCE, Language::Tsx).unwrap();
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&source, language).unwrap();
        assert_eq!(output.schema_version, 9);
        assert!(source.semantic_eq(&meari::parse(&output.code, language).unwrap()));
    }
    let NodeKind::Element { children, .. } = &source.root.kind else {
        panic!()
    };
    assert!(matches!(children[0].kind, NodeKind::Text(Value::Repeat(_))));
}

fn rows(values: Vec<&str>) -> Vec<(String, String)> {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let bound = binding::bind(
        &component,
        &BTreeMap::from([("refs".into(), values.into_value())]),
    )
    .unwrap();
    let NodeKind::Element { children, .. } = bound.root.kind else {
        panic!()
    };
    children
        .into_iter()
        .map(|child| {
            let NodeKind::Element {
                attributes,
                children,
                ..
            } = child.kind
            else {
                panic!()
            };
            let Value::String(identity) = &attributes
                .iter()
                .find(|attribute| attribute.name == meari::application::repeat::IDENTITY_ATTRIBUTE)
                .unwrap()
                .value
            else {
                panic!()
            };
            let NodeKind::Text(Value::String(text)) = &children[0].kind else {
                panic!()
            };
            (identity.clone(), text.clone())
        })
        .collect()
}

#[test]
fn projection_retains_keys_and_all_unicode_values_during_order_and_length_changes() {
    let before = rows(vec!["cue-a", "원문🦀"]);
    let after = rows(vec!["원문🦀", "new", "cue-a"]);
    assert_eq!(before[0], after[2]);
    assert_eq!(before[1], after[0]);
    assert_eq!(rows(vec!["cue-a"])[0], before[0]);
    assert!(rows(vec![]).is_empty());
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    assert!(
        binding::bind(
            &component,
            &BTreeMap::from([("refs".into(), vec!["same", "same"].into_value())])
        )
        .is_err()
    );
}

#[test]
fn unkeyed_and_non_lexical_maps_are_rejected() {
    for source in [
        SOURCE.replace(" key={reference}", ""),
        SOURCE.replace("key={reference}", "key={'fixed'}"),
        SOURCE.replace("string[]", "boolean[]"),
    ] {
        assert!(meari::parse(&source, Language::Tsx).is_err());
    }
}

#[test]
fn repeated_source_components_keep_their_state_owner_after_reordering() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{build::BuildPath, component_state::StateValue},
    };
    let entry: BuildPath = "list.tsx".to_owned().try_into().unwrap();
    let graph = meari::parse_module_graph(&entry, &BTreeMap::from([
        (entry.clone(), "import { Counter } from './counter';type Props={refs:string[]};export function List(props:Props){return <ul>{props.refs.map(reference => <Counter key={reference} label={reference}/>)}</ul>;}".into()),
        ("counter.tsx".to_owned().try_into().unwrap(), "import {useState} from 'react';type Props={label:string};export function Counter(props:Props){const [count,setCount]=useState(0);return <li><button onClick={()=>setCount(previous=>previous+1)}><span>{props.label}</span>{count}</button></li>;}".into()),
    ])).unwrap();
    let values = |refs: Vec<&str>| BTreeMap::from([("refs".into(), refs.into_value())]);
    let mut program =
        ModuleProgram::mount(graph, values(vec!["a", "b"]), "repeated-state".into(), 1).unwrap();
    let activated = program.activate("0/0/0").unwrap();
    let (identity, owner) = activated
        .instances
        .iter()
        .find(|(_, state)| {
            state.values.get("count")
                == Some(&StateValue::Number(
                    serde_json::Number::from_f64(1.0).unwrap(),
                ))
        })
        .map(|(identity, state)| (identity.clone(), state.instance.clone()))
        .unwrap();
    let reordered = program.update_props(values(vec!["b", "new", "a"])).unwrap();
    assert_eq!(reordered.instances[&identity].instance, owner);
    assert_eq!(
        reordered.instances[&identity].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    let activated = program.activate("0/2/0").unwrap();
    assert_eq!(
        activated.instances[&identity].values["count"],
        StateValue::Number(serde_json::Number::from_f64(2.0).unwrap())
    );
    let deleted = program.update_props(values(vec!["b"])).unwrap();
    assert!(!deleted.instances.contains_key(&identity));
}

#[test]
fn nested_templates_preserve_outer_capture_and_inner_shadowing() {
    for (binding, expected) in [
        ("inner", "outer-valueinner-value"),
        ("outer", "inner-valueinner-value"),
    ] {
        let source = format!(
            "type Props={{outer:string[];inner:string[]}};export function Nested(props:Props){{return <ul>{{props.outer.map(outer => <li key={{outer}}><ul>{{props.inner.map({binding} => <li key={{{binding}}}><span>{{outer}}</span>{{{binding}}}</li>)}}</ul></li>)}}</ul>;}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let generated = meari::emit(&component, Language::Rust).unwrap();
        assert!(component.semantic_eq(&meari::parse(&generated.code, Language::Rust).unwrap()));
        let bound = binding::bind(
            &component,
            &BTreeMap::from([
                ("outer".into(), vec!["outer-value"].into_value()),
                ("inner".into(), vec!["inner-value"].into_value()),
            ]),
        )
        .unwrap();
        fn text(node: &meari::domain::Node, output: &mut String) {
            match &node.kind {
                NodeKind::Text(Value::String(value)) => output.push_str(value),
                NodeKind::Element { children, .. } => {
                    for child in children {
                        text(child, output);
                    }
                }
                _ => panic!("unresolved repeat value"),
            }
        }
        let mut actual = String::new();
        text(&bound.root, &mut actual);
        assert_eq!(actual, expected);
    }
}
