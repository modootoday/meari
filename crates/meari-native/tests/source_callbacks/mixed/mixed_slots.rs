use super::super::*;
#[path = "../../../../meari/fixtures/node-slots/mixed_support.rs"]
mod support;

fn mount_mixed() -> (InteractiveDocument, Host) {
    mount_program(
        support::program("native-mixed-dock", false, false),
        BTreeMap::from([("dock_open".into(), support::callback())]),
    )
}

fn values(
    document: &InteractiveDocument,
    key: &str,
    open: bool,
    narrow: bool,
) -> BTreeMap<String, Value> {
    support::values(
        |key, reference, props| document.node_slot(key, reference, props),
        key,
        open,
        narrow,
    )
    .unwrap()
}

#[test]
fn original_frame_dock_owned_editor_accepts_controlled_update_and_retains_dirty_nodes() {
    let (mut document, mut host) = mount_mixed();
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
    document
        .set_input_value(&input, "dirty mixed text")
        .unwrap();
    assert_eq!(text(&document, input_node), "dirty mixed text");
    document.focus_path(&input).unwrap();
    let close = values(&document, "editor", false, true);
    document
        .update_module_props_with_callbacks(close, host.callbacks.export_bindings().unwrap())
        .unwrap();
    let trigger = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 열기")
        })
        .unwrap()
        .path
        .clone();
    document
        .dispatch(Action::Activate { path: trigger })
        .unwrap();
    let stale = document.drain_callbacks().pop().unwrap();
    assert_eq!(stale.invocation.arguments, CallbackArguments::Boolean(true));
    let open = values(&document, "editor", true, true);
    document
        .update_module_props_with_callbacks(open, host.callbacks.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id(&input_id),
        Some(input_node)
    );
    assert_eq!(text(&document, input_node), "dirty mixed text");
    assert_eq!(
        host.callbacks
            .dispatch(
                &stale.handle,
                stale.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    document.focus_path(&input).unwrap();
    let accepted = values(&document, "editor", true, true);
    document
        .update_module_props_with_callbacks(accepted, host.callbacks.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(input_node)
    );
    assert_eq!(text(&document, input_node), "dirty mixed text");
    let tree = accessibility::tree(&document);
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == input_node.as_u64())
            .unwrap()
            .1
            .value(),
        Some("dirty mixed text")
    );
    assert!(
        document
            .elements()
            .iter()
            .any(|element| element.dom_tag == "label"
                && element.attributes.get("for") == Some(&input_id))
    );
    assert!(host.received.borrow().is_empty());
}

#[test]
fn mixed_slot_replacement_remounts_and_foreign_or_retired_leases_keep_last_good() {
    let (mut document, mut host) = mount_mixed();
    let root = document
        .module_projection()
        .unwrap()
        .instance("root")
        .unwrap()
        .clone();
    let input = support::tag_path(
        &document.module_projection().unwrap().component.root,
        "input",
        "0",
    )
    .unwrap();
    let original_id = document.path_id(&input).unwrap().to_owned();
    let original_node = document
        .renderer
        .document
        .get_element_by_id(&original_id)
        .unwrap();
    document
        .set_input_value(&input, "dirty retired editor")
        .unwrap();
    let visible_trigger = values(&document, "editor", false, true);
    document
        .update_module_props_with_callbacks(
            visible_trigger,
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    let trigger = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 열기")
        })
        .unwrap()
        .path
        .clone();
    document
        .dispatch(Action::Activate {
            path: trigger.clone(),
        })
        .unwrap();
    let old_callback = document.drain_callbacks().pop().unwrap();
    let replacement = values(&document, "replacement", false, true);
    document
        .update_module_props_with_callbacks(replacement, host.callbacks.export_bindings().unwrap())
        .unwrap();
    let replacement_path = support::tag_path(
        &document.module_projection().unwrap().component.root,
        "input",
        "0",
    )
    .unwrap();
    let replacement_node = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&replacement_path).unwrap())
        .unwrap();
    assert_ne!(replacement_node, original_node);
    assert!(!document.input_values.contains_key(&replacement_path));
    assert!(
        document
            .renderer
            .document
            .get_element_by_id(&original_id)
            .is_none()
    );
    assert_eq!(
        document.module_projection().unwrap().instance("root"),
        Some(&root)
    );
    assert_eq!(
        host.callbacks
            .dispatch(
                &old_callback.handle,
                old_callback.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    document
        .dispatch(Action::Activate { path: trigger })
        .unwrap();
    let fresh = document.drain_callbacks().pop().unwrap();
    host.callbacks
        .dispatch(
            &fresh.handle,
            fresh.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    assert_eq!(host.received.borrow().len(), 1);
    // Hidden text controls have no editor until they are laid out. Open through
    // the existing controlled props before asserting the actual fresh text.
    let visible_replacement = values(&document, "replacement", false, false);
    document
        .update_module_props_with_callbacks(
            visible_replacement,
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id(&replacement_path).unwrap()),
        Some(replacement_node)
    );
    assert_ne!(text(&document, replacement_node), "dirty retired editor");
    let alternate = document
        .node_slot(
            "editor",
            support::alternate_reference(),
            support::editor_props("Alternate"),
        )
        .unwrap();
    let dock = document
        .node_slot(
            "dock",
            support::dock_reference(),
            support::dock_props(alternate, false, true),
        )
        .unwrap();
    document
        .update_module_props_with_callbacks(
            support::frame_props(dock),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    let alternate_path = support::tag_path(
        &document.module_projection().unwrap().component.root,
        "input",
        "0",
    )
    .unwrap();
    let alternate_node = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&alternate_path).unwrap())
        .unwrap();
    assert_ne!(alternate_node, replacement_node);
    assert_eq!(
        document.module_projection().unwrap().instance("root"),
        Some(&root)
    );
    let before_foreign = document.module_projection().unwrap().component.clone();
    let foreign = support::program("foreign-mixed", false, false)
        .node_slots()
        .component(
            "foreign",
            support::reference(),
            support::editor_props("Foreign"),
        )
        .unwrap();
    assert!(
        document
            .node_slot(
                "dock",
                support::dock_reference(),
                support::dock_props(foreign, false, false)
            )
            .is_err()
    );
    assert!(
        document
            .module_projection()
            .unwrap()
            .component
            .semantic_eq(&before_foreign)
    );
    let retired = document
        .node_slot(
            "unused",
            support::reference(),
            support::editor_props("Retired"),
        )
        .unwrap();
    let accepted = values(&document, "editor", false, false);
    document
        .update_module_props_with_callbacks(accepted, host.callbacks.export_bindings().unwrap())
        .unwrap();
    let before = document.module_projection().unwrap().component.clone();
    assert!(
        document
            .node_slot(
                "dock",
                support::dock_reference(),
                support::dock_props(retired, false, false)
            )
            .is_err()
    );
    assert!(
        document
            .module_projection()
            .unwrap()
            .component
            .semantic_eq(&before)
    );
}

#[test]
fn mixed_dock_failed_focus_patch_restores_dom_program_and_previous_callback_export() {
    let (mut document, mut host) = mount_mixed();
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
    document
        .set_input_value(&input, "dirty before rollback")
        .unwrap();
    assert_eq!(text(&document, input_node), "dirty before rollback");
    let close = values(&document, "editor", false, true);
    document
        .update_module_props_with_callbacks(close, host.callbacks.export_bindings().unwrap())
        .unwrap();
    let trigger = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 열기")
        })
        .unwrap()
        .path
        .clone();
    document.focus_path(&trigger).unwrap();
    let trigger_node = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&trigger).unwrap())
        .unwrap();
    document
        .dispatch(Action::Activate { path: trigger })
        .unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    let before = document.module_projection().unwrap().component.clone();
    document
        .replace_stylesheet("input,[role=dialog]{visibility:hidden}")
        .unwrap();
    let open = values(&document, "editor", true, true);
    let failure = document
        .update_module_props_with_callbacks(open, host.callbacks.export_bindings().unwrap())
        .unwrap_err();
    assert!(failure.contains("no visible focus target"), "{failure}");
    assert!(
        document
            .module_projection()
            .unwrap()
            .component
            .semantic_eq(&before)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&input_id),
        Some(input_node)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(trigger_node)
    );
    assert_eq!(text(&document, input_node), "dirty before rollback");
    host.callbacks
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    assert_eq!(host.received.borrow().len(), 1);
}
