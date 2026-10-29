use meari::{
    application::source_modules::ModuleProgram,
    domain::{
        Value,
        build::BuildPath,
        host_callbacks::{CallbackReference, CallbackSignature},
        node_slots::NodeSlot,
        source_modules::{ComponentReference, SourceModuleGraph},
    },
};
use std::collections::BTreeMap;
pub fn reference() -> ComponentReference {
    ComponentReference {
        module: "editor.tsx".to_owned().try_into().unwrap(),
        export: "Editor".into(),
    }
}
pub fn empty() -> Value {
    Value::NodeSlot(NodeSlot::Empty)
}
pub fn bindings() -> BTreeMap<String, Value> {
    BTreeMap::from([(
        "save".into(),
        Value::CallbackReference(CallbackReference {
            binding: "save".into(),
            signature: CallbackSignature::Action,
        }),
    )])
}
pub fn props(slot: Value, right: bool) -> BTreeMap<String, Value> {
    if right {
        BTreeMap::from([("left".into(), empty()), ("right".into(), slot)])
    } else {
        BTreeMap::from([("left".into(), slot), ("right".into(), empty())])
    }
}
pub fn editor() -> BTreeMap<String, Value> {
    BTreeMap::from([("onSave".into(), bindings()["save"].clone())])
}
pub fn graph() -> SourceModuleGraph {
    let root: BuildPath = "root.tsx".to_owned().try_into().unwrap();
    let editor: BuildPath = "editor.tsx".to_owned().try_into().unwrap();
    let mut graph = meari::parse_module_graph(
        &root,
        &BTreeMap::from([(root.clone(), include_str!("root.tsx").into())]),
    )
    .unwrap();
    graph.modules.extend(
        meari::parse_module_graph(
            &editor,
            &BTreeMap::from([(editor.clone(), include_str!("editor.tsx").into())]),
        )
        .unwrap()
        .modules,
    );
    graph.validate().unwrap();
    graph
}
pub fn program(name: &str) -> ModuleProgram {
    ModuleProgram::mount_with_node_slots(graph(), name.into(), 1, |factory| {
        Ok(props(
            factory.movable_component("editor", reference(), editor())?,
            false,
        ))
    })
    .unwrap()
}
