use meari::{
    application::{Language, binding},
    domain::{
        PropType, Value,
        host_callbacks::{CallbackReference, CallbackSignature},
    },
};
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../../fixtures/host-callback-review.tsx");

#[test]
fn original_review_source_preserves_callback_signatures_ids_disabled_flags_and_roundtrip() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    assert_eq!(
        component
            .props
            .iter()
            .filter(|prop| matches!(prop.kind, PropType::Callback(CallbackSignature::Action)))
            .count(),
        3
    );
    assert!(
        component
            .props
            .iter()
            .any(|prop| prop.name == "onDraftChange"
                && prop.kind == PropType::Callback(CallbackSignature::Text))
    );
    for prop in [
        "inputDisabled",
        "saveDisabled",
        "acceptDisabled",
        "refreshDisabled",
    ] {
        assert!(
            component
                .props
                .iter()
                .any(|field| field.name == prop && field.kind == PropType::Bool)
        );
    }
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 6);
    let reparsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&reparsed));
    let tsx = meari::emit(&reparsed, Language::Tsx).unwrap();
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    let direct = meari::parse("type Props={onSave:()=>void};export function Direct(props:Props){return <button onClick={props.onSave}/>;}", Language::Tsx).unwrap();
    assert!(
        direct.semantic_eq(
            &meari::parse(
                &meari::emit(&direct, Language::Rust).unwrap().code,
                Language::Rust
            )
            .unwrap()
        )
    );
}

#[test]
fn bindings_keep_typed_callback_identity_and_reject_runtime_source_emission() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let mut props = component
        .props
        .iter()
        .map(|prop| {
            (
                prop.name.clone(),
                match &prop.kind {
                    PropType::String => Value::String("draft".into()),
                    PropType::Bool => Value::Bool(false),
                    PropType::Callback(signature) => Value::CallbackReference(CallbackReference {
                        binding: format!("host_{}", prop.name),
                        signature: *signature,
                    }),
                    PropType::Array(_) => panic!("callback fixture requires explicit array values"),
                    PropType::Node => panic!("callback fixture requires explicit node slots"),
                    PropType::Record(_) => {
                        panic!("callback fixture requires explicit record values")
                    }
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let bound = binding::bind(&component, &props).unwrap();
    assert!(bound.props.is_empty());
    assert!(meari::emit(&bound, Language::Tsx).is_err());
    props.insert(
        "onSave".into(),
        Value::CallbackReference(CallbackReference {
            binding: "wrong".into(),
            signature: CallbackSignature::Text,
        }),
    );
    assert!(binding::bind(&component, &props).is_err());
}

#[test]
fn source_callback_type_and_call_mismatches_keep_original_diagnostics() {
    for source in [
        "type Props={onSave:()=>string};export function X(props:Props){return <button onClick={()=>props.onSave()}/>;}",
        "type Props={onEdit:(value:number)=>void};export function X(props:Props){return <input onChange={event=>props.onEdit(event.currentTarget.value)}/>;}",
        "type Props={onSave:()=>void};export function X(props:Props){return <input onChange={event=>props.onSave(event.currentTarget.value)}/>;}",
        "type Props={onEdit:(value:string)=>void};export function X(props:Props){return <button onClick={()=>props.onEdit()}/>;}",
    ] {
        let diagnostic = meari::parse(source, Language::Tsx).unwrap_err();
        assert_eq!(diagnostic.code, "unsupported");
        assert!(diagnostic.span.end <= source.len());
        assert!(diagnostic.span.start < diagnostic.span.end);
    }
}

#[test]
fn unchanged_conversion_source_keeps_negated_guard_and_typed_intent_inventory() {
    let source = include_str!("../../fixtures/host-callback-convert.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let bound = binding::bind(
        &component,
        &BTreeMap::from([
            ("canConvert".into(), Value::Bool(false)),
            ("resultLabel".into(), Value::String("ready".into())),
            (
                "onConvert".into(),
                Value::CallbackReference(CallbackReference {
                    binding: "convert_file".into(),
                    signature: CallbackSignature::Action,
                }),
            ),
        ]),
    )
    .unwrap();
    let program =
        meari::application::state_program::StateProgram::mount(bound, "convert".into(), 1).unwrap();
    let projection = program.project().unwrap();
    assert_eq!(projection.callbacks.len(), 1);
    assert_eq!(
        projection.callbacks["0/0"].reference.binding,
        "convert_file"
    );
    let meari::application::dom::DomNode::Element { children, .. } =
        meari::application::dom::prepare(&projection.component).unwrap()
    else {
        panic!("section");
    };
    let meari::application::dom::DomNode::Element { attributes, .. } = &children[0] else {
        panic!("button");
    };
    assert!(attributes.contains_key("disabled"));
    assert!(!attributes.contains_key("onclick"));
}

#[test]
fn module_forwarding_preserves_current_callback_identity_and_instance() {
    use meari::{application::source_modules::ModuleProgram, domain::build::BuildPath};
    let path = |name: &str| BuildPath::try_from(name.to_owned()).unwrap();
    let graph = meari::parse_module_graph(&path("view.tsx"), &BTreeMap::from([
        (path("view.tsx"), "import {Actions} from './actions';type Props={text:string;onSave:()=>void;onText:(value:string)=>void};export function View(props:Props){return <main><Actions text={props.text} onSave={props.onSave} onText={props.onText}/></main>; }".into()),
        (path("actions.tsx"), "type Props={text:string;onSave:()=>void;onText:(value:string)=>void};export function Actions(props:Props){return <section><textarea value={props.text} onChange={event=>props.onText(event.currentTarget.value)}/><button onClick={()=>props.onSave()}>Save</button></section>; }".into()),
    ])).unwrap();
    let props = |target: &str| {
        BTreeMap::from([
            ("text".into(), Value::String("draft".into())),
            (
                "onSave".into(),
                Value::CallbackReference(CallbackReference {
                    binding: target.into(),
                    signature: CallbackSignature::Action,
                }),
            ),
            (
                "onText".into(),
                Value::CallbackReference(CallbackReference {
                    binding: "draft_change".into(),
                    signature: CallbackSignature::Text,
                }),
            ),
        ])
    };
    let mut program =
        ModuleProgram::mount(graph, props("save"), "callback-modules".into(), 1).unwrap();
    let before = program.project().unwrap().clone();
    assert_eq!(before.callback_bindings().len(), 2);
    assert_eq!(
        before.callback_bindings()["0/0/0"].reference.binding,
        "draft_change"
    );
    assert_eq!(before.origins["0/0/0"].module, path("actions.tsx"));
    let after = program.update_props(props("save_next")).unwrap();
    assert_eq!(
        after.callback_bindings()["0/0/1"].reference.binding,
        "save_next"
    );
    assert_eq!(
        before.instances["root@0/0"].instance,
        after.instances["root@0/0"].instance
    );
}
