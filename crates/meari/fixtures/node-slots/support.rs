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
pub const EDITOR: &str = "import {useState,useId} from 'react';type Props={label:string};export function SlotEditor(props:Props){const [value,setValue]=useState('cold');const field=useId();return <article><label htmlFor={field}>{props.label}</label><input id={field}/><button onClick={()=>setValue('hot')}>{value}</button></article>;}";
pub fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
pub fn reference() -> ComponentReference {
    ComponentReference {
        module: path("editor.tsx"),
        export: "SlotEditor".into(),
    }
}
pub fn editor_props(label: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("label".into(), Value::String(label.into()))])
}
pub fn frame_props(workspace: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("productName".into(), Value::String("Meari".into())),
        (
            "contextLabel".into(),
            Value::String("Slot verification".into()),
        ),
        ("workspace".into(), workspace),
        ("navigation".into(), Value::NodeSlot(NodeSlot::Empty)),
        ("inspector".into(), Value::NodeSlot(NodeSlot::Empty)),
        ("queue".into(), Value::NodeSlot(NodeSlot::Empty)),
    ])
}
pub fn graph() -> SourceModuleGraph {
    let mut graph = meari::parse_module_graph(
        &path("studio/frame.tsx"),
        &BTreeMap::from([
            (
                path("studio/frame.tsx"),
                include_str!("studio/frame.tsx").into(),
            ),
            (
                path("components/window-chrome.tsx"),
                include_str!("components/window-chrome.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    let provider = meari::parse_module_graph(
        &path("editor.tsx"),
        &BTreeMap::from([(path("editor.tsx"), EDITOR.into())]),
    )
    .unwrap();
    graph.modules.extend(provider.modules);
    graph.validate().unwrap();
    graph
}
pub fn program(document: &str) -> ModuleProgram {
    ModuleProgram::mount_with_node_slots(graph(), document.into(), 1, |factory| {
        Ok(frame_props(factory.component(
            "editor",
            reference(),
            editor_props("Before"),
        )?))
    })
    .unwrap()
}
pub fn tag_path(node: &meari::domain::Node, tag: &str, current: &str) -> Option<String> {
    if let meari::domain::NodeKind::Element {
        tag: name,
        children,
        ..
    } = &node.kind
    {
        if name == tag {
            return Some(current.into());
        }
        return children
            .iter()
            .enumerate()
            .find_map(|(index, child)| tag_path(child, tag, &format!("{current}/{index}")));
    }
    None
}
