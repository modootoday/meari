#[path = "../../fixtures/node-slots/support.rs"]
mod support;
use meari::infrastructure::web_document::WebDocument;
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{HtmlElement, HtmlInputElement};

#[wasm_bindgen_test]
fn browser_original_frame_fixed_workspace_and_owned_inspector_keep_dirty_focus() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{Value, node_slots::NodeSlot},
    };
    let browser = web_sys::window().unwrap().document().unwrap();
    let host = browser.create_element("section").unwrap();
    browser.body().unwrap().append_child(&host).unwrap();
    let program = ModuleProgram::mount_with_node_slots(
        support::graph(),
        "web-mixed-frame".into(),
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
    let input_path =
        support::tag_path(&program.project().unwrap().component.root, "input", "0").unwrap();
    let mut document = WebDocument::mount_module_program(
        &host,
        program,
        "input{width:180px;height:28px}",
        "web-mixed-frame".into(),
        1,
    )
    .unwrap();
    let input: HtmlInputElement = document.element(&input_path).unwrap().unchecked_into();
    input.set_value("dirty inspector");
    input.focus().unwrap();
    let inspector = document
        .node_slot(
            "inspector",
            support::reference(),
            support::editor_props("After"),
        )
        .unwrap();
    let mut props = support::frame_props(Value::NodeSlot(NodeSlot::Empty));
    props.insert("inspector".into(), inspector);
    document.update_module_props(props).unwrap();
    assert!(input.is_same_node(Some(document.element(&input_path).unwrap().as_ref())));
    assert_eq!(input.value(), "dirty inspector");
    assert!(
        browser
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    let replacement = document
        .node_slot(
            "replacement",
            support::reference(),
            support::editor_props("Replacement"),
        )
        .unwrap();
    let mut invalid = support::frame_props(Value::NodeSlot(NodeSlot::Empty));
    invalid.insert("inspector".into(), replacement);
    assert!(document.update_module_props(invalid).is_err());
    assert!(input.is_same_node(Some(document.element(&input_path).unwrap().as_ref())));
    assert_eq!(input.value(), "dirty inspector");
    document.close();
    host.remove();
}

#[wasm_bindgen_test]
fn browser_original_frame_slot_keeps_dirty_focus_and_detaches_replaced_workspace() {
    let browser = web_sys::window().unwrap().document().unwrap();
    let host = browser.create_element("section").unwrap();
    browser.body().unwrap().append_child(&host).unwrap();
    let program = support::program("webSlots");
    let tree = &program.project().unwrap().component.root;
    let input_path = support::tag_path(tree, "input", "0").unwrap();
    let button_path = support::tag_path(tree, "button", "0").unwrap();
    let mut document = WebDocument::mount_module_program(
        &host,
        program,
        "input{width:180px;height:28px} button{height:28px}",
        "webSlots".into(),
        1,
    )
    .unwrap();
    let root = document.element("0").unwrap();
    let input: HtmlInputElement = document.element(&input_path).unwrap().unchecked_into();
    let button: HtmlElement = document.element(&button_path).unwrap().unchecked_into();
    input.set_value("dirty local text");
    input.focus().unwrap();
    button.click();
    assert_eq!(button.text_content().unwrap(), "hot");
    let lease = document
        .node_slot(
            "editor",
            support::reference(),
            support::editor_props("After"),
        )
        .unwrap();
    document
        .update_module_props(support::frame_props(lease.clone()))
        .unwrap();
    let retained: HtmlInputElement = document.element(&input_path).unwrap().unchecked_into();
    assert!(input.is_same_node(Some(retained.as_ref())));
    assert_eq!(retained.value(), "dirty local text");
    assert!(
        browser
            .active_element()
            .unwrap()
            .is_same_node(Some(retained.as_ref()))
    );
    assert_eq!(
        document
            .element(&button_path)
            .unwrap()
            .text_content()
            .unwrap(),
        "hot"
    );
    assert_eq!(
        host.query_selector("label")
            .unwrap()
            .unwrap()
            .get_attribute("for"),
        retained.get_attribute("id")
    );
    let replacement = document
        .node_slot(
            "replacement",
            support::reference(),
            support::editor_props("Replacement"),
        )
        .unwrap();
    document
        .update_module_props(support::frame_props(replacement))
        .unwrap();
    assert!(root.is_same_node(Some(document.element("0").unwrap().as_ref())));
    assert!(!input.is_connected());
    assert!(!button.is_connected());
    let fresh: HtmlElement = document.element(&button_path).unwrap().unchecked_into();
    assert_eq!(fresh.text_content().unwrap(), "cold");
    button.click();
    assert_eq!(fresh.text_content().unwrap(), "cold");
    assert!(
        document
            .update_module_props(support::frame_props(lease))
            .is_err()
    );
    assert!(fresh.is_same_node(Some(document.element(&button_path).unwrap().as_ref())));
    assert!(document.update_module_props(BTreeMap::new()).is_err());
    drop(document);
    assert_eq!(host.child_element_count(), 0);
    host.remove();
}
