use super::*;
use web_sys::HtmlInputElement;
#[path = "../../../fixtures/mixed-slot-migration/support.rs"]
mod support;

fn same(node: &web_sys::Node, element: Option<Element>) -> bool {
    element.is_some_and(|element| node.is_same_node(Some(element.as_ref())))
}

#[wasm_bindgen_test]
fn browser_mixed_movement_preserves_dom_dirty_focus_and_fresh_callback_authority() {
    let (mut document, mut host, container, _) =
        mount_program(support::program("web-movable"), "web-movable");
    let input = container
        .query_selector("input[aria-label='owned']")
        .unwrap()
        .unwrap()
        .unchecked_into::<HtmlInputElement>();
    let fixed = container
        .query_selector("input[aria-label='fixed']")
        .unwrap()
        .unwrap()
        .unchecked_into::<HtmlInputElement>();
    let button = container
        .query_selector("button")
        .unwrap()
        .unwrap()
        .unchecked_into::<HtmlElement>();
    input.set_value("dirty owned");
    fixed.set_value("dirty fixed");
    button.click();
    let stale = document.drain_callbacks().pop().unwrap();
    input.focus().unwrap();
    input.set_selection_range(2, 7).unwrap();
    for right in [true, false, true] {
        let slot = document
            .movable_node_slot("editor", support::reference(), support::editor())
            .unwrap();
        document
            .update_module_props_with_callbacks(
                support::props(slot, right),
                host.owner.export_bindings().unwrap(),
            )
            .unwrap();
        assert!(same(
            input.as_ref(),
            container
                .query_selector("input[aria-label='owned']")
                .unwrap()
        ));
        assert!(same(
            fixed.as_ref(),
            container
                .query_selector("input[aria-label='fixed']")
                .unwrap()
        ));
        assert_eq!(input.value(), "dirty owned");
        assert_eq!(input.selection_start().unwrap(), Some(2));
        assert_eq!(input.selection_end().unwrap(), Some(7));
        assert_eq!(fixed.value(), "dirty fixed");
        assert!(same(
            input.as_ref(),
            container.owner_document().unwrap().active_element()
        ));
        let label = container.query_selector("label").unwrap().unwrap();
        assert_eq!(label.get_attribute("for"), input.get_attribute("id"));
    }
    assert_eq!(
        host.owner
            .dispatch(
                &stale.handle,
                stale.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    button.click();
    let fresh = document.drain_callbacks().pop().unwrap();
    host.owner
        .dispatch(
            &fresh.handle,
            fresh.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    assert_eq!(host.requests.borrow().len(), 1);
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn browser_mixed_removal_retires_detached_targets_and_failed_update_keeps_old_export() {
    let (mut document, mut host, container, _) =
        mount_program(support::program("web-remove"), "web-remove");
    let input = container
        .query_selector("input[aria-label='owned']")
        .unwrap()
        .unwrap()
        .unchecked_into::<HtmlInputElement>();
    let button = container
        .query_selector("button")
        .unwrap()
        .unwrap()
        .unchecked_into::<HtmlElement>();
    button.click();
    let pending = document.drain_callbacks().pop().unwrap();
    assert!(
        document
            .update_module_props_with_callbacks(
                BTreeMap::new(),
                host.owner.export_bindings().unwrap()
            )
            .is_err()
    );
    assert!(same(
        input.as_ref(),
        container
            .query_selector("input[aria-label='owned']")
            .unwrap()
    ));
    host.owner
        .dispatch(
            &pending.handle,
            pending.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    button.click();
    let stale = document.drain_callbacks().pop().unwrap();
    input.focus().unwrap();
    document
        .update_module_props_with_callbacks(
            support::props(support::empty(), false),
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(
        container
            .query_selector("input[aria-label='owned']")
            .unwrap()
            .is_none()
    );
    assert!(container.query_selector("label").unwrap().is_none());
    assert!(!same(
        input.as_ref(),
        container.owner_document().unwrap().active_element()
    ));
    button
        .dispatch_event(&Event::new("click").unwrap())
        .unwrap();
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(
        host.owner
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
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(!same(
        input.as_ref(),
        container
            .query_selector("input[aria-label='owned']")
            .unwrap()
    ));
    document.close();
    container.remove();
}
