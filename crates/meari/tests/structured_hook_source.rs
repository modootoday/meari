use meari::{
    application::{
        Language,
        hook_expression::{self, Bindings},
        hook_values::{Budget, Owner},
        state_program::StateProgram,
    },
    domain::{
        NodeKind, Value,
        component_state::StateValue,
        hook_expression::{Expression, Kind},
        source_schema::SourceSchema,
        state_program::StateUpdate,
    },
};
use std::collections::BTreeMap;
const SOURCE: &str = include_str!("../fixtures/hooks/structured-state.tsx");

#[test]
fn parameterless_components_keep_empty_props_contract_and_reject_unbound_props() {
    for source in [
        "export function Empty() { return <button>empty</button>; }",
        "type Props = {}; export function Empty() { return <button>empty</button>; }",
    ] {
        let parsed = meari::parse(source, Language::Tsx).unwrap();
        assert!(parsed.props.is_empty());
        let emitted = meari::emit(&parsed, Language::Tsx).unwrap();
        assert!(parsed.semantic_eq(&meari::parse(&emitted.code, Language::Tsx).unwrap()));
    }
    for source in [
        "export function Empty() { return <p>{props.name}</p>; }",
        "type Props = { name: string }; export function Empty() { return <p>{props.name}</p>; }",
        "type Props = {}; export function Empty(other: Props) { return <button>empty</button>; }",
        "export async function Empty() { return <button>empty</button>; }",
        "export function* Empty() { return <button>empty</button>; }",
        "type Props = {}; export function Empty(props: Props, extra: Props) { return <button>empty</button>; }",
        "type Props = {}; export function Empty({}: Props) { return <button>empty</button>; }",
        "type Props = {}; export function Empty(...props: Props[]) { return <button>empty</button>; }",
    ] {
        assert_eq!(
            meari::parse(source, Language::Tsx).unwrap_err().code,
            "unsupported"
        );
    }
}

#[test]
fn parsed_array_fixture_uses_candidate_array_reads_and_changes_length_after_publication() {
    let component = meari::parse(
        include_str!("../fixtures/hooks/structured-array.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let StateValue::Structured(initial) = &component.states[0].initial else {
        panic!()
    };
    let mut owner = Owner::new(Budget::default());
    let evaluated = hook_expression::evaluate(
        owner.begin().unwrap(),
        initial,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .unwrap();
    let old = evaluated.root;
    owner
        .prepare_publish(evaluated.candidate)
        .unwrap()
        .publish();
    assert_eq!(
        owner.number(&owner.element(&old, 1).unwrap().unwrap()),
        Ok(2.0)
    );
    let NodeKind::Element { attributes, .. } = &component.root.kind else {
        panic!()
    };
    let event = attributes
        .iter()
        .find_map(|attribute| match &attribute.value {
            Value::Event(event) => Some(event),
            _ => None,
        })
        .unwrap();
    let StateUpdate::Set(StateValue::Structured(update)) = &event.update else {
        panic!()
    };
    let evaluated = hook_expression::evaluate(
        owner.begin().unwrap(),
        update,
        Bindings {
            states: &BTreeMap::new(),
            previous: Some(&old),
        },
    )
    .unwrap();
    let first = evaluated
        .candidate
        .element(&evaluated.root, 0)
        .unwrap()
        .unwrap();
    assert_eq!(evaluated.candidate.read_number(&first), Ok(3.0));
    assert!(
        evaluated
            .candidate
            .element(&evaluated.root, 1)
            .unwrap()
            .is_none()
    );
    assert!(owner.element(&old, 1).unwrap().is_some());
    let next = evaluated.root;
    owner
        .prepare_publish(evaluated.candidate)
        .unwrap()
        .publish();
    assert!(owner.element(&next, 1).unwrap().is_none());
    assert!(owner.element(&old, 1).unwrap().is_some());
}

#[test]
fn real_frontends_roundtrip_structured_declarations_and_setter_with_portable_number_bits() {
    let original = meari::parse(SOURCE, Language::Tsx).unwrap();
    assert!(matches!(
        original.states[0].initial,
        StateValue::Structured(_)
    ));
    let schema = serde_json::to_value(SourceSchema::from_component(&original).unwrap()).unwrap();
    assert_eq!(schema["version"], 4);
    let rust = meari::emit(&original, Language::Rust).unwrap();
    let parsed_rust = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(original.semantic_eq(&parsed_rust));
    let tsx = meari::emit(&parsed_rust, Language::Tsx).unwrap();
    let parsed_tsx = meari::parse(&tsx.code, Language::Tsx).unwrap();
    assert!(original.semantic_eq(&parsed_tsx));
    let json = serde_json::to_value(&original.states[0].initial).unwrap();
    let encoded = serde_json::to_string(&json).unwrap();
    assert!(encoded.contains("8000000000000000"));
    let number = Expression {
        span: Default::default(),
        kind: Kind::Number(1.0000000000000002_f64.to_bits()),
    };
    let encoded = serde_json::to_value(&number).unwrap();
    assert!(encoded["kind"]["value"].is_string());
    assert_eq!(
        serde_json::from_value::<Expression>(encoded).unwrap(),
        number
    );
    let array = meari::parse(
        include_str!("../fixtures/hooks/structured-array.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let rust = meari::emit(&array, Language::Rust).unwrap();
    let parsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(array.semantic_eq(&parsed));
    let tsx = meari::emit(&parsed, Language::Tsx).unwrap();
    assert!(array.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
}

#[test]
fn structured_shape_rejects_wrong_setters_and_enters_schema_without_changing_legacy_schema() {
    let original = meari::parse(SOURCE, Language::Tsx).unwrap();
    let mut changed = original.clone();
    let StateValue::Structured(expression) = &mut changed.states[0].initial else {
        panic!()
    };
    let Kind::Record(fields) = &mut expression.kind else {
        panic!()
    };
    let meari::domain::hook_expression::Entry::Field { value, .. } = &mut fields[0] else {
        panic!()
    };
    value.kind = Kind::Text("wrong count type".into());
    assert!(meari::emit(&changed, Language::Tsx).is_err());
    for wrong in [
        "{ count: 1 }",
        "[1, 2]",
        "{ count: 1, rows: [true], zero: -0 }",
    ] {
        let source = SOURCE.replace("{ count: 1, rows: [\"next\"], zero: -0 }", wrong);
        assert!(meari::parse(&source, Language::Tsx).is_err());
    }
    let scalar =
        meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx).unwrap();
    let schema = serde_json::to_value(SourceSchema::from_component(&scalar).unwrap()).unwrap();
    assert_ne!(schema["version"], 4);
    for hook in schema["hooks"].as_array().unwrap() {
        assert!(hook.get("shape").is_none());
    }
    let structured =
        serde_json::to_value(SourceSchema::from_component(&original).unwrap()).unwrap();
    assert!(structured["hooks"][0].get("shape").is_some());
}

#[test]
fn parsed_fixture_consumes_real_candidate_staged_reader_and_publication_guard() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let StateValue::Structured(initial) = &component.states[0].initial else {
        panic!()
    };
    let mut owner = Owner::new(Budget::default());
    let evaluated = hook_expression::evaluate(
        owner.begin().unwrap(),
        initial,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .unwrap();
    let old = evaluated.root;
    owner
        .prepare_publish(evaluated.candidate)
        .unwrap()
        .publish();
    let NodeKind::Element { attributes, .. } = &component.root.kind else {
        panic!()
    };
    let event = attributes
        .iter()
        .find_map(|attribute| match &attribute.value {
            Value::Event(event) => Some(event),
            _ => None,
        })
        .unwrap();
    let StateUpdate::Set(StateValue::Structured(update)) = &event.update else {
        panic!()
    };
    let bindings = BTreeMap::from([("state".into(), old.clone())]);
    let evaluated = hook_expression::evaluate(
        owner.begin().unwrap(),
        update,
        Bindings {
            states: &bindings,
            previous: Some(&old),
        },
    )
    .unwrap();
    let count = evaluated
        .candidate
        .field(&evaluated.root, "count")
        .unwrap()
        .unwrap();
    assert_eq!(evaluated.candidate.read_number(&count), Ok(1.0));
    assert_eq!(
        owner.number(&owner.field(&old, "count").unwrap().unwrap()),
        Ok(0.0)
    );
    let next = evaluated.root;
    owner
        .prepare_publish(evaluated.candidate)
        .unwrap()
        .publish();
    assert_eq!(
        owner.number(&owner.field(&next, "count").unwrap().unwrap()),
        Ok(1.0)
    );
    assert!(!owner.same_value(&old, &next).unwrap());
}

#[test]
fn structured_ir_never_falls_back_to_scalar_json_runtime_and_unsupported_literals_keep_spans() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    assert!(
        StateProgram::mount(component, "not-yet-prepared".into(), 1)
            .err()
            .unwrap()
            .contains("prepared hook runtime")
    );
    for (bad, expected) in [
        (
            "{get count() { return 1; }}",
            "plain structured data property",
        ),
        ("[1,,2]", "sparse/spread arrays"),
        ("{__proto__: null}", "prototype-setting"),
        (r#"{ label: "\uD800" }"#, "lone UTF-16"),
        (r#"{ "\uD800": 1 }"#, "lone UTF-16"),
    ] {
        let source = format!(
            "import {{ useState }} from \"react\"; export function Example() {{ const [state, setState] = useState({bad}); return <button>data</button>; }}"
        );
        let error = meari::parse(&source, Language::Tsx).unwrap_err();
        assert!(error.span.end > error.span.start);
        assert!(error.span.end <= source.len());
        assert_eq!(error.code, "unsupported");
        assert!(error.message.contains(expected), "{error:?}");
        let literal_start = source.find(bad).unwrap();
        assert!(error.span.start >= literal_start);
        assert!(error.span.end <= literal_start + bad.len());
    }
    let scalar =
        meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx).unwrap();
    assert!(StateProgram::mount(scalar, "legacy-counter".into(), 1).is_ok());
    let paired = r#"import { useState } from "react"; export function Paired() { const [state, setState] = useState({ label: "\uD83D\uDE00" }); return <button>pair</button>; }"#;
    let parsed = meari::parse(paired, Language::Tsx).unwrap();
    let generated = meari::emit(&parsed, Language::Tsx).unwrap();
    assert!(parsed.semantic_eq(&meari::parse(&generated.code, Language::Tsx).unwrap()));
}
