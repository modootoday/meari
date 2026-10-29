use super::*;
use blitz_dom::Document;
use blitz_traits::events::*;
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};

const NAVIGATION: &str =
    include_str!("../../../meari/fixtures/node-slots/studio/navigation-item.tsx");

fn navigation() -> (InteractiveDocument, Host, BTreeMap<String, Value>, String) {
    let mut values = props(NAVIGATION);
    values.insert("workspaceId".into(), Value::String("첫 작업/🌊".into()));
    values.insert("label".into(), Value::String("작업 열기".into()));
    let (document, host) = mount_program(program(NAVIGATION, values.clone()), values.clone());
    let path = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    let projection = document.module_projection().unwrap();
    assert_eq!(
        projection.origins[&path].module,
        BuildPath::try_from("view.tsx".to_owned()).unwrap()
    );
    assert_eq!(
        projection.callback_bindings()[&path].span,
        meari::domain::Span {
            start: 349,
            end: 390
        }
    );
    (document, host, values, path)
}

#[test]
fn original_navigation_pointer_keyboard_and_accessibility_share_one_captured_intent() {
    let (mut document, mut host, _, path) = navigation();
    document
        .replace_stylesheet("button{position:fixed;left:16px;top:16px;width:150px;height:35px}")
        .unwrap();
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
    let mut pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    let button = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&path).unwrap())
        .unwrap();
    let tree = accessibility::tree(&document);
    let accessible = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == button.as_u64())
        .unwrap()
        .1;
    assert_eq!(accessible.role(), accesskit::Role::Button);
    assert_eq!(accessible.label(), Some("작업 열기"));
    accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(button.as_u64()),
            data: None,
        },
    )
    .unwrap();
    let accessibility_pending = document.drain_callbacks();
    assert_eq!(accessibility_pending.len(), 1);
    pending.extend(accessibility_pending);
    document.focus_path(&path).unwrap();
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let keyboard_pending = document.drain_callbacks();
    assert_eq!(keyboard_pending.len(), 1);
    pending.extend(keyboard_pending);
    assert!(host.received.borrow().is_empty());
    for request in pending {
        assert_eq!(
            request.invocation.span,
            meari::domain::Span {
                start: 349,
                end: 390
            }
        );
        assert_eq!(
            request.invocation.arguments,
            CallbackArguments::Text("첫 작업/🌊".into())
        );
        host.callbacks
            .dispatch(
                &request.handle,
                request.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    assert_eq!(host.received.borrow().len(), 3);
}

#[test]
fn accepted_capture_update_revokes_old_handle_and_failed_update_keeps_last_good() {
    let (mut document, mut host, mut values, path) = navigation();
    document
        .dispatch(Action::Activate { path: path.clone() })
        .unwrap();
    let first = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        first.invocation.arguments,
        CallbackArguments::Text("첫 작업/🌊".into())
    );
    values.insert("workspaceId".into(), Value::String("다음 작업".into()));
    document
        .update_module_props_with_callbacks(
            values.clone(),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert_eq!(
        host.callbacks
            .dispatch(
                &first.handle,
                first.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    document
        .dispatch(Action::Activate { path: path.clone() })
        .unwrap();
    let accepted = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        accepted.invocation.arguments,
        CallbackArguments::Text("다음 작업".into())
    );
    let mut foreign_scope = document.callback_scope().unwrap().clone();
    foreign_scope.source_generation += 1;
    let foreign = HostCallbacks::new(foreign_scope);
    assert!(
        document
            .update_module_props_with_callbacks(values.clone(), foreign.export_bindings().unwrap())
            .is_err()
    );
    values.insert("workspaceId".into(), Value::Bool(false));
    assert!(
        document
            .update_module_props_with_callbacks(values, host.callbacks.export_bindings().unwrap())
            .is_err()
    );
    host.callbacks
        .dispatch(
            &accepted.handle,
            accepted.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    document.dispatch(Action::Activate { path }).unwrap();
    let after_failure = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        after_failure.invocation.arguments,
        CallbackArguments::Text("다음 작업".into())
    );
    assert_eq!(host.received.borrow().len(), 1);
}
