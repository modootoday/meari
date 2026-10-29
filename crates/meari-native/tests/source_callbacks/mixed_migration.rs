use super::*;
#[path = "../../../meari/fixtures/mixed-slot-migration/support.rs"]
mod support;

fn labeled(document: &InteractiveDocument, label: &str) -> String {
    document
        .elements()
        .iter()
        .find(|element| element.attributes.get("aria-label").map(String::as_str) == Some(label))
        .unwrap()
        .path
        .clone()
}

#[test]
fn mixed_slot_movement_keeps_native_nodes_dirty_focus_a11y_and_revokes_old_callbacks() {
    let (mut document, mut host) =
        mount_program(support::program("native-movable"), support::bindings());
    let path = labeled(&document, "owned");
    let fixed = labeled(&document, "fixed");
    let id = document.path_id(&path).unwrap().to_owned();
    let fixed_id = document.path_id(&fixed).unwrap().to_owned();
    let node = document.renderer.document.get_element_by_id(&id).unwrap();
    let fixed_node = document
        .renderer
        .document
        .get_element_by_id(&fixed_id)
        .unwrap();
    document.set_input_value(&path, "dirty owned").unwrap();
    document.set_input_value(&fixed, "dirty fixed").unwrap();
    document.focus_path(&path).unwrap();
    let callback = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    document
        .dispatch(Action::Activate { path: callback })
        .unwrap();
    let stale = document.drain_callbacks().pop().unwrap();
    document.focus_path(&path).unwrap();
    for right in [true, false, true] {
        let slot = document
            .movable_node_slot("editor", support::reference(), support::editor())
            .unwrap();
        document
            .update_module_props_with_callbacks(
                support::props(slot, right),
                host.callbacks.export_bindings().unwrap(),
            )
            .unwrap();
        assert_eq!(
            document.renderer.document.get_element_by_id(&id),
            Some(node)
        );
        assert_eq!(
            document.renderer.document.get_element_by_id(&fixed_id),
            Some(fixed_node)
        );
        assert_eq!(text(&document, node), "dirty owned");
        assert_eq!(text(&document, fixed_node), "dirty fixed");
        assert_eq!(
            document.renderer.document.get_focussed_node_id(),
            Some(node)
        );
        assert_eq!(
            document.session.focus.as_deref(),
            Some(labeled(&document, "owned").as_str())
        );
        let tree = accessibility::tree(&document);
        assert!(tree.nodes.iter().any(|(id, _)| id.0 == node.as_u64()));
        assert_eq!(tree.focus.0, node.as_u64());
    }
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
    let path = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    document.dispatch(Action::Activate { path }).unwrap();
    let fresh = document.drain_callbacks().pop().unwrap();
    host.callbacks
        .dispatch(
            &fresh.handle,
            fresh.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    assert_eq!(host.received.borrow().len(), 1);
}

#[test]
fn mixed_removal_retires_focus_a11y_paths_callbacks_and_failed_update_keeps_frame() {
    let (mut document, mut host) =
        mount_program(support::program("native-remove"), support::bindings());
    let path = labeled(&document, "owned");
    let id = document.path_id(&path).unwrap().to_owned();
    let node = document.renderer.document.get_element_by_id(&id).unwrap();
    document.focus_path(&path).unwrap();
    let callback = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    document
        .dispatch(Action::Activate {
            path: callback.clone(),
        })
        .unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    assert!(
        document
            .update_module_props_with_callbacks(
                BTreeMap::new(),
                host.callbacks.export_bindings().unwrap()
            )
            .is_err()
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&id),
        Some(node)
    );
    host.callbacks
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    document
        .dispatch(Action::Activate {
            path: callback.clone(),
        })
        .unwrap();
    let stale = document.drain_callbacks().pop().unwrap();
    document
        .update_module_props_with_callbacks(
            support::props(support::empty(), false),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(document.renderer.document.get_element_by_id(&id).is_none());
    assert_ne!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    assert_ne!(document.session.focus.as_deref(), Some(path.as_str()));
    assert!(
        !accessibility::tree(&document)
            .nodes
            .iter()
            .any(|(id, _)| id.0 == node.as_u64())
    );
    assert!(
        document
            .dispatch(Action::Activate { path: callback })
            .is_err()
    );
    assert!(document.drain_callbacks().is_empty());
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
    let slot = document
        .movable_node_slot("editor", support::reference(), support::editor())
        .unwrap();
    document
        .update_module_props_with_callbacks(
            support::props(slot, true),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert_ne!(
        document.renderer.document.get_element_by_id(&id),
        Some(node)
    );
}
