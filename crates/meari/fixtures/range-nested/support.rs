use meari::{
    application::source_modules::{ModuleProgram, ModuleProjection},
    domain::{
        Node, NodeKind, Value,
        build::BuildPath,
        component_state::InstanceHandle,
        host_callbacks::{CallbackReference, CallbackSignature},
        source_modules::{ComponentReference, SourceModuleGraph},
    },
};
use std::collections::{BTreeMap, BTreeSet};
pub fn reference() -> ComponentReference {
    ComponentReference {
        module: "read-root.tsx".to_owned().try_into().unwrap(),
        export: "ReadRoot".into(),
    }
}
pub fn bindings() -> BTreeMap<String, Value> {
    ["onDraft", "onCommit"]
        .into_iter()
        .map(|name| {
            (
                name.into(),
                Value::CallbackReference(CallbackReference {
                    binding: name.into(),
                    signature: CallbackSignature::Text,
                }),
            )
        })
        .collect()
}
pub fn reader_props(value: &str) -> BTreeMap<String, Value> {
    let mut values = bindings();
    values.extend([
        ("value".into(), Value::String(value.into())),
        ("preview".into(), Value::String(format!("preview-{value}"))),
        (
            "positionLabel".into(),
            Value::String(format!("position-{value}")),
        ),
        (
            "durationLabel".into(),
            Value::String(format!("duration-label-{value}")),
        ),
        ("disabled".into(), Value::Bool(false)),
    ]);
    values
}
pub fn root_props(workspace: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([("workspace".into(), workspace)])
}
pub fn graph() -> SourceModuleGraph {
    let root: BuildPath = "product-frame.tsx".to_owned().try_into().unwrap();
    let reader: BuildPath = "read-root.tsx".to_owned().try_into().unwrap();
    let transport: BuildPath = "transport.tsx".to_owned().try_into().unwrap();
    let mut graph = meari::parse_module_graph(
        &root,
        &BTreeMap::from([(root.clone(), include_str!("product-frame.tsx").into())]),
    )
    .unwrap();
    graph.modules.extend(
        meari::parse_module_graph(
            &reader,
            &BTreeMap::from([
                (reader.clone(), include_str!("read-root.tsx").into()),
                (transport, include_str!("transport.tsx").into()),
            ]),
        )
        .unwrap()
        .modules,
    );
    graph.validate().unwrap();
    graph
}
pub fn program(name: &str) -> ModuleProgram {
    ModuleProgram::mount_with_node_slots(graph(), name.into(), 1, |factory| {
        Ok(root_props(factory.component(
            "workspace",
            reference(),
            reader_props("20"),
        )?))
    })
    .unwrap()
}
pub fn instances(projection: &ModuleProjection) -> BTreeMap<String, InstanceHandle> {
    projection
        .instances
        .iter()
        .map(|(key, state)| (key.clone(), state.instance.clone()))
        .collect()
}
pub fn display_paths(root: &Node) -> BTreeSet<String> {
    fn visit(node: &Node, path: &str, output: &mut BTreeSet<String>) {
        if let NodeKind::Element {
            tag,
            attributes,
            children,
        } = &node.kind
        {
            if tag=="span" && attributes.iter().any(|attribute| attribute.name=="id" && matches!(&attribute.value,Value::String(id) if ["preview","position","duration"].contains(&id.as_str()))) {
                assert_eq!(children.len(),1);
                assert!(matches!(&children[0].kind,NodeKind::Text(Value::String(_))));
                output.insert(format!("{path}/0"));
            }
            for (index, child) in children.iter().enumerate() {
                visit(child, &format!("{path}/{index}"), output);
            }
        }
    }
    let mut output = BTreeSet::new();
    visit(root, "0", &mut output);
    assert_eq!(output.len(), 3);
    output
}
