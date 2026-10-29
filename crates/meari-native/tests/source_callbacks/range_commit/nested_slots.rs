use super::*;
#[path = "../../../../meari/fixtures/range-nested/support.rs"]
mod support;

fn fixture() -> (InteractiveDocument, Host, String) {
    let (mut document, host) =
        mount_program(support::program("native-nested-range"), support::bindings());
    document
        .replace_stylesheet(
            "input[type=range]{position:fixed;left:20px;top:20px;width:176px;height:24px}",
        )
        .unwrap();
    let path = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("nested-range")
        })
        .unwrap()
        .path
        .clone();
    (document, host, path)
}
fn update(document: &mut InteractiveDocument, value: &str, paths: &BTreeSet<String>) {
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
#[test]
fn native_nested_owned_workspace_draft_display_and_terminal_keep_identity_focus_and_callback_lifetime()
 {
    let (mut document, mut host, path) = fixture();
    let identities = support::instances(document.module_projection().unwrap());
    assert_eq!(identities.len(), 3);
    let paths = support::display_paths(&document.module_projection().unwrap().component.root);
    let handlers = document.module_projection().unwrap().callback_bindings();
    let scope = document.callback_scope().unwrap().clone();
    let id = document.path_id(&path).unwrap().to_owned();
    let node = document.renderer.document.get_element_by_id(&id).unwrap();
    let frame = document
        .renderer
        .document
        .get_element_by_id("frame")
        .unwrap();
    let label = document
        .renderer
        .document
        .get_element_by_id("range-label")
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    assert_eq!(old[0].invocation.prop, "onDraft");
    assert_eq!(
        old[0].invocation.arguments,
        CallbackArguments::Text("50".into())
    );
    update(&mut document, "50", &paths);
    assert_eq!(
        support::instances(document.module_projection().unwrap()),
        identities
    );
    assert_eq!(
        document.module_projection().unwrap().callback_bindings(),
        handlers
    );
    assert_eq!(document.callback_scope(), Some(&scope));
    assert_eq!(
        document.renderer.document.get_element_by_id("frame"),
        Some(frame)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id("range-label"),
        Some(label)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&id),
        Some(node)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(label)
            .unwrap()
            .attr(blitz_dom::LocalName::from("for")),
        Some(id.as_str())
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(node)
            .unwrap()
            .attr(blitz_dom::LocalName::from("value")),
        Some("50")
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    for (id, expected) in [
        ("preview", "preview-50"),
        ("position", "position-50"),
        ("duration", "duration-label-50"),
    ] {
        let label = document.renderer.document.get_element_by_id(id).unwrap();
        assert_eq!(
            document
                .renderer
                .document
                .get_node(label)
                .unwrap()
                .text_content(),
            expected
        );
    }
    let a11y = accessibility::tree(&document);
    assert!(a11y.nodes.iter().any(|(id, _)| id.0 == node.as_u64()));
    assert_eq!(a11y.focus.0, node.as_u64());
    for event in old {
        host.callbacks
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("60".into()))]
    );
    update(&mut document, "60", &paths);
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("60".into()))]
    );
    assert_eq!(
        support::instances(document.module_projection().unwrap()),
        identities
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    assert!(!document.has_pending_callbacks());
}

#[test]
fn native_nested_foreign_retired_and_rebound_workspace_keep_last_good_or_cancel_original_terminal()
{
    let (mut document, mut host, path) = fixture();
    let paths = support::display_paths(&document.module_projection().unwrap().component.root);
    let id = document.path_id(&path).unwrap().to_owned();
    let node = document.renderer.document.get_element_by_id(&id).unwrap();
    let identities = support::instances(document.module_projection().unwrap());
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
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    update(&mut document, "50", &paths);
    let foreign_program = support::program("foreign-native-range");
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
    assert_eq!(
        support::instances(document.module_projection().unwrap()),
        identities
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&id),
        Some(node)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    for event in old {
        host.callbacks
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
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
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    for event in stale {
        assert_eq!(
            host.callbacks
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
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert!(deliver(&mut document, &mut host).is_empty());
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("70".into()))]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("70".into()))]
    );
}
