use meari::{
    application::source_modules::ModuleProgram,
    domain::{Value, build::BuildPath, component_state::StateValue},
};
use meari_native::{
    application::Renderer,
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn viewport() -> Viewport {
    Viewport {
        width: 640,
        height: 480,
        dark: false,
    }
}

#[test]
fn original_queue_footer_state_negation_updates_hidden_and_aria_on_retained_nodes() {
    let entry = path("studio/queue-footer.tsx");
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([(
            entry.clone(),
            include_str!("../../../meari/fixtures/node-slots/studio/queue-footer.tsx").into(),
        )]),
    )
    .unwrap();
    let values = BTreeMap::from([
        ("summaryLabel".into(), Value::String("Summary".into())),
        ("detailsLabel".into(), Value::String("Details".into())),
    ]);
    let program = ModuleProgram::mount(graph, values, "queue-state-predicate".into(), 1).unwrap();
    let mut document = InteractiveDocument::from_module_program(
        program,
        "button{width:120px;height:30px}",
        viewport(),
    )
    .unwrap();
    let button = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0/0").unwrap())
        .unwrap();
    let section = document
        .renderer
        .document
        .get_element_by_id("studio-queue-details")
        .unwrap();
    document.focus_path("0/0/0").unwrap();
    for expanded in [true, false, true] {
        document
            .dispatch(Action::Activate {
                path: "0/0/0".into(),
            })
            .unwrap();
        assert_eq!(
            document
                .renderer
                .document
                .get_element_by_id(document.path_id("0/0/0").unwrap()),
            Some(button)
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_element_by_id("studio-queue-details"),
            Some(section)
        );
        assert_eq!(
            document.renderer.document.get_focussed_node_id(),
            Some(button)
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_node(button)
                .unwrap()
                .attr(blitz_dom::LocalName::from("aria-expanded")),
            Some(if expanded { "true" } else { "false" })
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_node(section)
                .unwrap()
                .attr(blitz_dom::LocalName::from("hidden"))
                .is_some(),
            !expanded
        );
        let tree = meari_native::infrastructure::accessibility::tree(&document);
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| id.0 == button.as_u64())
                .unwrap()
                .1
                .is_expanded(),
            Some(expanded)
        );
    }
}

#[test]
fn keyed_rows_retain_native_nodes_focus_and_component_state_after_structural_updates() {
    use meari::runtime::IntoValue;
    let sources = BTreeMap::from([
        (path("list.tsx"), "import {Counter} from './counter';type Props={refs:string[]};export function List(props:Props){return <ul>{props.refs.map(reference => <Counter key={reference} label={reference}/>)}</ul>;}".into()),
        (path("counter.tsx"), "import {useState} from 'react';type Props={label:string};export function Counter(props:Props){const [count,setCount]=useState(0);return <li><button onClick={()=>setCount(previous=>previous+1)}><span>{props.label}</span>{count}</button></li>;}".into()),
    ]);
    let graph = meari::parse_module_graph(&path("list.tsx"), &sources).unwrap();
    let values = |refs: Vec<&str>| BTreeMap::from([("refs".into(), refs.into_value())]);
    let program =
        ModuleProgram::mount(graph, values(vec!["a", "b"]), "native-repeat".into(), 1).unwrap();
    let mut document = InteractiveDocument::from_module_program(
        program,
        "button{width:100px;height:30px}",
        viewport(),
    )
    .unwrap();
    let row_id = document.path_id("0/0").unwrap().to_owned();
    let button_id = document.path_id("0/0/0").unwrap().to_owned();
    let row_node = document
        .renderer
        .document
        .get_element_by_id(&row_id)
        .unwrap();
    let button_node = document
        .renderer
        .document
        .get_element_by_id(&button_id)
        .unwrap();
    document.focus_path("0/0/0").unwrap();
    document
        .dispatch(Action::Activate {
            path: "0/0/0".into(),
        })
        .unwrap();
    document
        .update_module_props(values(vec!["b", "new", "a"]))
        .unwrap();
    assert_eq!(document.path_id("0/2"), Some(row_id.as_str()));
    assert_eq!(
        document.renderer.document.get_element_by_id(&row_id),
        Some(row_node)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&button_id),
        Some(button_node)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(button_node)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(row_node)
            .unwrap()
            .text_content(),
        "a1"
    );
    document
        .dispatch(Action::Activate {
            path: "0/2/0".into(),
        })
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(row_node)
            .unwrap()
            .text_content(),
        "a2"
    );
    let before = document.module_projection().unwrap().component.clone();
    assert!(
        document
            .update_module_props(values(vec!["same", "same"]))
            .is_err()
    );
    assert_eq!(document.module_projection().unwrap().component, before);
    assert_eq!(
        document.renderer.document.get_element_by_id(&row_id),
        Some(row_node)
    );
    document.update_module_props(values(vec!["b"])).unwrap();
    assert!(
        document
            .renderer
            .document
            .get_element_by_id(&row_id)
            .is_none()
    );
    assert!(document.renderer.document.get_node(row_node).is_none());
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    assert!(!tree.nodes.iter().any(|(id, _)| id.0 == row_node.as_u64()));
}

#[test]
fn module_application_factory_scopes_separate_mounts_and_keeps_declared_stamp() {
    use meari::{
        application::{
            app_bundle::{AppBuildStamp, ApplicationDefinition},
            module_app_bundle::ModuleApplicationDefinition,
        },
        domain::build::{BuildTarget, ContentHash, TargetKind},
    };
    let sources = BTreeMap::from([
        (path("view.tsx"), "import {Counter} from './counter';type Props={};export function View(props:Props){return <main><Counter/></main>; }".into()),
        (path("counter.tsx"), "import {useState} from 'react';type Props={};export function Counter(props:Props){const [count,setCount]=useState(0);return <button onClick={()=>setCount(previous=>previous+1)}>{count}</button>; }".into()),
    ]);
    let modules = meari::parse_module_graph(&path("view.tsx"), &sources).unwrap();
    let definition = ModuleApplicationDefinition {
        schema_version: 1,
        application: ApplicationDefinition {
            component: modules
                .definition(&modules.entry)
                .unwrap()
                .component
                .clone(),
            props: BTreeMap::new(),
            title: "Module application".into(),
            native_viewport: viewport(),
            stamp: AppBuildStamp {
                session: ContentHash::of(b"session"),
                generation: 7,
                graph_hash: modules.fingerprint().unwrap(),
                target: BuildTarget {
                    kind: TargetKind::Native,
                    triple: "x86_64-pc-windows-msvc".into(),
                },
            },
        },
        modules,
    };
    let mut first = InteractiveDocument::from_module_application(
        &definition,
        "button{width:120px;height:32px}",
    )
    .unwrap();
    let second = InteractiveDocument::from_module_application(&definition, "").unwrap();
    let first_root = first.module_projection().unwrap().instances["root"]
        .instance
        .clone();
    assert_ne!(
        first_root,
        second.module_projection().unwrap().instances["root"].instance
    );
    first
        .dispatch(Action::Activate { path: "0/0".into() })
        .unwrap();
    assert_eq!(
        first.module_projection().unwrap().instances["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    assert_eq!(
        second.module_projection().unwrap().instances["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(0.0).unwrap())
    );
    assert_eq!(first.application_stamp().unwrap().generation, 7);
    let mut invalid = definition.clone();
    invalid.application.title.clear();
    assert!(invalid.program("invalid".into(), 1).is_err());
    assert!(InteractiveDocument::from_module_application(&invalid, "").is_err());
}

#[test]
fn native_module_events_preserve_independent_child_state_and_live_input_focus() {
    let sources = BTreeMap::from([
        (path("parent.tsx"), "import { Counter } from './counter'; type Props={caption:string}; export function Parent(props:Props){return <main><Counter caption={props.caption}/><Counter caption={props.caption}/></main>; }".into()),
        (path("counter.tsx"), "import { useState,useId } from 'react'; type Props={caption:string}; export function Counter(props:Props){const field=useId(); const [count,setCount]=useState(0); return <section><label htmlFor={field}>{props.caption}</label><input id={field}/><button onClick={() => setCount(previous => previous+1)}>{count}</button></section>; }".into()),
    ]);
    let graph = meari::parse_module_graph(&path("parent.tsx"), &sources).unwrap();
    let program = ModuleProgram::mount(
        graph,
        BTreeMap::from([("caption".into(), Value::String("Before".into()))]),
        "native-modules".into(),
        1,
    )
    .unwrap();
    let mut document = InteractiveDocument::from_module_program(
        program,
        "input{width:150px;height:30px}",
        viewport(),
    )
    .unwrap();
    document.set_input_value("0/0/1", "retained🦀").unwrap();
    document.focus_path("0/0/1").unwrap();
    let input_id = document.path_id("0/0/1").unwrap().to_owned();
    let input = document
        .renderer
        .document
        .get_element_by_id(&input_id)
        .unwrap();
    assert!(
        document
            .dispatch(Action::Activate {
                path: "0/0/2".into()
            })
            .unwrap()
    );
    document
        .update_module_props(BTreeMap::from([(
            "caption".into(),
            Value::String("After".into()),
        )]))
        .unwrap();
    let projection = document.module_projection().unwrap();
    assert_eq!(
        projection.instances["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    assert_eq!(
        projection.instances["root@0/1"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(0.0).unwrap())
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&input_id),
        Some(input)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(input)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(input)
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text(),
        "retained🦀"
    );
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == input.as_u64())
            .unwrap()
            .1
            .value(),
        Some("retained🦀")
    );
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == input.as_u64())
            .unwrap()
            .1
            .label(),
        Some("After")
    );
    let pixels = document.renderer.rasterize(&viewport()).unwrap();
    assert_eq!(pixels.len(), 640 * 480 * 4);
}

#[test]
fn native_module_typed_inputs_commit_only_after_dom_patch_succeeds() {
    let graph = meari::parse_module_graph(
        &path("input.tsx"),
        &BTreeMap::from([(
            path("input.tsx"),
            include_str!("../../../meari/fixtures/input-state.tsx").into(),
        )]),
    )
    .unwrap();
    let program =
        ModuleProgram::mount(graph, BTreeMap::new(), "native-module-input".into(), 1).unwrap();
    let mut document = InteractiveDocument::from_module_program(
        program,
        "input{width:150px;height:30px}",
        viewport(),
    )
    .unwrap();
    document.set_input_value("0/0", "shared graph🦀").unwrap();
    let before = document.module_projection().unwrap().instances.clone();
    assert_eq!(
        before["root"].values["text"],
        StateValue::String("shared graph🦀".into())
    );
    let editor = document
        .renderer
        .document
        .get_element_by_id("editor")
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(editor)
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text(),
        "shared graph🦀"
    );
    let check = document
        .renderer
        .document
        .get_element_by_id("check")
        .unwrap();
    let name = blitz_dom::QualName::new(
        None,
        blitz_dom::Namespace::from(""),
        blitz_dom::LocalName::from("id"),
    );
    document
        .renderer
        .document
        .mutate()
        .set_attribute(check, name, "external-check");
    assert!(document.set_input_value("0/0", "uncommitted").is_err());
    assert_eq!(document.module_projection().unwrap().instances, before);
    assert_eq!(
        document
            .renderer
            .document
            .get_node(editor)
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text(),
        "shared graph🦀"
    );
}
