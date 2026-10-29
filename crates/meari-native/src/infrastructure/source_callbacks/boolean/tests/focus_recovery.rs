//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

#[test]
fn public_dock_failed_focus_commit_preserves_dirty_dom_program_and_previous_export() {
    let (mut document, host, mut registry, calls) = mount_source(ORIGINAL_DOCK_ENTRY);
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    let editor = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("slot editor")
        })
        .unwrap()
        .clone();
    let editor_node = document
        .renderer
        .document
        .get_element_by_id(&editor.id)
        .unwrap();
    document
        .set_input_value(&editor.path, "dirty retained")
        .unwrap();
    document
        .update_module_props_with_callbacks(props(false, false), host.export_bindings().unwrap())
        .unwrap();
    let trigger = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 열기")
        })
        .unwrap()
        .clone();
    let trigger_node = document
        .renderer
        .document
        .get_element_by_id(&trigger.id)
        .unwrap();
    document.focus_path(&trigger.path).unwrap();
    document
        .dispatch(Action::Activate {
            path: trigger.path.clone(),
        })
        .unwrap();
    let pending = document.drain_callbacks().pop().unwrap();
    let before_program = document.module_projection().unwrap().component.clone();
    let before_source = document.source.clone();
    let content_before = document
        .elements()
        .iter()
        .find(|element| element.attributes.get("role").map(String::as_str) == Some("dialog"))
        .unwrap()
        .id
        .clone();
    let content_node = document
        .renderer
        .document
        .get_element_by_id(&content_before)
        .unwrap();
    document
        .replace_stylesheet("input,[role=dialog]{visibility:hidden}")
        .unwrap();
    let failure = document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap_err();
    assert!(failure.contains("no visible focus target"));
    assert_eq!(
        document.module_projection().unwrap().component,
        before_program
    );
    assert_eq!(document.source, before_source);
    assert!(document.session.widgets.values().all(|widget| !widget.open));
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(trigger_node)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&editor.id),
        Some(editor_node)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(editor_node)
            .unwrap()
            .element_data()
            .unwrap()
            .text_input_data()
            .unwrap()
            .editor
            .raw_text(),
        "dirty retained"
    );
    let content = document
        .elements()
        .iter()
        .find(|element| element.attributes.get("role").map(String::as_str) == Some("dialog"))
        .unwrap();
    assert!(content.attributes.contains_key("hidden"));
    assert_eq!(content.attributes["data-state"], "closed");
    assert_eq!(
        document.renderer.document.get_element_by_id(&content.id),
        Some(content_node)
    );
    let actual_content = document.renderer.document.get_node(content_node).unwrap();
    assert!(
        actual_content
            .attr(blitz_dom::LocalName::from("hidden"))
            .is_some()
    );
    assert_eq!(
        actual_content.attr(blitz_dom::LocalName::from("data-state")),
        Some("closed")
    );
    assert!(!document.source_input_poisoned);
    assert_eq!(calls.get(), 0);
    host.dispatch(
        &pending.handle,
        pending.invocation.arguments.clone(),
        &mut registry,
    )
    .unwrap();
    assert_eq!(calls.get(), 1);
    document.replace_stylesheet("").unwrap();
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    assert!(
        host.dispatch(&pending.handle, pending.invocation.arguments, &mut registry)
            .is_err()
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn original_composed_dock_actual_enter_space_and_accessibility_route_once() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    let (mut document, host, _registry, calls) = mount_source(ORIGINAL_DOCK_ENTRY);
    let trigger = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 열기")
        })
        .unwrap()
        .clone();
    let close = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("검사기 설정 닫기")
        })
        .unwrap()
        .clone();
    let key = |key, code| {
        UiEvent::KeyDown(BlitzKeyEvent {
            key,
            code,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        })
    };
    document.focus_path(&trigger.aliases[0]).unwrap();
    document.handle_ui_event(key(Key::Named(NamedKey::Enter), Code::Enter));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    document.focus_path(&close.aliases[0]).unwrap();
    document.handle_ui_event(key(Key::Character(" ".into()), Code::Space));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(false)
    );
    if let UiEvent::KeyDown(mut release) = key(Key::Character(" ".into()), Code::Space) {
        release.state = KeyState::Released;
        document.handle_ui_event(UiEvent::KeyUp(release));
        assert!(
            document.drain_callbacks().is_empty(),
            "Space release must not duplicate the close intent"
        );
    }
    let close_node = document
        .renderer
        .document
        .get_element_by_id(&close.id)
        .unwrap();
    crate::infrastructure::accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(close_node.as_u64()),
            data: None,
        },
    )
    .unwrap();
    assert_eq!(document.drain_callbacks().len(), 1);
    assert!(
        document
            .session
            .widgets
            .values()
            .any(|widget| widget.controlled_open && widget.open)
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn original_composed_dock_actual_pointer_hit_routes_single_open_intent() {
    use blitz_dom::Document;
    use blitz_traits::events::*;
    use keyboard_types::Modifiers;
    let (mut document, _host, _registry, calls) = mount_source(ORIGINAL_DOCK_ENTRY);
    document.replace_stylesheet(".aw-inspector-trigger button{position:fixed;left:16px;top:16px;width:100px;height:32px}").unwrap();
    let pointer = |buttons| BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: 20.0,
            page_y: 20.0,
            client_x: 20.0,
            client_y: 20.0,
            screen_x: 20.0,
            screen_y: 20.0,
        },
        button: MouseEventButton::Main,
        buttons,
        mods: Modifiers::empty(),
        details: PointerDetails::default(),
        element: Point::default(),
        active_pointers: Default::default(),
    };
    document.handle_ui_event(UiEvent::PointerMove(pointer(MouseEventButtons::None)));
    document.handle_ui_event(UiEvent::PointerDown(pointer(MouseEventButtons::Primary)));
    document.handle_ui_event(UiEvent::PointerUp(pointer(MouseEventButtons::None)));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    assert!(document.session.widgets.values().all(|widget| !widget.open));
    assert_eq!(calls.get(), 0);
}
