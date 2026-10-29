use meari::{
    application::{Language, binding, repeat::IDENTITY_ATTRIBUTE},
    domain::{
        Component, NodeKind, PropType, Value, array_value::ArrayValue, record_value::RecordValue,
    },
};
use std::collections::BTreeMap;

mod typed_fixture {
    include!("../fixtures/record-props/cards.rs");
}

const SOURCE: &str = r#"type Props={items:{key:string;number:string;front:string;back:string;label:string}[]};
export function Cards(props:Props){return <ul>{props.items.map(row => <li key={row.key} data-key={row.key}><span>{row.number}</span><span>{row.front}</span><span>{row.back}</span><span>{row.label}</span></li>)}</ul>;}"#;

fn items(component: &Component, rows: &[(&str, &str)]) -> Value {
    let PropType::Array(element) = &component.props[0].kind else {
        panic!("array expected")
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!("record expected")
    };
    Value::Array(ArrayValue {
        element: element.as_ref().clone(),
        items: rows
            .iter()
            .enumerate()
            .map(|(index, (key, front))| {
                Value::Record(
                    RecordValue::new(
                        schema.clone(),
                        BTreeMap::from([
                            ("key".into(), Value::String((*key).into())),
                            ("number".into(), Value::String(index.to_string())),
                            ("front".into(), Value::String((*front).into())),
                            ("back".into(), Value::String("原文🦀".into())),
                            ("label".into(), Value::String(format!("카드 {key}"))),
                        ]),
                    )
                    .unwrap(),
                )
            })
            .collect(),
    })
}

fn project(component: &Component, rows: &[(&str, &str)]) -> Component {
    binding::bind(
        component,
        &BTreeMap::from([("items".into(), items(component, rows))]),
    )
    .unwrap()
}

fn identities(component: &Component) -> BTreeMap<String, String> {
    let NodeKind::Element { children, .. } = &component.root.kind else {
        panic!("root expected")
    };
    children
        .iter()
        .map(|child| {
            let NodeKind::Element { attributes, .. } = &child.kind else {
                panic!("row expected")
            };
            let string = |name| {
                let Value::String(value) = &attributes
                    .iter()
                    .find(|attribute| attribute.name == name)
                    .unwrap()
                    .value
                else {
                    panic!("resolved string expected")
                };
                value.clone()
            };
            (string("data-key"), string(IDENTITY_ATTRIBUTE))
        })
        .collect()
}

#[test]
fn actual_card_record_source_roundtrips_through_named_rust_and_tsx() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        assert_eq!(output.schema_version, 15);
        assert!(component.semantic_eq(&meari::parse(&output.code, language).unwrap()));
    }
    let output = meari::emit(&component, Language::Rust).unwrap();
    let tampered = output
        .code
        .replace("self.front.into_value()", "self.back.into_value()");
    assert_ne!(output.code, tampered);
    assert!(meari::parse(&tampered, Language::Rust).is_err());
}

#[test]
fn canonical_record_conversion_and_member_construction_compile_as_real_rust() {
    let source = meari::parse(SOURCE, Language::Tsx).unwrap();
    let fixture = meari::parse(
        include_str!("../fixtures/record-props/cards.rs"),
        Language::Rust,
    )
    .unwrap();
    assert!(source.semantic_eq(&fixture));
    let node = typed_fixture::Cards(typed_fixture::Props {
        items: vec![typed_fixture::CardRow {
            key: "real".into(),
            number: "1".into(),
            front: "앞면🦀".into(),
            back: "原文".into(),
            label: "카드".into(),
        }],
    });
    let mut component = fixture;
    component.props.clear();
    component.root = node;
    let bound = binding::bind(&component, &BTreeMap::new()).unwrap();
    assert_eq!(
        identities(&bound).keys().cloned().collect::<Vec<_>>(),
        vec!["real"]
    );
    assert!(format!("{:?}", meari::application::dom::prepare(&bound).unwrap()).contains("앞면🦀"));
}

#[test]
fn record_key_identity_survives_order_content_and_length_changes_without_flattening() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let before = identities(&project(&component, &[("a", "A"), ("b", "B")]));
    let after = identities(&project(
        &component,
        &[("b", "변경🦀"), ("new", "新"), ("a", "A")],
    ));
    assert_eq!(before["a"], after["a"]);
    assert_eq!(before["b"], after["b"]);
    assert_eq!(
        identities(&project(&component, &[("a", "A")])),
        BTreeMap::from([("a".into(), before["a"].clone())])
    );
    assert!(identities(&project(&component, &[])).is_empty());
    let bound = project(&component, &[("a", "앞면🦀")]);
    let dom = meari::application::dom::prepare(&bound).unwrap();
    let rendered = format!("{dom:?}");
    assert!(rendered.contains("앞면🦀"));
    assert!(rendered.contains("原文🦀"));
    assert!(rendered.contains("카드 a"));
    assert!(
        meari::emit(&component, Language::Tsx)
            .unwrap()
            .code
            .contains("row.front")
    );
}

#[test]
fn duplicate_keys_wrong_shapes_and_cross_scope_members_fail_before_projection() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    assert!(
        binding::bind(
            &component,
            &BTreeMap::from([(
                "items".into(),
                items(&component, &[("same", "A"), ("same", "B")])
            )])
        )
        .is_err()
    );
    let mut invalid = items(&component, &[("a", "A")]);
    let Value::Array(array) = &mut invalid else {
        panic!()
    };
    let Value::Record(row) = &mut array.items[0] else {
        panic!()
    };
    row.fields.insert("front".into(), Value::Bool(true));
    assert!(binding::bind(&component, &BTreeMap::from([("items".into(), invalid)])).is_err());
    for source in [
        SOURCE.replace("row.front", "row.absent"),
        SOURCE.replace("key={row.key}", "key={props.key}"),
        SOURCE.replace("key:string", "key:boolean"),
        SOURCE.replace("front:string", "front?:string"),
        SOURCE.replace("row.front", "foreign.front"),
        SOURCE.replace("row.front", "row.front.noSuchField"),
    ] {
        assert!(meari::parse(&source, Language::Tsx).is_err(), "{source}");
    }
}

#[test]
fn nested_record_arrays_keep_outer_capture_and_inner_shadowing() {
    for (inner_name, expected) in [("child", "outerinner"), ("row", "innerinner")] {
        let source = format!(
            "type Props={{items:{{key:string;label:string;children:{{key:string;label:string}}[]}}[]}};export function Nested(props:Props){{return <ul>{{props.items.map(row => <li key={{row.key}}><ul>{{row.children.map({inner_name} => <li key={{{inner_name}.key}}>{{row.label}}{{{inner_name}.label}}</li>)}}</ul></li>)}}</ul>;}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let PropType::Array(outer) = &component.props[0].kind else {
            panic!()
        };
        let PropType::Record(outer_fields) = outer.as_ref() else {
            panic!()
        };
        let PropType::Array(inner) = &outer_fields[2].kind else {
            panic!()
        };
        let PropType::Record(inner_fields) = inner.as_ref() else {
            panic!()
        };
        let child = RecordValue::new(
            inner_fields.clone(),
            BTreeMap::from([
                ("key".into(), Value::String("child".into())),
                ("label".into(), Value::String("inner".into())),
            ]),
        )
        .unwrap();
        let row = RecordValue::new(
            outer_fields.clone(),
            BTreeMap::from([
                ("key".into(), Value::String("root".into())),
                ("label".into(), Value::String("outer".into())),
                (
                    "children".into(),
                    Value::Array(ArrayValue {
                        element: inner.as_ref().clone(),
                        items: vec![Value::Record(child)],
                    }),
                ),
            ]),
        )
        .unwrap();
        let bound = binding::bind(
            &component,
            &BTreeMap::from([(
                "items".into(),
                Value::Array(ArrayValue {
                    element: outer.as_ref().clone(),
                    items: vec![Value::Record(row)],
                }),
            )]),
        )
        .unwrap();
        fn text(node: &meari::domain::Node) -> String {
            match &node.kind {
                NodeKind::Text(Value::String(value)) => value.clone(),
                NodeKind::Element { children, .. } => children.iter().map(text).collect(),
                _ => panic!("unresolved expression"),
            }
        }
        assert_eq!(text(&bound.root), expected);
        for language in [Language::Rust, Language::Tsx] {
            let code = meari::emit(&component, language).unwrap().code;
            assert!(component.semantic_eq(&meari::parse(&code, language).unwrap()));
        }
    }
}

#[test]
fn record_rows_forward_to_real_modules_and_keep_state_with_their_keys() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{build::BuildPath, component_state::StateValue},
    };
    let entry: BuildPath = "cards.tsx".to_owned().try_into().unwrap();
    let parent = "import { Card } from './card';type Props={items:{key:string;front:string}[]};export function Cards(props:Props){return <ul>{props.items.map(row => <Card key={row.key} item={row}/>)}</ul>;}";
    let child = "import {useState} from 'react';type Props={item:{front:string;key:string}};export function Card(props:Props){const [count,setCount]=useState(0);return <li><button onClick={()=>setCount(previous=>previous+1)}>{props.item.front}{count}</button></li>;}";
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([
            (entry.clone(), parent.into()),
            ("card.tsx".to_owned().try_into().unwrap(), child.into()),
        ]),
    )
    .unwrap();
    let PropType::Array(element) = &graph.modules[&entry].component.props[0].kind else {
        panic!()
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!()
    };
    let values = |keys: &[&str]| {
        BTreeMap::from([(
            "items".into(),
            Value::Array(ArrayValue {
                element: element.as_ref().clone(),
                items: keys
                    .iter()
                    .map(|key| {
                        Value::Record(
                            RecordValue::new(
                                schema.clone(),
                                BTreeMap::from([
                                    ("key".into(), Value::String((*key).into())),
                                    ("front".into(), Value::String(format!("front {key}"))),
                                ]),
                            )
                            .unwrap(),
                        )
                    })
                    .collect(),
            }),
        )])
    };
    let initial = values(&["a", "b"]);
    let reordered_values = values(&["b", "new", "a"]);
    let deleted_values = values(&["b"]);
    let mut program = ModuleProgram::mount(graph, initial, "record-modules".into(), 1).unwrap();
    let activated = program.activate("0/0/0").unwrap();
    let one = StateValue::Number(serde_json::Number::from_f64(1.0).unwrap());
    let (identity, instance) = activated
        .instances
        .iter()
        .find(|(_, state)| state.values.get("count") == Some(&one))
        .map(|(identity, state)| (identity.clone(), state.instance.clone()))
        .unwrap();
    let reordered = program.update_props(reordered_values).unwrap();
    assert_eq!(reordered.instances[&identity].instance, instance);
    assert_eq!(reordered.instances[&identity].values["count"], one);
    let activated = program.activate("0/2/0").unwrap();
    assert_eq!(
        activated.instances[&identity].values["count"],
        StateValue::Number(serde_json::Number::from_f64(2.0).unwrap())
    );
    assert!(
        !program
            .update_props(deleted_values)
            .unwrap()
            .instances
            .contains_key(&identity)
    );
}
