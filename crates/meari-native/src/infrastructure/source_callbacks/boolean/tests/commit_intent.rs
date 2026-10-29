//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::domain::Viewport;
use meari::application::host_callbacks::HostCallbacks;
use std::cell::Cell;

#[test]
fn intent_is_typed_deferred_and_authoritative_props_commit_expires_old_export() {
    let (mut document, host, mut registry, calls) = mount();
    document
        .dispatch(Action::Activate { path: "0".into() })
        .unwrap();
    assert_eq!(calls.get(), 0);
    assert_eq!(document.elements()[0].attributes["aria-checked"], "false");
    let pending = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        pending.invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    host.dispatch(
        &pending.handle,
        pending.invocation.arguments.clone(),
        &mut registry,
    )
    .unwrap();
    assert_eq!(calls.get(), 1);
    let node = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0").unwrap())
        .unwrap();
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    assert_eq!(document.elements()[0].attributes["aria-checked"], "true");
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0").unwrap()),
        Some(node)
    );
    assert!(
        host.dispatch(
            &pending.handle,
            CallbackArguments::Boolean(true),
            &mut registry
        )
        .is_err()
    );
    assert_eq!(calls.get(), 1);
    document
        .dispatch(Action::Activate { path: "0".into() })
        .unwrap();
    assert_eq!(
        document.drain_callbacks()[0].invocation.arguments,
        CallbackArguments::Boolean(false)
    );
}

#[test]
fn disabled_and_failed_props_do_not_produce_or_commit_state() {
    let (mut document, host, _registry, calls) = mount();
    let mut invalid = props(true, false);
    invalid.remove("checked");
    assert!(
        document
            .update_module_props_with_callbacks(invalid, host.export_bindings().unwrap())
            .is_err()
    );
    assert_eq!(document.elements()[0].attributes["aria-checked"], "false");
    document
        .update_module_props_with_callbacks(props(false, true), host.export_bindings().unwrap())
        .unwrap();
    assert!(
        document
            .dispatch(Action::Activate { path: "0".into() })
            .is_err()
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(calls.get(), 0);
}

#[test]
fn actual_keyboard_and_accessibility_emit_one_typed_intent_and_keep_accepted_state() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    let (mut document, _host, _registry, calls) = mount();
    document.focus_path("0").unwrap();
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
    document.handle_ui_event(key(Key::Named(NamedKey::Enter), Code::Enter));
    assert!(document.last_error.is_none());
    assert!(document.drain_callbacks().is_empty());
    document.handle_ui_event(key(Key::Character(" ".into()), Code::Space));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    let node = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0").unwrap())
        .unwrap();
    crate::infrastructure::accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(node.as_u64()),
            data: None,
        },
    )
    .unwrap();
    assert_eq!(document.drain_callbacks().len(), 1);
    let tree = crate::infrastructure::accessibility::tree(&document);
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == node.as_u64())
            .unwrap()
            .1
            .toggled(),
        Some(accesskit::Toggled::False)
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn public_factory_rejects_uncontrolled_boolean_before_exporting_host_authority() {
    let source = "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <Switch defaultChecked={props.checked} onCheckedChange={props.change}/>;}";
    let exports = Cell::new(0);
    let result = InteractiveDocument::from_module_program_with_callbacks(
        program_source(source),
        "",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        None,
        7,
        |_| {
            exports.set(exports.get() + 1);
            Err("unexpected export factory".into())
        },
    );
    assert!(
        result
            .err()
            .unwrap()
            .contains("explicit controlled Switch.checked")
    );
    assert_eq!(exports.get(), 0);
}

#[test]
fn public_factory_requires_a_live_registered_boolean_export() {
    let source = "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <Switch checked={props.checked} onCheckedChange={props.change}/>;}";
    let mut owner = None;
    let result = InteractiveDocument::from_module_program_with_callbacks(
        program_source(source),
        "",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        None,
        7,
        |scope| {
            let host = HostCallbacks::new(scope);
            let export = host.export_bindings().unwrap();
            owner = Some(host);
            Ok(export)
        },
    );
    assert!(
        result
            .err()
            .unwrap()
            .contains("callback reference is not exported")
    );
    assert!(owner.is_some());
}

#[test]
fn dialog_trigger_close_escape_and_overlay_wait_for_host_acceptance() {
    let (mut document, host, _registry, calls) = mount_source(DIALOG);
    document.focus_path("0/1/0").unwrap();
    document
        .dispatch(Action::Activate {
            path: "0/1/0".into(),
        })
        .unwrap();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    assert!(!document.session.widgets["0/1"].open);
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    for action in [
        Action::Activate {
            path: "0/1/1/1".into(),
        },
        Action::Key {
            path: "0/1/1/0".into(),
            key: "Escape".into(),
            shift: false,
        },
        Action::DismissOutside {
            path: "0/1/2".into(),
        },
    ] {
        document.dispatch(action).unwrap();
        let pending = document.drain_callbacks();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].invocation.arguments,
            CallbackArguments::Boolean(false)
        );
        assert!(document.session.widgets["0/1"].open);
    }
    document
        .update_module_props_with_callbacks(props(false, false), host.export_bindings().unwrap())
        .unwrap();
    assert!(!document.session.widgets["0/1"].open);
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0/1/0").unwrap())
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn replaced_dialog_target_with_same_markup_cannot_emit_using_live_root_owner() {
    let (mut document, _host, _registry, calls) = mount_source(DIALOG);
    let path = "0/1/0";
    let id = document.path_id(path).unwrap().to_owned();
    let original = document.renderer.document.get_element_by_id(&id).unwrap();
    let node = document.renderer.document.get_node(original).unwrap();
    let parent = node.parent.unwrap();
    let data = node.element_data().unwrap();
    let name = data.name.clone();
    let attrs = data.attrs.iter().cloned().collect();
    let replacement = {
        let mut mutation = document.renderer.document.mutate();
        mutation.remove_node(original);
        let replacement = mutation.create_element(name, attrs);
        mutation.append_children(parent, &[replacement]);
        replacement
    };
    assert_ne!(original, replacement);
    assert_eq!(
        document.renderer.document.get_element_by_id(&id),
        Some(replacement)
    );
    assert!(
        document
            .dispatch(Action::Activate { path: path.into() })
            .unwrap_err()
            .contains("action target was replaced")
    );
    assert!(document.drain_callbacks().is_empty());
    assert!(!document.session.widgets["0/1"].open);
    assert_eq!(calls.get(), 0);
}

#[test]
fn original_inspector_dock_composes_buttons_and_keeps_dirty_slot_across_host_acceptance() {
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
    assert_eq!(trigger.aliases.len(), 1);
    assert_eq!(close.aliases.len(), 1);
    assert_eq!(
        document.path_id(&trigger.path),
        document.path_id(&trigger.aliases[0])
    );
    assert_eq!(
        document
            .elements()
            .iter()
            .filter(|element| element.dom_tag == "button")
            .count(),
        2
    );
    document.focus_path(&trigger.aliases[0]).unwrap();
    document
        .dispatch(Action::Activate {
            path: trigger.aliases[0].clone(),
        })
        .unwrap();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
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
        .set_input_value(&editor.path, "dirty slot")
        .unwrap();
    document
        .dispatch(Action::Activate {
            path: close.aliases[0].clone(),
        })
        .unwrap();
    assert_eq!(
        document.drain_callbacks()[0].invocation.arguments,
        CallbackArguments::Boolean(false)
    );
    document
        .update_module_props_with_callbacks(props(false, false), host.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        document.renderer.document.get_element_by_id(&trigger.id)
    );
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
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
        "dirty slot"
    );
    document
        .update_module_props_with_callbacks(props(false, true), host.export_bindings().unwrap())
        .unwrap();
    assert!(
        document
            .session
            .widgets
            .values()
            .any(|widget| widget.controlled_open && widget.open && !widget.modal)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id(&editor.id),
        Some(editor_node)
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(calls.get(), 0);
}
