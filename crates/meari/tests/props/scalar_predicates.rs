use meari::{
    application::{Language, binding, source_modules::ModuleProgram},
    domain::{
        NodeKind, Prop, PropType, Span, Value,
        bool_expression::{
            BoolExpression, BoolExpressionKind as Bool, ScalarExpression,
            ScalarExpressionKind as Scalar,
        },
        build::BuildPath,
    },
};
use std::collections::BTreeMap;

const SOURCE: &str = "type Props={id:string;other:string;enabled:boolean;expected:boolean};export function View(props:Props){return <Button disabled={!(props.id === props.other) && props.enabled !== props.expected}/>;}";

#[test]
fn strict_scalar_comparison_roundtrips_with_language_specific_operators() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 13);
    assert!(rust.code.contains("props.id == props.other"));
    assert!(rust.code.contains("props.enabled != props.expected"));
    let parsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&parsed));
    let tsx = meari::emit(&parsed, Language::Tsx).unwrap();
    assert!(tsx.code.contains("props.id === props.other"));
    assert!(tsx.code.contains("props.enabled !== props.expected"));
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
}

#[test]
fn comparison_evaluation_preserves_empty_unicode_and_boolean_types() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    for id in ["", "작업/🌊", "e\u{301}", "é"] {
        for other in ["", "작업/🌊", "e\u{301}", "é"] {
            for enabled in [false, true] {
                for expected in [false, true] {
                    let bound = binding::bind(
                        &component,
                        &BTreeMap::from([
                            ("id".into(), Value::String(id.into())),
                            ("other".into(), Value::String(other.into())),
                            ("enabled".into(), Value::Bool(enabled)),
                            ("expected".into(), Value::Bool(expected)),
                        ]),
                    )
                    .unwrap();
                    let NodeKind::Element { attributes, .. } = bound.root.kind else {
                        panic!()
                    };
                    assert_eq!(
                        attributes
                            .iter()
                            .find(|attribute| attribute.name == "disabled")
                            .unwrap()
                            .value,
                        Value::Bool(id != other && enabled != expected)
                    );
                }
            }
        }
    }
}

#[test]
fn comparison_rejects_coercion_calls_missing_props_and_different_types() {
    for rejected in [
        SOURCE.replace("props.id === props.other", "props.id == props.other"),
        SOURCE.replace("props.id === props.other", "props.id === props.enabled"),
        SOURCE.replace("props.id === props.other", "props.missing === props.other"),
        SOURCE.replace("props.id === props.other", "read() === props.other"),
        SOURCE.replace("props.id === props.other", "props?.id === props.other"),
        SOURCE.replace("props.id === props.other", "props.id === 1"),
    ] {
        assert!(
            meari::parse(&rejected, Language::Tsx).is_err(),
            "{rejected}"
        );
    }
}

#[test]
fn comparison_passes_typed_boolean_to_child_and_keeps_instance_identity() {
    let entry = BuildPath::try_from("parent.tsx".to_owned()).unwrap();
    let child = BuildPath::try_from("child.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(&entry, &BTreeMap::from([
        (entry.clone(), "import {Child} from './child';type Props={id:string};export function Parent(props:Props){return <Child selected={props.id === \"선택/🌊\"}/>;}".into()),
        (child, "type Props={selected:boolean};export function Child(props:Props){return <Button aria-current={props.selected}/>;}".into()),
    ])).unwrap();
    let mut program = ModuleProgram::mount(
        graph,
        BTreeMap::from([("id".into(), Value::String("선택/🌊".into()))]),
        "typed-comparison".into(),
        1,
    )
    .unwrap();
    let first = program.project().unwrap().clone();
    let second = program
        .update_props(BTreeMap::from([(
            "id".into(),
            Value::String("other".into()),
        )]))
        .unwrap();
    assert_eq!(
        first.instances.keys().collect::<Vec<_>>(),
        second.instances.keys().collect::<Vec<_>>()
    );
    for (projection, selected) in [(first, true), (second, false)] {
        let NodeKind::Element { attributes, .. } = projection.component.root.kind else {
            panic!()
        };
        assert_eq!(
            attributes
                .iter()
                .find(|attribute| attribute.name == "aria-current")
                .unwrap()
                .value,
            Value::Bool(selected)
        );
    }
}

#[test]
fn comparison_metadata_roundtrips_and_old_boolean_json_remains_valid() {
    let expression = BoolExpression {
        kind: Bool::Equal(
            ScalarExpression {
                kind: Scalar::Prop("id".into()),
                span: Span { start: 1, end: 9 },
            },
            ScalarExpression {
                kind: Scalar::String("작업/🌊".into()),
                span: Span { start: 14, end: 23 },
            },
        ),
        span: Span { start: 1, end: 23 },
    };
    let decoded: BoolExpression =
        serde_json::from_str(&serde_json::to_string(&expression).unwrap()).unwrap();
    assert_eq!(decoded, expression);
    decoded
        .validate(&[Prop {
            name: "id".into(),
            kind: PropType::String,
        }])
        .unwrap();
    let old: BoolExpression =
        serde_json::from_str(r#"{"kind":{"Prop":"enabled"},"span":{"start":0,"end":7}}"#).unwrap();
    old.validate(&[Prop {
        name: "enabled".into(),
        kind: PropType::Bool,
    }])
    .unwrap();
    assert!(
        old.evaluate(&BTreeMap::from([("enabled".into(), Value::Bool(true))]))
            .unwrap()
    );
}

#[test]
fn comparison_budget_and_mismatched_runtime_values_fail_explicitly() {
    let scalar = |kind| ScalarExpression {
        kind,
        span: Span::default(),
    };
    let expression = BoolExpression {
        kind: Bool::Equal(
            scalar(Scalar::Prop("id".into())),
            scalar(Scalar::String("value".into())),
        ),
        span: Span::default(),
    };
    assert!(
        expression
            .evaluate(&BTreeMap::from([("id".into(), Value::Bool(true))]))
            .is_err()
    );
    assert!(
        expression
            .evaluate(&BTreeMap::from([(
                "id".into(),
                Value::String("x".repeat(1024 * 1024 + 1))
            )]))
            .is_err()
    );
    let oversized = BoolExpression {
        kind: Bool::Equal(
            scalar(Scalar::String("x".repeat(1024 * 1024 + 1))),
            scalar(Scalar::String("value".into())),
        ),
        span: Span::default(),
    };
    assert!(oversized.validate(&[]).is_err());
    let mut deep = expression;
    for _ in 0..65 {
        deep = BoolExpression {
            kind: Bool::Not(Box::new(deep)),
            span: Span::default(),
        };
    }
    assert!(
        deep.validate(&[Prop {
            name: "id".into(),
            kind: PropType::String
        }])
        .is_err()
    );
}

#[test]
fn comparison_operands_count_toward_exact_node_budget() {
    let scalar = || ScalarExpression {
        kind: Scalar::String("x".into()),
        span: Span::default(),
    };
    let mut expression = BoolExpression {
        kind: Bool::Equal(scalar(), scalar()),
        span: Span::default(),
    };
    for _ in 0..11 {
        expression = BoolExpression {
            kind: Bool::Or(Box::new(expression.clone()), Box::new(expression)),
            span: Span::default(),
        };
    }
    // 2048 comparisons * 3 nodes + 2047 OR nodes = 8191.
    expression.validate(&[]).unwrap();
    expression = BoolExpression {
        kind: Bool::Not(Box::new(expression)),
        span: Span::default(),
    };
    expression.validate(&[]).unwrap();
    expression = BoolExpression {
        kind: Bool::Not(Box::new(expression)),
        span: Span::default(),
    };
    assert!(expression.validate(&[]).is_err());
}

#[test]
fn literal_left_boolean_literal_and_previous_boolean_schema_roundtrip() {
    let source = SOURCE
        .replace("props.id === props.other", "\"작업/🌊\" === props.id")
        .replace(
            "props.enabled !== props.expected",
            "true !== props.expected",
        );
    let component = meari::parse(&source, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let old = SOURCE.replace(
        "!(props.id === props.other) && props.enabled !== props.expected",
        "props.enabled && props.expected",
    );
    assert_eq!(
        meari::emit(&meari::parse(&old, Language::Tsx).unwrap(), Language::Tsx)
            .unwrap()
            .schema_version,
        7
    );
}

#[test]
fn original_navigation_preserves_actual_comparison_and_operand_spans() {
    fn first(node: &meari::domain::Node) -> Option<&BoolExpression> {
        let NodeKind::Element {
            attributes,
            children,
            ..
        } = &node.kind
        else {
            return None;
        };
        attributes
            .iter()
            .find_map(|attribute| match &attribute.value {
                Value::Predicate(expression) => Some(expression.as_ref()),
                _ => None,
            })
            .or_else(|| children.iter().find_map(first))
    }
    let source = include_str!("../../fixtures/node-slots/studio/navigation.tsx");
    let entry = BuildPath::try_from("studio/navigation.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([
            (entry.clone(), source.into()),
            (
                BuildPath::try_from("studio/navigation-item.tsx".to_owned()).unwrap(),
                include_str!("../../fixtures/node-slots/studio/navigation-item.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    let expression = first(&graph.modules[&entry].component.root).unwrap();
    assert_eq!(
        expression.span,
        Span {
            start: 588,
            end: 614
        }
    );
    let Bool::Equal(left, right) = &expression.kind else {
        panic!()
    };
    assert_eq!(&source[left.span.start..left.span.end], "props.selectedId");
    assert_eq!(&source[right.span.start..right.span.end], "\"SEG\"");
    let bad = SOURCE.replace("props.id === props.other", "props.missing === props.other");
    let error = meari::parse(&bad, Language::Tsx).unwrap_err();
    assert_eq!(&bad[error.span.start..error.span.end], "props.missing");
}

#[test]
fn invalid_prop_binding_keeps_last_accepted_projection() {
    let entry = BuildPath::try_from("view.tsx".to_owned()).unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), SOURCE.into())]))
            .unwrap();
    let mut props = BTreeMap::from([
        ("id".into(), Value::String("same".into())),
        ("other".into(), Value::String("same".into())),
        ("enabled".into(), Value::Bool(true)),
        ("expected".into(), Value::Bool(false)),
    ]);
    let mut program =
        ModuleProgram::mount(graph, props.clone(), "comparison-last-good".into(), 1).unwrap();
    let before = program.project().unwrap().clone();
    props.insert("id".into(), Value::Bool(true));
    assert!(program.update_props(props).is_err());
    assert!(
        program
            .project()
            .unwrap()
            .component
            .semantic_eq(&before.component)
    );
    assert_eq!(program.project().unwrap().instances, before.instances);
}

#[test]
fn comparison_result_cannot_bind_to_a_string_child_prop() {
    let entry = BuildPath::try_from("parent.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(&entry, &BTreeMap::from([
        (entry.clone(), "import {Child} from './child';type Props={id:string};export function Parent(props:Props){return <Child selected={props.id === \"selected\"}/>;}".into()),
        (BuildPath::try_from("child.tsx".to_owned()).unwrap(), "type Props={selected:string};export function Child(props:Props){return <Button aria-current={props.selected}/>;}".into()),
    ])).unwrap();
    let error = ModuleProgram::mount(
        graph,
        BTreeMap::from([("id".into(), Value::String("selected".into()))]),
        "wrong-comparison-child".into(),
        1,
    )
    .err()
    .expect("bool result cannot satisfy string child declaration");
    assert_eq!(
        error.file,
        BuildPath::try_from("child.tsx".to_owned()).unwrap()
    );
    assert!(
        error
            .diagnostic
            .message
            .contains("invalid binding: selected")
    );
}
