use meari::{
    application::{Language, host_callbacks::DocumentCallbacks, source_modules::ModuleProgram},
    domain::{
        Value,
        build::BuildPath,
        host_callbacks::{CallbackReference, CallbackSignature},
    },
};
use std::collections::BTreeMap;
#[path = "nested_slots.rs"]
mod nested_slots;
const SOURCE: &str = "type Props={onDraft:(value:string)=>void;onCommit:(value:string)=>void};export function Range(props:Props){return <input type=\"range\" value=\"20\" onChange={event=>props.onDraft(event.currentTarget.value)} onValueCommit={value=>props.onCommit(value)}/>;}";
#[test]
fn range_draft_and_commit_keep_distinct_bindings_on_same_original_node() {
    let entry: BuildPath = "range.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), SOURCE.into())]))
            .unwrap();
    let values = BTreeMap::from([
        (
            "onDraft".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onDraft".into(),
                signature: CallbackSignature::Text,
            }),
        ),
        (
            "onCommit".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onCommit".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ]);
    let program = ModuleProgram::mount(graph, values, "range-slots".into(), 1).unwrap();
    let callbacks = program.project().unwrap().callback_bindings();
    assert_eq!(callbacks.len(), 2);
    let draft = callbacks
        .iter()
        .find(|(_, handler)| handler.reference.binding == "onDraft")
        .unwrap();
    let commit = callbacks
        .iter()
        .find(|(_, handler)| handler.reference.binding == "onCommit")
        .unwrap();
    assert_eq!(*commit.0, DocumentCallbacks::value_commit_key(draft.0));
}
#[test]
fn range_value_commit_roundtrip_preserves_text_callback_semantics() {
    let original = meari::parse(SOURCE, Language::Tsx).unwrap();
    let rust = meari::convert(SOURCE, Language::Tsx, Language::Rust).unwrap();
    let parsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(original.semantic_eq(&parsed));
    let tsx = meari::convert(&rust.code, Language::Rust, Language::Tsx).unwrap();
    assert!(original.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
}
#[test]
fn range_value_commit_does_not_admit_boolean_or_other_input_controls() {
    assert!(
        meari::parse(
            &SOURCE.replace("onCommit:(value:string)", "onCommit:(value:boolean)"),
            Language::Tsx
        )
        .is_err()
    );
    assert!(
        meari::parse(
            &SOURCE.replace("type=\"range\"", "type=\"color\""),
            Language::Tsx
        )
        .is_err()
    );
}

#[test]
fn range_value_feedback_preserves_original_lease_and_rejects_other_declaration_changes() {
    use meari::application::host_callbacks::HostCallbacks;
    use meari::domain::{host_callbacks::CallbackArguments, plugins::PluginScope};
    let source = SOURCE
        .replace("type Props={", "type Props={value:string;")
        .replace("value=\"20\"", "value={props.value}");
    let entry: BuildPath = "range.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source)])).unwrap();
    let mut values = BTreeMap::from([
        ("value".into(), Value::String("20".into())),
        (
            "onDraft".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onDraft".into(),
                signature: CallbackSignature::Text,
            }),
        ),
        (
            "onCommit".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onCommit".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ]);
    let mut program =
        ModuleProgram::mount(graph, values.clone(), "retained-range".into(), 1).unwrap();
    let previous = program.project().unwrap().clone();
    let scope = PluginScope {
        instance: previous.instances["root"].instance.clone(),
        source_generation: 1,
    };
    let mut host = HostCallbacks::new(scope.clone());
    host.bind_local("onDraft".into(), CallbackSignature::Text)
        .unwrap();
    host.bind_local("onCommit".into(), CallbackSignature::Text)
        .unwrap();
    let mut queue = DocumentCallbacks::new(
        host.export_bindings().unwrap(),
        &scope,
        previous.callback_bindings(),
    )
    .unwrap();
    let path = previous
        .callback_bindings()
        .into_iter()
        .find(|(_, handler)| handler.reference.binding == "onDraft")
        .unwrap()
        .0;
    let original = queue.event_owner(&path).unwrap();
    queue.enqueue_guarded(&path, Some("50")).unwrap();
    values.insert("value".into(), Value::String("50".into()));
    let next = program.update_props(values).unwrap();
    queue
        .validate_retained_range_props(
            &previous.component,
            &next.component,
            &next.callback_bindings(),
        )
        .unwrap();
    assert_eq!(queue.event_owner(&path).unwrap(), original);
    let old = queue.drain().pop().unwrap();
    assert!(
        host.dispatch_local(&old.handle, CallbackArguments::Text("50".into()), &scope)
            .is_ok()
    );
    for attribute in ["id", "disabled", "min", "max", "step", "className"] {
        let mut changed = next.component.clone();
        if let meari::domain::NodeKind::Element { attributes, .. } = &mut changed.root.kind {
            attributes.push(meari::domain::Attribute {
                name: attribute.into(),
                value: Value::String("changed".into()),
            });
        }
        assert!(
            queue
                .validate_retained_range_props(
                    &previous.component,
                    &changed,
                    &next.callback_bindings()
                )
                .is_err()
        );
    }
    let mut changed = next.callback_bindings();
    changed.get_mut(&path).unwrap().captured_text = Some("different capture".into());
    assert!(
        queue
            .validate_retained_range_props(&previous.component, &next.component, &changed)
            .is_err()
    );
    host.invalidate().unwrap();
    assert!(
        queue
            .validate_retained_range_props(
                &previous.component,
                &next.component,
                &next.callback_bindings()
            )
            .is_err()
    );
    assert!(!original.is_active());
}
