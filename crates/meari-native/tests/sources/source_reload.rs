use meari::{
    application::Language,
    domain::{Component, component_state::StateValue},
};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

fn component(source: &str) -> Component {
    meari::parse(source, Language::Tsx).unwrap()
}
fn source() -> &'static str {
    include_str!("../../../meari/fixtures/state-counter.tsx")
}
fn document() -> InteractiveDocument {
    InteractiveDocument::new(
        component(source()),
        "button{width:100px;height:30px}input{width:100px;height:30px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
    )
    .unwrap()
}
fn activate(document: &mut InteractiveDocument) {
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
}

#[test]
fn native_source_commit_preserves_dirty_input_focus_nodes_and_component_state() {
    let mut document = document();
    activate(&mut document);
    document.set_input_value("0/4", "retained draft").unwrap();
    document.focus_path("0/4").unwrap();
    let input = document
        .renderer
        .document
        .get_element_by_id("name")
        .unwrap();
    let state = document.component_state().unwrap();
    let next = source()
        .replace("Count ", "Total ")
        .replace("previous + 1", "previous + 10");
    let prepared = document
        .prepare_source_replacement(component(&next))
        .unwrap();
    assert_eq!(document.component_state().unwrap(), state);
    document.commit_source_replacement(prepared).unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id("name"),
        Some(input)
    );
    assert_eq!(document.input_values["0/4"], "retained draft");
    assert_eq!(document.session.focus.as_deref(), Some("0/4"));
    assert_eq!(document.component_state().unwrap(), state);
    activate(&mut document);
    assert_eq!(
        document.component_state().unwrap().values["count"],
        StateValue::Number(serde_json::Number::from_f64(11.0).unwrap())
    );
    let status = document
        .renderer
        .document
        .get_element_by_id("status")
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(status)
            .unwrap()
            .text_content(),
        "Total 11 units"
    );
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    let input = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == input.as_u64())
        .unwrap();
    assert_eq!(input.1.value(), Some("retained draft"));
}

#[test]
fn native_source_shape_and_stale_rejections_leave_current_state_unchanged() {
    let mut document = document();
    let before = document.component_state().unwrap();
    let changed = source()
        .replace("<p id=\"status\">", "<div id=\"status\">")
        .replace("</p>", "</div>");
    assert!(
        document
            .prepare_source_replacement(component(&changed))
            .is_err()
    );
    assert_eq!(document.component_state().unwrap(), before);
    let changed = source().replace("id=\"name\"", "id=\"renamed\"");
    assert!(
        document
            .prepare_source_replacement(component(&changed))
            .is_err()
    );
    let prepared = document
        .prepare_source_replacement(component(source()))
        .unwrap();
    activate(&mut document);
    let latest = document.component_state().unwrap();
    assert!(document.commit_source_replacement(prepared).is_err());
    assert_eq!(document.component_state().unwrap(), latest);
}

#[test]
fn native_source_rejects_original_mixed_hook_reordering_before_binding() {
    let source = component(include_str!("../../../meari/fixtures/mixed-hooks.tsx"));
    let document = InteractiveDocument::new(
        source.clone(),
        "",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
    )
    .unwrap();
    let mut changed = source;
    changed.hook_order.swap(0, 1);
    assert!(document.prepare_source_replacement(changed).is_err());
}

#[test]
fn native_application_factory_keeps_original_prop_schema_for_replacement() {
    use meari::{
        application::app_bundle::{AppBuildStamp, ApplicationDefinition},
        domain::{Value, build::*},
    };
    use std::collections::BTreeMap;
    let source = source()
        .replace("type Props = {};", "type Props = { caption: string };")
        .replace("Count {count}", "{props.caption} {count}");
    let definition = ApplicationDefinition {
        component: component(&source),
        props: BTreeMap::from([("caption".into(), Value::String("First".into()))]),
        title: "Source schema fixture".into(),
        native_viewport: Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        stamp: AppBuildStamp {
            session: ContentHash::of(b"session"),
            generation: 1,
            graph_hash: ContentHash::of(b"graph"),
            target: BuildTarget {
                kind: TargetKind::Native,
                triple: "x86_64-pc-windows-msvc".into(),
            },
        },
    };
    let mut document = InteractiveDocument::from_application(&definition, "").unwrap();
    let prepared = document
        .prepare_source_replacement_with_props(
            definition.component.clone(),
            &BTreeMap::from([("caption".into(), Value::String("Next".into()))]),
        )
        .unwrap();
    document.commit_source_replacement(prepared).unwrap();
    let status = document
        .renderer
        .document
        .get_element_by_id("status")
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(status)
            .unwrap()
            .text_content(),
        "Next 0 units"
    );
}
