use meari::{application::Language, domain::component_state::StateValue};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

#[test]
fn mixed_hook_state_dispatch_preserves_native_label_and_input_identity() {
    let component = meari::parse(
        include_str!("../../../meari/fixtures/mixed-hooks.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        component,
        "button{width:100px;height:30px}",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )
    .unwrap();
    let input = document
        .renderer
        .document
        .get_element_by_id("bridge-hook-field")
        .unwrap();
    let toggle = document
        .elements()
        .iter()
        .find(|element| element.path == "0/4")
        .unwrap()
        .path
        .clone();
    assert!(
        document
            .dispatch(Action::Activate { path: toggle })
            .unwrap()
    );
    let increment = document
        .elements()
        .iter()
        .find(|element| element.path == "0/3")
        .unwrap()
        .path
        .clone();
    assert!(
        document
            .dispatch(Action::Activate { path: increment })
            .unwrap()
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id("bridge-hook-field"),
        Some(input)
    );
    let help = document
        .renderer
        .document
        .get_element_by_id("bridge-hook-help")
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(help)
            .unwrap()
            .text_content(),
        "1"
    );
}

#[test]
fn captured_arithmetic_and_functional_string_updates_reach_native_dom() {
    let component = meari::parse(
        include_str!("../../../meari/fixtures/state-capture.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        component,
        "button{width:100px;height:30px}",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )
    .unwrap();
    for id in [
        "capture",
        "capture",
        "step",
        "functional",
        "append",
        "append",
    ] {
        let path = document
            .elements()
            .iter()
            .find(|element| element.id == id)
            .unwrap()
            .path
            .clone();
        document.dispatch(Action::Activate { path }).unwrap();
    }
    let text = |id: &str| {
        document
            .renderer
            .document
            .get_node(document.renderer.document.get_element_by_id(id).unwrap())
            .unwrap()
            .text_content()
    };
    assert_eq!(text("count"), "14");
    assert_eq!(text("label"), "A!!");
}

#[test]
fn source_setters_patch_mixed_text_disabled_state_and_preserve_editable_identity() {
    let component = meari::parse(
        include_str!("../../../meari/fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        component,
        "button{width:100px;height:30px}input{width:100px;height:30px}",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )
    .unwrap();
    let path = |document: &InteractiveDocument, id: &str| {
        document
            .elements()
            .iter()
            .find(|element| element.id == id)
            .unwrap()
            .path
            .clone()
    };
    let input = path(&document, "name");
    document.set_input_value(&input, "retained input").unwrap();
    document.focus_path(&input).unwrap();
    let input_node = document
        .renderer
        .document
        .get_element_by_id("name")
        .unwrap();
    let units_node = document
        .renderer
        .document
        .get_element_by_id("units")
        .unwrap();
    let increment = path(&document, "increment");
    assert!(
        document
            .dispatch(Action::Activate {
                path: increment.clone()
            })
            .unwrap()
    );
    assert!(
        document
            .dispatch(Action::Key {
                path: increment.clone(),
                key: "Enter".into(),
                shift: false
            })
            .unwrap()
    );
    let rename = path(&document, "rename");
    document
        .dispatch(Action::Activate { path: rename })
        .unwrap();
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
        "Count 2 units ready🦀"
    );
    assert_eq!(
        document.renderer.document.get_element_by_id("units"),
        Some(units_node)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id("name"),
        Some(input_node)
    );
    assert_eq!(document.input_values[&input], "retained input");
    assert_eq!(
        document
            .renderer
            .document
            .get_node(input_node)
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .text(),
        "retained input"
    );
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    let input_a11y = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == input_node.as_u64())
        .unwrap();
    assert_eq!(input_a11y.1.value(), Some("retained input"));
    assert_eq!(document.session.focus.as_deref(), Some(input.as_str()));
    let toggle = path(&document, "toggle");
    document
        .dispatch(Action::Activate {
            path: toggle.clone(),
        })
        .unwrap();
    assert!(
        !document
            .dispatch(Action::Activate {
                path: increment.clone()
            })
            .unwrap()
    );
    assert_eq!(
        document.component_state().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
    assert_eq!(
        document.component_state().unwrap().values["count"],
        StateValue::Number(serde_json::Number::from_f64(2.0).unwrap())
    );
    document
        .dispatch(Action::Activate { path: toggle })
        .unwrap();
    document
        .dispatch(Action::Activate { path: increment })
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(status)
            .unwrap()
            .text_content(),
        "Count 3 units ready🦀"
    );
    let id = document
        .renderer
        .document
        .get_element_by_id("increment")
        .unwrap();
    meari_native::infrastructure::accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(id.as_u64()),
            data: None,
        },
    )
    .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_node(status)
            .unwrap()
            .text_content(),
        "Count 4 units ready🦀"
    );
}
