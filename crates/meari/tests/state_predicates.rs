use meari::{
    application::{Language, binding, source_modules::ModuleProgram, state_program::StateProgram},
    domain::{
        Node, NodeKind, Span, Value,
        bool_expression::{BoolExpression, BoolExpressionKind as Bool},
        build::BuildPath,
    },
};
use std::collections::BTreeMap;

const SOURCE: &str = "import {useState} from 'react';type Props={enabled:boolean;id:string};export function Panel(props:Props){const [expanded,setExpanded]=useState(false);return <section hidden={!expanded || !props.enabled || props.id !== \"open\"}><button aria-expanded={expanded && props.enabled} onClick={()=>setExpanded(previous=>!previous)}>toggle</button></section>;}";
fn props(enabled: bool) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("enabled".into(), Value::Bool(enabled)),
        ("id".into(), Value::String("open".into())),
    ])
}
fn attribute(node: &Node, name: &str) -> Value {
    let NodeKind::Element { attributes, .. } = &node.kind else {
        panic!()
    };
    attributes
        .iter()
        .find(|a| a.name == name)
        .unwrap()
        .value
        .clone()
}

#[test]
fn state_and_props_predicates_roundtrip_and_reproject_without_freezing_the_state() {
    let source = meari::parse(SOURCE, Language::Tsx).unwrap();
    let rust = meari::emit(&source, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 14);
    assert!(rust.code.contains("state_bool(state, \"expanded\")?"));
    assert!(source.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let bound = binding::bind(&source, &props(true)).unwrap();
    let Value::Predicate(predicate) = attribute(&bound.root, "hidden") else {
        panic!()
    };
    let Bool::Or(_, right) = predicate.kind else {
        panic!()
    };
    assert!(matches!(right.kind, Bool::Literal(false)));
    let mut program = StateProgram::mount(bound, "state-predicate".into(), 1).unwrap();
    let before = program.snapshot().unwrap();
    assert_eq!(
        attribute(&program.project().unwrap().component.root, "hidden"),
        Value::Bool(true)
    );
    let event = program.project().unwrap().events["0/0"].clone();
    let projected = program.dispatch(&event).unwrap();
    assert_eq!(
        attribute(&projected.component.root, "hidden"),
        Value::Bool(false)
    );
    let after = program.snapshot().unwrap();
    assert_eq!(before.instance, after.instance);
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(
        attribute(&program.dispatch(&event).unwrap().component.root, "hidden"),
        Value::Bool(true)
    );
}

#[test]
fn bound_comparisons_do_not_copy_large_prop_payloads_into_state_predicates() {
    let source = meari::parse(SOURCE, Language::Tsx).unwrap();
    let payload = "x".repeat(16 * 1024);
    let mut values = props(true);
    values.insert("id".into(), Value::String(payload.clone()));
    let bound = binding::bind(&source, &values).unwrap();
    let encoded = serde_json::to_string(&bound).unwrap();
    assert!(!encoded.contains(&payload));
    assert!(encoded.len() < payload.len());
    let Value::Predicate(predicate) = attribute(&bound.root, "hidden") else {
        panic!()
    };
    let Bool::Or(_, right) = predicate.kind else {
        panic!()
    };
    assert!(matches!(right.kind, Bool::Literal(true)));
}

#[test]
fn string_repeat_item_shadowing_boolean_state_is_not_read_as_the_outer_state() {
    let source = "import {useState} from 'react';type Props={rows:string[]};export function Shadow(props:Props){const [expanded,setExpanded]=useState(false);return <div>{props.rows.map(expanded=><button key={expanded} disabled={!expanded}>row</button>)}</div>;}";
    let error =
        meari::parse(source, Language::Tsx).expect_err("the string item shadows the Boolean state");
    assert_eq!(&source[error.span.start..error.span.end], "expanded");
}

#[test]
fn unshadowed_state_in_repeat_roundtrips_and_reprojects_both_predicate_and_direct_reads() {
    use meari::runtime::IntoValue;
    let source = "import {useState} from 'react';type Props={rows:string[]};export function Rows(props:Props){const [expanded,setExpanded]=useState(false);return <div><button onClick={()=>setExpanded(previous=>!previous)}>toggle</button>{props.rows.map(row=><button key={row} disabled={!expanded} aria-expanded={expanded}>row</button>)}</div>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let values = BTreeMap::from([("rows".into(), vec!["a", "b"].into_value())]);
    let mut program = StateProgram::mount(
        binding::bind(&component, &values).unwrap(),
        "repeat-state-scope".into(),
        1,
    )
    .unwrap();
    let instance = program.snapshot().unwrap().instance;
    for disabled in [true, false, true] {
        let projection = program.project().unwrap();
        let NodeKind::Element { children, .. } = &projection.component.root.kind else {
            panic!()
        };
        assert_eq!(children.len(), 3);
        for child in &children[1..] {
            assert_eq!(attribute(child, "disabled"), Value::Bool(disabled));
            assert_eq!(attribute(child, "aria-expanded"), Value::Bool(!disabled));
        }
        assert_eq!(program.snapshot().unwrap().instance, instance);
        program.dispatch(&projection.events["0/0"]).unwrap();
    }
    let bad_rust = rust
        .code
        .replacen(", \"row\", \"row\", ", ", \"expanded\", \"expanded\", ", 1);
    assert_ne!(bad_rust, rust.code);
    assert!(
        meari::parse(&bad_rust, Language::Rust)
            .unwrap_err()
            .message
            .contains("shadowed")
    );
    let mut direct = component;
    let NodeKind::Element { children, .. } = &mut direct.root.kind else {
        panic!()
    };
    let NodeKind::Text(Value::Repeat(repeat)) = &mut children[1].kind else {
        panic!()
    };
    repeat.item = "expanded".into();
    repeat.key = "expanded".into();
    let NodeKind::Element { attributes, .. } = &mut repeat.template.kind else {
        panic!()
    };
    attributes.retain(|attribute| attribute.name != "disabled");
    assert!(
        meari::emit(&direct, Language::Rust)
            .unwrap_err()
            .message
            .contains("shadowed")
    );
}

#[test]
fn nested_repeat_shadowing_and_unsupported_index_do_not_resolve_outer_boolean_state() {
    for body in [
        "props.rows.map(row=><div key={row}>{props.rows.map(expanded=><button key={expanded} disabled={!expanded}>row</button>)}</div>)",
        "props.rows.map(expanded=><div key={expanded}>{props.rows.map(row=><button key={row} disabled={!expanded}>row</button>)}</div>)",
    ] {
        let source = format!(
            "import {{useState}} from 'react';type Props={{rows:string[]}};export function Nested(props:Props){{const [expanded,setExpanded]=useState(false);return <div>{{{body}}}</div>;}}"
        );
        let error = meari::parse(&source, Language::Tsx).unwrap_err();
        assert_eq!(&source[error.span.start..error.span.end], "expanded");
        assert!(error.message.contains("shadowed"));
    }
    assert!(meari::parse("import {useState} from 'react';type Props={rows:string[]};export function Index(props:Props){const [expanded,setExpanded]=useState(false);return <div>{props.rows.map((row,expanded)=><button key={row} disabled={!expanded}>row</button>)}</div>;}",Language::Tsx).is_err());
}

#[test]
fn undeclared_and_nonboolean_state_reads_are_rejected_at_the_original_operand() {
    for source in [
        SOURCE.replace("!expanded", "!missing"),
        SOURCE.replace("useState(false)", "useState('false')"),
        SOURCE.replace("useState(false)", "useState(0)"),
    ] {
        let error = meari::parse(&source, Language::Tsx).unwrap_err();
        assert!(
            error.message.contains("declared boolean state"),
            "{error:?}"
        );
        assert!(!source[error.span.start..error.span.end].is_empty());
    }
}

#[test]
fn state_predicates_keep_short_circuit_and_missing_runtime_state_is_not_a_false_literal() {
    let missing = BoolExpression {
        kind: Bool::State("missing".into()),
        span: Span { start: 7, end: 14 },
    };
    let value = BoolExpression {
        kind: Bool::And(
            Box::new(BoolExpression {
                kind: Bool::Literal(false),
                span: Span::default(),
            }),
            Box::new(missing.clone()),
        ),
        span: Span::default(),
    };
    assert!(!value.evaluate(&BTreeMap::new()).unwrap());
    assert_eq!(
        missing.evaluate(&BTreeMap::new()).unwrap_err().span,
        missing.span
    );
}

#[test]
fn direct_state_reads_keep_legacy_ir_while_redundant_standalone_predicates_are_rejected() {
    let source = "import {useState} from 'react';type Props={};export function Direct(props:Props){const [expanded,setExpanded]=useState(false);return <button aria-expanded={expanded} onClick={()=>setExpanded(previous=>!previous)}>toggle</button>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_ne!(rust.schema_version, 14);
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let mut noncanonical = component.clone();
    let NodeKind::Element { attributes, .. } = &mut noncanonical.root.kind else {
        panic!()
    };
    attributes
        .iter_mut()
        .find(|a| a.name == "aria-expanded")
        .unwrap()
        .value = Value::Predicate(Box::new(BoolExpression {
        kind: Bool::State("expanded".into()),
        span: Span::default(),
    }));
    assert!(
        meari::emit(&noncanonical, Language::Rust)
            .unwrap_err()
            .message
            .contains("canonical Value::State")
    );
    assert!(meari::emit(&noncanonical, Language::Tsx).is_err());
}

#[test]
fn module_instances_isolate_state_and_rejected_props_keep_the_accepted_projection() {
    let path = |s: &str| BuildPath::try_from(s.to_owned()).unwrap();
    let graph=meari::parse_module_graph(&path("root.tsx"),&BTreeMap::from([(path("root.tsx"),"import {Panel} from './panel';type Props={enabled:boolean;id:string};export function Root(props:Props){return <main><Panel enabled={props.enabled} id={props.id}/><Panel enabled={props.enabled} id={props.id}/></main>;}".into()),(path("panel.tsx"),SOURCE.into())])).unwrap();
    let mut program =
        ModuleProgram::mount(graph, props(true), "isolated-predicates".into(), 1).unwrap();
    let first = program
        .project()
        .unwrap()
        .event_paths()
        .next()
        .unwrap()
        .clone();
    program.activate(&first).unwrap();
    let before = program.project().unwrap().clone();
    let NodeKind::Element { children, .. } = &before.component.root.kind else {
        panic!()
    };
    assert_eq!(attribute(&children[0], "hidden"), Value::Bool(false));
    assert_eq!(attribute(&children[1], "hidden"), Value::Bool(true));
    program.update_props(props(false)).unwrap();
    let NodeKind::Element { children, .. } = &program.project().unwrap().component.root.kind else {
        panic!()
    };
    assert_eq!(attribute(&children[0], "hidden"), Value::Bool(true));
    program.update_props(props(true)).unwrap();
    assert_eq!(program.project().unwrap().instances, before.instances);
    let accepted = program.project().unwrap().clone();
    let mut invalid = props(true);
    invalid.insert("enabled".into(), Value::String("true".into()));
    assert!(program.update_props(invalid).is_err());
    assert!(
        program
            .project()
            .unwrap()
            .component
            .semantic_eq(&accepted.component)
    );
    assert_eq!(program.project().unwrap().instances, accepted.instances);
}
