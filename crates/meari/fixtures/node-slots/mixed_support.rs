//! Original Frame/Dock composition through opaque owned node slots.
#[path = "support.rs"]
#[allow(dead_code)] // Existing fixture helpers serve several independently compiled test targets.
mod base;
pub use base::{editor_props, path, reference, tag_path};
use meari::{
    application::source_modules::{ModuleProgram, NodeSlotFactory},
    domain::{
        Value,
        host_callbacks::{CallbackReference, CallbackSignature},
        node_slots::NodeSlot,
        source_modules::{ComponentReference, SourceModuleGraph},
    },
};
use std::collections::BTreeMap;

pub fn graph() -> SourceModuleGraph {
    let mut graph = base::graph();
    let dock = meari::parse_module_graph(
        &path("studio/inspector-dock.tsx"),
        &BTreeMap::from([(
            path("studio/inspector-dock.tsx"),
            include_str!("studio/inspector-dock.tsx").into(),
        )]),
    )
    .unwrap();
    graph.modules.extend(dock.modules);
    let alternate = meari::parse_module_graph(&path("alternate.tsx"), &BTreeMap::from([
        (path("alternate.tsx"), "type Props={label:string};export function AlternateEditor(props:Props){return <article><label>{props.label}</label><input/></article>;}".into()),
    ])).unwrap();
    graph.modules.extend(alternate.modules);
    graph
}

pub fn alternate_reference() -> ComponentReference {
    ComponentReference {
        module: path("alternate.tsx"),
        export: "AlternateEditor".into(),
    }
}

pub fn callback() -> Value {
    Value::CallbackReference(CallbackReference {
        binding: "dock_open".into(),
        signature: CallbackSignature::Boolean,
    })
}

pub fn dock_reference() -> ComponentReference {
    ComponentReference {
        module: path("studio/inspector-dock.tsx"),
        export: "InspectorDock".into(),
    }
}

pub fn dock_props(children: Value, open: bool, narrow: bool) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("children".into(), children),
        ("title".into(), Value::String("Mixed slot inspector".into())),
        ("open".into(), Value::Bool(open)),
        ("narrow".into(), Value::Bool(narrow)),
        ("onOpenChange".into(), callback()),
    ])
}

pub fn frame_props(inspector: Value) -> BTreeMap<String, Value> {
    let mut props = base::frame_props(Value::NodeSlot(NodeSlot::Empty));
    props.insert("inspector".into(), inspector);
    props
}

pub fn values(
    mut component: impl FnMut(
        &str,
        ComponentReference,
        BTreeMap<String, Value>,
    ) -> Result<Value, String>,
    editor_key: &str,
    open: bool,
    narrow: bool,
) -> Result<BTreeMap<String, Value>, String> {
    let editor = component(editor_key, reference(), editor_props("Mixed editor"))?;
    let dock = component("dock", dock_reference(), dock_props(editor, open, narrow))?;
    Ok(frame_props(dock))
}

pub fn factory_values(
    factory: &NodeSlotFactory<'_>,
    open: bool,
    narrow: bool,
) -> Result<BTreeMap<String, Value>, String> {
    values(
        |key, reference, props| factory.component(key, reference, props),
        "editor",
        open,
        narrow,
    )
}

pub fn program(document: &str, open: bool, narrow: bool) -> ModuleProgram {
    ModuleProgram::mount_with_node_slots(graph(), document.into(), 1, |factory| {
        factory_values(factory, open, narrow)
    })
    .unwrap()
}
