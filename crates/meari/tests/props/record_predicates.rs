use meari::{
    application::{Language, binding},
    domain::{
        Component, Node, NodeKind, PropType, Value, array_value::ArrayValue,
        record_value::RecordValue,
    },
};
use std::collections::BTreeMap;

const SOURCE: &str = r#"type Props={blocked:boolean;items:{key:string;editable:boolean;flags:{hidden:boolean};label:string}[]};
export function Choices(props:Props){return <div>{props.items.map(item => <button key={item.key} disabled={!item.editable || props.blocked} aria-hidden={item.flags.hidden === true}>{item.label}</button>)}</div>}"#;

fn items(component: &Component, editable: bool) -> Value {
    let PropType::Array(element) = &component
        .props
        .iter()
        .find(|prop| prop.name == "items")
        .unwrap()
        .kind
    else {
        panic!("array required")
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!("record required")
    };
    let PropType::Record(flags) = &schema
        .iter()
        .find(|prop| prop.name == "flags")
        .unwrap()
        .kind
    else {
        panic!("nested record required")
    };
    let row = RecordValue::new(
        schema.clone(),
        BTreeMap::from([
            ("key".into(), Value::String("stable".into())),
            ("editable".into(), Value::Bool(editable)),
            (
                "flags".into(),
                Value::Record(
                    RecordValue::new(
                        flags.clone(),
                        BTreeMap::from([("hidden".into(), Value::Bool(false))]),
                    )
                    .unwrap(),
                ),
            ),
            ("label".into(), Value::String("선택".into())),
        ]),
    )
    .unwrap();
    Value::Array(ArrayValue {
        element: element.as_ref().clone(),
        items: vec![Value::Record(row)],
    })
}

fn attribute(node: &Node, name: &str) -> Value {
    let NodeKind::Element { attributes, .. } = &node.kind else {
        panic!("element required")
    };
    attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .unwrap()
        .value
        .clone()
}

#[test]
fn original_lexical_negation_and_nested_comparison_roundtrip() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        assert_eq!(output.schema_version, 17);
        let reparsed = meari::parse(&output.code, language).unwrap();
        assert!(component.semantic_eq(&reparsed), "{}", output.code);
    }
}

#[test]
fn actual_repeat_projection_resolves_lexical_and_host_boolean_together() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    for (editable, blocked, expected) in [
        (true, false, false),
        (false, false, true),
        (true, true, true),
    ] {
        let projected = binding::bind(
            &component,
            &BTreeMap::from([
                ("blocked".into(), Value::Bool(blocked)),
                ("items".into(), items(&component, editable)),
            ]),
        )
        .unwrap();
        let NodeKind::Element { children, .. } = &projected.root.kind else {
            panic!("root required")
        };
        assert_eq!(children.len(), 1);
        assert_eq!(attribute(&children[0], "disabled"), Value::Bool(expected));
        assert_eq!(attribute(&children[0], "aria-hidden"), Value::Bool(false));
    }
}

#[test]
fn wrong_field_types_and_unknown_nested_fields_are_diagnostics() {
    for source in [
        SOURCE.replace("!item.editable", "!item.label"),
        SOURCE.replace("item.flags.hidden", "item.flags.missing"),
    ] {
        let diagnostic = meari::parse(&source, Language::Tsx).unwrap_err();
        assert!(diagnostic.span.end > diagnostic.span.start);
        assert!(diagnostic.span.end <= source.len());
    }
}

#[test]
fn nested_prop_string_comparison_and_boolean_read_roundtrip_and_bind() {
    let source = "type Props={settings:{flags:{disabled:boolean};mode:string}};export function Settings(props:Props){return <button disabled={props.settings.flags.disabled || props.settings.mode !== \"edit\"}>edit</button>}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    let PropType::Record(schema) = &component.props[0].kind else {
        panic!("settings")
    };
    let PropType::Record(flags_schema) = &schema
        .iter()
        .find(|field| field.name == "flags")
        .unwrap()
        .kind
    else {
        panic!("flags")
    };
    for (disabled, mode, expected) in [
        (false, "edit", false),
        (false, "view", true),
        (true, "edit", true),
    ] {
        let flags = RecordValue::new(
            flags_schema.clone(),
            BTreeMap::from([("disabled".into(), Value::Bool(disabled))]),
        )
        .unwrap();
        let settings = RecordValue::new(
            schema.clone(),
            BTreeMap::from([
                ("flags".into(), Value::Record(flags)),
                ("mode".into(), Value::String(mode.into())),
            ]),
        )
        .unwrap();
        let bound = binding::bind(
            &component,
            &BTreeMap::from([("settings".into(), Value::Record(settings))]),
        )
        .unwrap();
        assert_eq!(attribute(&bound.root, "disabled"), Value::Bool(expected));
    }
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        assert_eq!(output.schema_version, 17);
        assert!(component.semantic_eq(&meari::parse(&output.code, language).unwrap()));
    }
}

#[test]
fn string_member_comparison_keeps_lexical_operand_until_expansion() {
    let source = SOURCE
        .replace("blocked:boolean;", "blocked:boolean;target:string;")
        .replace(
            "!item.editable || props.blocked",
            "item.label !== props.target || props.blocked",
        );
    let component = meari::parse(&source, Language::Tsx).unwrap();
    for (target, expected) in [("선택", false), ("다른 카드", true)] {
        let bound = binding::bind(
            &component,
            &BTreeMap::from([
                ("blocked".into(), Value::Bool(false)),
                ("target".into(), Value::String(target.into())),
                ("items".into(), items(&component, true)),
            ]),
        )
        .unwrap();
        let NodeKind::Element { children, .. } = &bound.root.kind else {
            panic!("root")
        };
        assert_eq!(attribute(&children[0], "disabled"), Value::Bool(expected));
    }
}

#[test]
fn empty_repeat_does_not_evaluate_unbound_lexical_predicates() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let Value::Array(mut rows) = items(&component, true) else {
        panic!("items")
    };
    rows.items.clear();
    let bound = binding::bind(
        &component,
        &BTreeMap::from([
            ("blocked".into(), Value::Bool(false)),
            ("items".into(), Value::Array(rows)),
        ]),
    )
    .unwrap();
    let NodeKind::Element { children, .. } = &bound.root.kind else {
        panic!("root")
    };
    assert!(children.is_empty());
}

#[test]
fn lexical_record_predicate_keeps_live_state_across_dispatch() {
    use meari::application::state_program::StateProgram;
    let source = SOURCE.replace("type Props", "import {useState} from 'react';type Props")
        .replace("return <div>", "const [busy,setBusy]=useState(false);return <div><button onClick={()=>setBusy(previous=>!previous)}>toggle</button>")
        .replace("!item.editable || props.blocked", "!item.editable || props.blocked || busy");
    let component = meari::parse(&source, Language::Tsx).unwrap();
    let bound = binding::bind(
        &component,
        &BTreeMap::from([
            ("blocked".into(), Value::Bool(false)),
            ("items".into(), items(&component, true)),
        ]),
    )
    .unwrap();
    let mut program = StateProgram::mount(bound, "record-predicate-state".into(), 1).unwrap();
    let instance = program.snapshot().unwrap().instance;
    for disabled in [false, true, false] {
        let projection = program.project().unwrap();
        let NodeKind::Element { children, .. } = &projection.component.root.kind else {
            panic!("root required")
        };
        assert_eq!(attribute(&children[1], "disabled"), Value::Bool(disabled));
        assert_eq!(program.snapshot().unwrap().instance, instance);
        program.dispatch(&projection.events["0/0"]).unwrap();
    }
}

#[test]
fn nested_repeat_shadowing_and_outer_capture_read_the_correct_record() {
    for (inner, predicate) in [
        ("row", "!row.enabled"),
        ("child", "!row.enabled && child.enabled"),
    ] {
        let source = format!(
            "type Props={{rows:{{key:string;enabled:boolean;children:{{key:string;enabled:boolean}}[]}}[]}};export function Nested(props:Props){{return <div>{{props.rows.map(row=><section key={{row.key}}>{{row.children.map({inner}=><button key={{{inner}.key}} disabled={{{predicate}}}>child</button>)}}</section>)}}</div>}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let PropType::Array(row_kind) = &component.props[0].kind else {
            panic!("rows")
        };
        let PropType::Record(row_schema) = row_kind.as_ref() else {
            panic!("row schema")
        };
        let PropType::Array(child_kind) = &row_schema
            .iter()
            .find(|field| field.name == "children")
            .unwrap()
            .kind
        else {
            panic!("children")
        };
        let PropType::Record(child_schema) = child_kind.as_ref() else {
            panic!("child schema")
        };
        let child = Value::Record(
            RecordValue::new(
                child_schema.clone(),
                BTreeMap::from([
                    ("key".into(), Value::String("child".into())),
                    ("enabled".into(), Value::Bool(true)),
                ]),
            )
            .unwrap(),
        );
        let row = Value::Record(
            RecordValue::new(
                row_schema.clone(),
                BTreeMap::from([
                    ("key".into(), Value::String("parent".into())),
                    ("enabled".into(), Value::Bool(false)),
                    (
                        "children".into(),
                        Value::Array(ArrayValue {
                            element: child_kind.as_ref().clone(),
                            items: vec![child],
                        }),
                    ),
                ]),
            )
            .unwrap(),
        );
        let projection = binding::bind(
            &component,
            &BTreeMap::from([(
                "rows".into(),
                Value::Array(ArrayValue {
                    element: row_kind.as_ref().clone(),
                    items: vec![row],
                }),
            )]),
        )
        .unwrap();
        let NodeKind::Element { children, .. } = &projection.root.kind else {
            panic!("root")
        };
        let NodeKind::Element { children, .. } = &children[0].kind else {
            panic!("section")
        };
        assert_eq!(
            attribute(&children[0], "disabled"),
            Value::Bool(inner == "child")
        );
        for language in [Language::Rust, Language::Tsx] {
            let output = meari::emit(&component, language).unwrap();
            assert!(component.semantic_eq(&meari::parse(&output.code, language).unwrap()));
        }
    }
}

#[test]
fn module_mount_and_props_update_keep_record_predicate_bindings_exact() {
    use meari::{application::source_modules::ModuleProgram, domain::build::BuildPath};
    let path: BuildPath = "choices.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&path, &BTreeMap::from([(path.clone(), SOURCE.into())])).unwrap();
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let props = |editable| {
        BTreeMap::from([
            ("blocked".into(), Value::Bool(false)),
            ("items".into(), items(&component, editable)),
        ])
    };
    let mut program =
        ModuleProgram::mount(graph, props(true), "record-predicate-module".into(), 1).unwrap();
    for editable in [true, false, true] {
        program.update_props(props(editable)).unwrap();
        let NodeKind::Element { children, .. } = &program.project().unwrap().component.root.kind
        else {
            panic!("root")
        };
        assert_eq!(attribute(&children[0], "disabled"), Value::Bool(!editable));
    }
    assert!(program.update_props(BTreeMap::new()).is_err());
    let NodeKind::Element { children, .. } = &program.project().unwrap().component.root.kind else {
        panic!("root")
    };
    assert_eq!(attribute(&children[0], "disabled"), Value::Bool(false));
}
