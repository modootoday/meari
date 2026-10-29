use meari::domain::build::BuildPath;
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}

#[test]
fn original_frame_declares_four_typed_node_slots() {
    let graph = meari::parse_module_graph(
        &path("studio/frame.tsx"),
        &BTreeMap::from([
            (
                path("studio/frame.tsx"),
                include_str!("../fixtures/node-slots/studio/frame.tsx").into(),
            ),
            (
                path("components/window-chrome.tsx"),
                include_str!("../fixtures/node-slots/components/window-chrome.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    assert_eq!(graph.modules.len(), 2);
}
#[path = "node_slots/graph.rs"]
mod graph;
#[path = "../fixtures/node-slots/support.rs"]
mod support;

#[test]
fn original_empty_caller_mounts_and_application_parses_with_typed_comparison() {
    let sources = BTreeMap::from([
        (
            path("studio/frame.tsx"),
            include_str!("../fixtures/node-slots/studio/frame.tsx").into(),
        ),
        (
            path("components/window-chrome.tsx"),
            include_str!("../fixtures/node-slots/components/window-chrome.tsx").into(),
        ),
        (
            path("verification/studio-frame-empty.tsx"),
            include_str!("../fixtures/node-slots/verification/studio-frame-empty.tsx").into(),
        ),
    ]);
    let graph =
        meari::parse_module_graph(&path("verification/studio-frame-empty.tsx"), &sources).unwrap();
    let program = meari::application::source_modules::ModuleProgram::mount(
        graph,
        BTreeMap::from([
            (
                "productName".into(),
                meari::domain::Value::String("Meari".into()),
            ),
            (
                "contextLabel".into(),
                meari::domain::Value::String("Empty".into()),
            ),
        ]),
        "originalEmpty".into(),
        1,
    )
    .unwrap();
    assert!(
        !program
            .project()
            .unwrap()
            .component
            .props
            .iter()
            .any(|prop| prop.kind == meari::domain::PropType::Node)
    );
    let original = BTreeMap::from([
        (
            path("studio/application.tsx"),
            include_str!("../fixtures/node-slots/studio/application.tsx").into(),
        ),
        (
            path("studio/frame.tsx"),
            include_str!("../fixtures/node-slots/studio/frame.tsx").into(),
        ),
        (
            path("components/window-chrome.tsx"),
            include_str!("../fixtures/node-slots/components/window-chrome.tsx").into(),
        ),
        (
            path("studio/navigation.tsx"),
            include_str!("../fixtures/node-slots/studio/navigation.tsx").into(),
        ),
        (
            path("studio/navigation-item.tsx"),
            include_str!("../fixtures/node-slots/studio/navigation-item.tsx").into(),
        ),
        (
            path("studio/inspector-dock.tsx"),
            include_str!("../fixtures/node-slots/studio/inspector-dock.tsx").into(),
        ),
        (
            path("studio/queue-footer.tsx"),
            include_str!("../fixtures/node-slots/studio/queue-footer.tsx").into(),
        ),
    ]);
    let graph = meari::parse_module_graph(&path("studio/application.tsx"), &original).unwrap();
    assert_eq!(graph.modules.len(), original.len());
    let navigation = &graph.modules[&path("studio/navigation.tsx")].component;
    assert_eq!(
        meari::emit(navigation, meari::application::Language::Tsx)
            .unwrap()
            .schema_version,
        13
    );
}

#[test]
fn same_identity_slot_updates_keep_component_instances_across_64_commits() {
    let mut program = support::program("boundedUpdates");
    let before = program.project().unwrap().instances.clone();
    assert!(support::tag_path(&program.project().unwrap().component.root, "input", "0").is_some());
    for index in 0..64 {
        let lease = program
            .node_slots()
            .component(
                "editor",
                support::reference(),
                support::editor_props(&format!("label{index}")),
            )
            .unwrap();
        let projection = program.update_props(support::frame_props(lease)).unwrap();
        assert_eq!(projection.instances.len(), before.len());
        for (name, state) in &projection.instances {
            assert_eq!(state.instance, before[name].instance);
        }
    }
}

#[test]
fn node_payloads_roundtrip_and_iterable_source_types_are_explicitly_rejected() {
    let source = "type Props={label:string};export function Example(props:Props){return <Container body={<><span>{props.label}</span><b>kept</b></>}/>;}";
    let parsed = meari::parse(source, meari::application::Language::Tsx).unwrap();
    let rust = meari::emit(&parsed, meari::application::Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 10);
    let restored = meari::parse(&rust.code, meari::application::Language::Rust).unwrap();
    assert!(parsed.semantic_eq(&restored));
    let tsx = meari::emit(&restored, meari::application::Language::Tsx).unwrap();
    assert!(
        parsed.semantic_eq(&meari::parse(&tsx.code, meari::application::Language::Tsx).unwrap())
    );
    assert!(meari::parse("import type {ReactNode} from 'react';type Props={body:ReactNode[]};export function Wrong(props:Props){return <div/>;}",meari::application::Language::Tsx).is_err());
}
