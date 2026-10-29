use super::*;
#[path = "../../../../fixtures/range-nested/support.rs"]
mod support;

fn fixture(
    name: &str,
) -> (
    WebDocument,
    Host,
    Element,
    HtmlInputElement,
    BTreeSet<String>,
) {
    let program = support::program(name);
    assert_eq!(support::instances(program.project().unwrap()).len(), 3);
    let paths = support::display_paths(&program.project().unwrap().component.root);
    let (document, host, container, _) = mount_program(program, name);
    let input = container
        .query_selector("input[aria-label='nested-range']")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    (document, host, container, input, paths)
}
fn update(document: &mut WebDocument, value: &str, paths: &BTreeSet<String>) {
    let slot = document
        .node_slot(
            "workspace",
            support::reference(),
            support::reader_props(value),
        )
        .unwrap();
    document
        .update_module_range_props_with_text(support::root_props(slot), paths)
        .unwrap();
}
#[wasm_bindgen_test]
fn browser_nested_owned_workspace_draft_display_and_terminal_keep_node_focus_label_and_callback_lifetime()
 {
    let (mut document, mut host, container, input, paths) = fixture("web-nested-feedback");
    let scope = document.callback_scope().unwrap();
    let label = container.query_selector("#range-label").unwrap().unwrap();
    let frame = container.query_selector("#frame").unwrap().unwrap();
    let preview = container.query_selector("#preview").unwrap().unwrap();
    let id = input.id();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].invocation.prop, "onDraft");
    assert_eq!(
        old[0].invocation.arguments,
        CallbackArguments::Text("50".into())
    );
    update(&mut document, "50", &paths);
    assert_eq!(document.callback_scope(), Some(scope));
    assert!(
        input.is_same_node(
            container
                .query_selector("input")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert!(
        label.is_same_node(
            container
                .query_selector("#range-label")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert!(
        frame.is_same_node(
            container
                .query_selector("#frame")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert!(
        preview.is_same_node(
            container
                .query_selector("#preview")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert_eq!(label.get_attribute("for"), Some(id));
    assert_eq!(input.value(), "50");
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    for (id, expected) in [
        ("preview", "preview-50"),
        ("position", "position-50"),
        ("duration", "duration-label-50"),
    ] {
        assert_eq!(
            container
                .query_selector(&format!("#{id}"))
                .unwrap()
                .unwrap()
                .text_content()
                .as_deref(),
            Some(expected)
        );
    }
    for event in old {
        host.owner
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert!(deliver(&mut document, &mut host).is_empty());
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "60");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("60".into()))]
    );
    update(&mut document, "60", &paths);
    key(&input, "keyup", "ArrowRight");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("60".into()))]
    );
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    assert!(!document.has_pending_callbacks());
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn browser_nested_foreign_retired_and_rebound_workspace_keep_last_good_or_cancel_original_terminal()
{
    let (mut document, mut host, container, input, paths) = fixture("web-nested-negative");
    let retired = document
        .node_slot(
            "workspace",
            support::reference(),
            support::reader_props("20"),
        )
        .unwrap();
    document
        .update_module_range_props_with_text(support::root_props(retired.clone()), &paths)
        .unwrap();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    update(&mut document, "50", &paths);
    let foreign_program = support::program("web-foreign-range");
    let foreign = foreign_program
        .node_slots()
        .component(
            "workspace",
            support::reference(),
            support::reader_props("50"),
        )
        .unwrap();
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(foreign), &paths)
            .unwrap_err()
            .contains("foreign")
    );
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(retired), &paths)
            .unwrap_err()
            .contains("retired")
    );
    let mut disabled = support::reader_props("50");
    disabled.insert("disabled".into(), Value::Bool(true));
    let disabled = document
        .node_slot("workspace", support::reference(), disabled)
        .unwrap();
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(disabled), &paths)
            .is_err()
    );
    let mut rebound = support::reader_props("50");
    rebound.insert("onDraft".into(), rebound["onCommit"].clone());
    let rebound = document
        .node_slot("workspace", support::reference(), rebound)
        .unwrap();
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(rebound), &paths)
            .is_err()
    );
    assert!(
        input.is_same_node(
            container
                .query_selector("input")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert_eq!(input.value(), "50");
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    for event in old {
        host.owner
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "60");
    let stale = document.drain_callbacks();
    assert_eq!(stale.len(), 1);
    let fresh = document
        .node_slot(
            "workspace",
            support::reference(),
            support::reader_props("60"),
        )
        .unwrap();
    document
        .update_module_props_with_callbacks(
            support::root_props(fresh),
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    for event in stale {
        assert_eq!(
            host.owner
                .dispatch(
                    &event.handle,
                    event.invocation.arguments,
                    &mut host.registry
                )
                .unwrap_err()
                .code,
            PluginErrorCode::Stale
        );
    }
    key(&input, "keyup", "ArrowRight");
    assert!(deliver(&mut document, &mut host).is_empty());
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "70");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("70".into()))]
    );
    key(&input, "keyup", "ArrowRight");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("70".into()))]
    );
    document.close();
    container.remove();
}
