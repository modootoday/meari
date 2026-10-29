use meari::{
    application::source_modules::ModuleProgram,
    domain::{
        Value,
        build::BuildPath,
        node_slots::NodeSlot,
        source_modules::{ComponentReference, SourceModuleGraph},
    },
};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn reference() -> ComponentReference {
    ComponentReference {
        module: path("editor.tsx"),
        export: "Editor".into(),
    }
}
fn graph() -> SourceModuleGraph {
    let root = "import type {ReactNode} from 'react';type Props={left:ReactNode;right:ReactNode};export function Root(props:Props){return <main>{props.left}<input defaultValue=\"fixed\"/>{props.right}<p>tail</p></main>}";
    let editor = "import {useId,useState} from 'react';type Props={};export function Editor(props:Props){const field=useId();const [text,setText]=useState('cold');return <article><input id={field}/><button onClick={()=>setText('hot')}>{text}</button></article>}";
    let mut graph = meari::parse_module_graph(
        &path("root.tsx"),
        &BTreeMap::from([(path("root.tsx"), root.into())]),
    )
    .unwrap();
    graph.modules.extend(
        meari::parse_module_graph(
            &path("editor.tsx"),
            &BTreeMap::from([(path("editor.tsx"), editor.into())]),
        )
        .unwrap()
        .modules,
    );
    graph.validate().unwrap();
    graph
}
fn props(left: Value, right: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([("left".into(), left), ("right".into(), right)])
}
fn empty() -> Value {
    Value::NodeSlot(NodeSlot::Empty)
}
fn instance(program: &ModuleProgram) -> meari::domain::component_state::InstanceHandle {
    let projection = program.project().unwrap();
    let origin = projection
        .origins
        .values()
        .find(|origin| origin.module.as_str() == "editor.tsx")
        .unwrap();
    projection.instance(&origin.instance).unwrap().clone()
}
fn mount() -> ModuleProgram {
    ModuleProgram::mount_with_node_slots(graph(), "mixed-slots".into(), 1, |factory| {
        Ok(props(
            factory.movable_component("editor", reference(), BTreeMap::new())?,
            empty(),
        ))
    })
    .unwrap()
}

#[test]
fn movable_same_parent_identity_survives_movement_and_fresh_leases() {
    let mut program = mount();
    let original = instance(&program);
    let event = program
        .project()
        .unwrap()
        .event_paths()
        .next()
        .unwrap()
        .clone();
    program.activate(&event).unwrap();
    for right in [true, false, true] {
        let slot = program
            .node_slots()
            .movable_component("editor", reference(), BTreeMap::new())
            .unwrap();
        program
            .update_props(if right {
                props(empty(), slot)
            } else {
                props(slot, empty())
            })
            .unwrap();
        assert_eq!(instance(&program), original);
        fn hot(node: &meari::domain::Node) -> bool {
            match &node.kind {
                meari::domain::NodeKind::Text(Value::String(value)) => value == "hot",
                meari::domain::NodeKind::Element { children, .. } => children.iter().any(hot),
                _ => false,
            }
        }
        assert!(hot(&program.project().unwrap().component.root));
    }
}

#[test]
fn removal_retires_old_lease_and_reinsertion_creates_new_instance() {
    let mut program = mount();
    let original = instance(&program);
    let old = program
        .node_slots()
        .movable_component("editor", reference(), BTreeMap::new())
        .unwrap();
    program.update_props(props(old.clone(), empty())).unwrap();
    program.update_props(props(empty(), empty())).unwrap();
    assert!(
        !program
            .project()
            .unwrap()
            .origins
            .values()
            .any(|origin| origin.module.as_str() == "editor.tsx")
    );
    assert!(program.update_props(props(old, empty())).is_err());
    let new = program
        .node_slots()
        .movable_component("editor", reference(), BTreeMap::new())
        .unwrap();
    program.update_props(props(empty(), new)).unwrap();
    assert_ne!(instance(&program), original);
}

#[test]
fn duplicate_movable_key_is_rejected_without_changing_last_good() {
    let mut program = mount();
    let before = program.project().unwrap().component.clone();
    let original = instance(&program);
    let first = program
        .node_slots()
        .movable_component("editor", reference(), BTreeMap::new())
        .unwrap();
    let second = program
        .node_slots()
        .movable_component("editor", reference(), BTreeMap::new())
        .unwrap();
    assert!(program.update_props(props(first, second)).is_err());
    assert!(before.semantic_eq(&program.project().unwrap().component));
    assert_eq!(instance(&program), original);
}

#[test]
fn fixed_component_slot_keeps_its_existing_placement_identity_contract() {
    let mut program =
        ModuleProgram::mount_with_node_slots(graph(), "fixed-slots".into(), 1, |factory| {
            Ok(props(
                factory.component("editor", reference(), BTreeMap::new())?,
                empty(),
            ))
        })
        .unwrap();
    let original = instance(&program);
    let slot = program
        .node_slots()
        .component("editor", reference(), BTreeMap::new())
        .unwrap();
    program.update_props(props(empty(), slot)).unwrap();
    assert_ne!(instance(&program), original);
}

#[test]
fn moving_to_another_source_parent_remounts_instead_of_reusing_authority() {
    let mut graph = graph();
    let source = "import type {ReactNode} from 'react';type Props={left:ReactNode;right:ReactNode};export function Root(props:Props){return <main><section>{props.left}</section><section>{props.right}</section></main>}";
    let replacement = meari::parse_module_graph(
        &path("root.tsx"),
        &BTreeMap::from([(path("root.tsx"), source.into())]),
    )
    .unwrap();
    graph.modules.insert(
        path("root.tsx"),
        replacement.modules[&path("root.tsx")].clone(),
    );
    let mut program =
        ModuleProgram::mount_with_node_slots(graph, "separate-parents".into(), 1, |factory| {
            Ok(props(
                factory.movable_component("editor", reference(), BTreeMap::new())?,
                empty(),
            ))
        })
        .unwrap();
    let original = instance(&program);
    let slot = program
        .node_slots()
        .movable_component("editor", reference(), BTreeMap::new())
        .unwrap();
    program.update_props(props(empty(), slot)).unwrap();
    assert_ne!(instance(&program), original);
}
