#[path = "../../meari/fixtures/node-slots/support.rs"]
mod support;

#[test]
fn original_frame_fixed_workspace_and_owned_inspector_keep_dirty_focus_on_props_update() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{Value, node_slots::NodeSlot},
    };
    let program = ModuleProgram::mount_with_node_slots(
        support::graph(),
        "mixed-frame-only".into(),
        1,
        |factory| {
            let inspector = factory.component(
                "inspector",
                support::reference(),
                support::editor_props("Before"),
            )?;
            let mut props = support::frame_props(Value::NodeSlot(NodeSlot::Empty));
            props.insert("inspector".into(), inspector);
            Ok(props)
        },
    )
    .unwrap();
    let mut document = InteractiveDocument::from_module_program(
        program,
        "input{width:180px;height:28px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
    )
    .unwrap();
    let input = support::tag_path(
        &document.module_projection().unwrap().component.root,
        "input",
        "0",
    )
    .unwrap();
    let input_id = document.path_id(&input).unwrap().to_owned();
    let input_node = document
        .renderer
        .document
        .get_element_by_id(&input_id)
        .unwrap();
    document.set_input_value(&input, "dirty inspector").unwrap();
    document.focus_path(&input).unwrap();
    let inspector = document
        .node_slot(
            "inspector",
            support::reference(),
            support::editor_props("After"),
        )
        .unwrap();
    let mut props = support::frame_props(Value::NodeSlot(NodeSlot::Empty));
    props.insert("inspector".into(), inspector);
    props.insert("contextLabel".into(), Value::String("Updated frame".into()));
    document.update_module_props(props).unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id(&input_id),
        Some(input_node)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(input_node)
    );
    assert_eq!(
        document.input_values.get(&input).map(String::as_str),
        Some("dirty inspector")
    );
}
use meari_native::{
    application::Renderer,
    domain::{Viewport, widgets::Action},
    infrastructure::document::InteractiveDocument,
};

#[test]
fn original_frame_owned_slot_retains_dirty_focus_state_and_replaces_only_workspace() {
    let mut document = InteractiveDocument::from_module_program(
        support::program("nativeSlots"),
        "article{display:block} input{width:180px;height:28px} button{height:28px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
    )
    .unwrap();
    let tree = &document.module_projection().unwrap().component.root;
    let input = support::tag_path(tree, "input", "0").unwrap();
    let button = support::tag_path(tree, "button", "0").unwrap();
    let main = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0").unwrap())
        .unwrap();
    let input_id = document.path_id(&input).unwrap().to_owned();
    let input_node = document
        .renderer
        .document
        .get_element_by_id(&input_id)
        .unwrap();
    let button_id = document.path_id(&button).unwrap().to_owned();
    let button_node = document
        .renderer
        .document
        .get_element_by_id(&button_id)
        .unwrap();
    document
        .set_input_value(&input, "dirty local text")
        .unwrap();
    document.focus_path(&input).unwrap();
    document
        .dispatch(Action::Activate {
            path: button.clone(),
        })
        .unwrap();
    let updated = document
        .node_slot(
            "editor",
            support::reference(),
            support::editor_props("After"),
        )
        .unwrap();
    document
        .update_module_props(support::frame_props(updated.clone()))
        .unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id(&input_id),
        Some(input_node)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(input_node)
    );
    assert_eq!(
        document.input_values.get(&input).map(String::as_str),
        Some("dirty local text")
    );
    let current_button = document
        .renderer
        .document
        .get_node(button_node)
        .unwrap()
        .text_content();
    assert_eq!(current_button, "hot");
    let pixels = document
        .renderer
        .rasterize(&Viewport {
            width: 640,
            height: 480,
            dark: false,
        })
        .unwrap();
    assert_eq!(pixels.len(), 640 * 480 * 4);
    let new_workspace = document
        .node_slot(
            "replacement",
            support::reference(),
            support::editor_props("Replacement"),
        )
        .unwrap();
    document
        .update_module_props(support::frame_props(new_workspace))
        .unwrap();
    let replacement = document.path_id(&input).unwrap().to_owned();
    let replacement_node = document
        .renderer
        .document
        .get_element_by_id(&replacement)
        .unwrap();
    assert_ne!(replacement_node, input_node);
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0").unwrap()),
        Some(main)
    );
    let fresh_button = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&button).unwrap())
        .unwrap();
    assert_ne!(fresh_button, button_node);
    assert_eq!(
        document
            .renderer
            .document
            .get_node(fresh_button)
            .unwrap()
            .text_content(),
        "cold"
    );
    assert!(
        document
            .update_module_props(support::frame_props(updated))
            .is_err()
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&replacement),
        Some(replacement_node)
    );
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.role() == accesskit::Role::TextInput)
    );
}
