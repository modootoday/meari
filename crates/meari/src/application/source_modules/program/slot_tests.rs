use super::*;
use crate::domain::{
    host_callbacks::{CallbackReference, CallbackSignature},
    source_modules::ComponentReference,
};
fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn target() -> ComponentReference {
    ComponentReference {
        module: path("editor.tsx"),
        export: "Editor".into(),
    }
}
fn label(value: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("label".into(), Value::String(value.into()))])
}
fn props(body: Value, callback: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("body".into(), body),
        (
            "onSave".into(),
            Value::CallbackReference(CallbackReference {
                binding: callback.into(),
                signature: CallbackSignature::Action,
            }),
        ),
    ])
}
fn program() -> ModuleProgram {
    let mut graph=crate::parse_module_graph(&path("root.tsx"),&BTreeMap::from([(path("root.tsx"),"import type {ReactNode} from 'react';type Props={body:ReactNode;onSave:()=>void};export function Root(props:Props){return <main><button onClick={()=>props.onSave()}>save</button>{props.body}</main>; }".into())])).unwrap();
    graph.modules.extend(crate::parse_module_graph(&path("editor.tsx"),&BTreeMap::from([(path("editor.tsx"),"type Props={label:string};export function Editor(props:Props){return <article>{props.label}</article>;}".into())])).unwrap().modules);
    ModuleProgram::mount_with_node_slots(graph, "privateSlots".into(), 1, |factory| {
        Ok(props(
            factory.component("editor", target(), label("first"))?,
            "save",
        ))
    })
    .unwrap()
}
#[test]
fn active_metadata_stays_one_and_unconsumed_old_generation_is_rejected() {
    let mut program = program();
    let never_consumed = program
        .node_slots()
        .component("unused", target(), label("unused"))
        .unwrap();
    let first = program.props["body"].clone();
    let generation = program.slot_generation;
    for index in 0..32 {
        let next = program
            .node_slots()
            .component("editor", target(), label(&format!("step{index}")))
            .unwrap();
        program.update_props(props(next, "save")).unwrap();
        assert_eq!(program.active_slots.len(), 1);
    }
    assert_eq!(program.slot_generation, generation + 32);
    let before = program.project().unwrap().clone();
    for old in [never_consumed, first] {
        assert!(program.update_props(props(old, "different")).is_err());
        assert!(
            before
                .component
                .semantic_eq(&program.project().unwrap().component)
        );
        assert_eq!(
            before.callback_bindings(),
            program.project().unwrap().callback_bindings()
        );
        assert_eq!(program.active_slots.len(), 1);
    }
}
#[test]
fn generation_overflow_is_checked_before_projection_or_exports_are_committed() {
    let mut program = program();
    program.slot_generation = u64::MAX;
    let before = program.project().unwrap().clone();
    let values = program.props.clone();
    let next = program
        .node_slots()
        .component("replacement", target(), label("never committed"))
        .unwrap();
    let error = program.update_props(props(next, "different")).unwrap_err();
    assert!(error.diagnostic.message.contains("generation exhausted"));
    assert_eq!(program.props, values);
    assert_eq!(program.slot_generation, u64::MAX);
    assert_eq!(program.active_slots.len(), 1);
    assert!(
        program
            .project()
            .unwrap()
            .component
            .semantic_eq(&before.component)
    );
    assert_eq!(
        program.project().unwrap().callback_bindings(),
        before.callback_bindings()
    );
    for (name, previous) in before.instances {
        let MountedState::Scalar(current) = &program.mounted[&name].state else {
            panic!("legacy fixture must use the scalar backend");
        };
        let current = current.snapshot().unwrap();
        assert_eq!(current.instance, previous.instance);
        assert_eq!(current.revision, previous.revision);
    }
}

#[test]
fn user_identity_keys_cannot_inject_module_path_segments() {
    let mut program = program();
    let key = "editor/@slot:0/0";
    let next = program
        .node_slots()
        .component(key, target(), label("encoded"))
        .unwrap();
    let projection = program.update_props(props(next, "save")).unwrap();
    assert!(
        projection
            .instances
            .keys()
            .all(|instance| !instance.contains(key))
    );
    let instance = projection
        .origins
        .values()
        .find(|origin| origin.module == path("editor.tsx"))
        .unwrap()
        .instance
        .clone();
    let again = program
        .node_slots()
        .component(key, target(), label("same identity"))
        .unwrap();
    assert!(
        program
            .update_props(props(again, "save"))
            .unwrap()
            .instances
            .contains_key(&instance)
    );
}
