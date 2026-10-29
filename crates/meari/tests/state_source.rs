use meari::{
    application::{Language, state_program::StateProgram},
    domain::{NodeKind, Value, component_state::StateValue},
};

#[test]
fn source_state_and_setter_events_roundtrip_and_execute_without_render_time_effects() {
    let named = "import {useState} from 'react';type Props={};export function Demo(props:Props){const [state,setState]=useState(0);return <button onClick={()=>setState(previous=>previous+1)}>{state}</button>;}";
    let named_component = meari::parse(named, Language::Tsx).unwrap();
    let named_rust = meari::convert(named, Language::Tsx, Language::Rust).unwrap();
    assert!(named_component.semantic_eq(&meari::parse(&named_rust.code, Language::Rust).unwrap()));
    let source = include_str!("../fixtures/state-counter.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    assert_eq!(component.states.len(), 3);
    let rust = meari::convert(source, Language::Tsx, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 3);
    assert!(rust.mappings.iter().any(|mapping| {
        source[mapping.source.start..mapping.source.end].contains("useState")
            && rust.code[mapping.generated.start..mapping.generated.end]
                .contains("state_declaration")
    }));
    let parsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&parsed));
    let tsx = meari::convert(&rust.code, Language::Rust, Language::Tsx).unwrap();
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    let mut program = StateProgram::mount(component, "state-source".into(), 1).unwrap();
    let first = program.project().unwrap();
    assert_eq!(program.snapshot().unwrap().revision, 0);
    let increment = first
        .events
        .values()
        .find(|event| event.setter == "setCount")
        .unwrap()
        .clone();
    let rename = first
        .events
        .values()
        .find(|event| event.setter == "setLabel")
        .unwrap()
        .clone();
    program.dispatch(&increment).unwrap();
    program.dispatch(&increment).unwrap();
    let projected = program.dispatch(&rename).unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["count"],
        StateValue::Number(serde_json::Number::from_f64(2.0).unwrap())
    );
    assert_eq!(program.snapshot().unwrap().revision, 3);
    let NodeKind::Element { children, .. } = &projected.component.root.kind else {
        panic!()
    };
    let NodeKind::Element { children, .. } = &children[0].kind else {
        panic!()
    };
    assert_eq!(children[1].kind, NodeKind::Text(Value::String("2".into())));
    assert_eq!(
        children[3].kind,
        NodeKind::Text(Value::String(" ready🦀".into()))
    );
}

#[test]
fn unsupported_source_state_shapes_keep_precise_diagnostics() {
    for (declaration, click) in [
        (
            "const [count,setCount]=useState(0)",
            "() => setCount(Math.random())",
        ),
        (
            "const [count,setCount]=useState(0)",
            "async () => setCount(1)",
        ),
        (
            "const [count,setCount]=useState(0)",
            "() => { setCount(1); }",
        ),
        (
            "const [count,setCount]=useState(0)",
            "() => setCount(\"wrong type\")",
        ),
        (
            "const [count,setCount]=useState(1e999)",
            "() => setCount(1)",
        ),
        (
            "const [count,setCount]=useState(() => 0)",
            "() => setCount(1)",
        ),
        (
            "const [count,setCount,extra]=useState(0)",
            "() => setCount(1)",
        ),
    ] {
        let source = format!(
            "import {{useState}} from 'react';type Props={{}};export function Demo(props:Props){{{declaration};return <button onClick={{{click}}}>Count</button>;}}"
        );
        let error = meari::parse(&source, Language::Tsx).unwrap_err();
        assert!(error.span.end <= source.len());
        assert!(!error.message.is_empty());
    }
}

#[test]
fn numeric_source_state_preserves_javascript_literal_and_text_semantics() {
    for (initial, expected) in [
        ("1e21", "1e+21"),
        ("1e-6", "0.000001"),
        ("-0", "0"),
        ("9007199254740993", "9007199254740992"),
    ] {
        let source = format!(
            "import {{useState}} from 'react';type Props={{}};export function Demo(props:Props){{const [count,setCount]=useState({initial});return <p>{{count}}</p>;}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let program = StateProgram::mount(component, "numeric-state".into(), 1).unwrap();
        let projection = program.project().unwrap();
        let NodeKind::Element { children, .. } = projection.component.root.kind else {
            panic!()
        };
        assert_eq!(
            children[0].kind,
            NodeKind::Text(Value::String(expected.into()))
        );
    }
}
