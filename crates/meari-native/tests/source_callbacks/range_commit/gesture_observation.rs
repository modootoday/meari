use super::*;
fn fixture() -> (InteractiveDocument, Host, String) {
    let (mut document, host) = mount(CONTROLLED);
    document
        .update_module_range_props(controlled_value("20"))
        .unwrap();
    document
        .replace_stylesheet("input{position:fixed;left:20px;top:20px;width:176px;height:24px}")
        .unwrap();
    let path = document
        .elements()
        .iter()
        .find(|element| element.id == "range")
        .unwrap()
        .path
        .clone();
    (document, host, path)
}
#[test]
fn range_gesture_readonly_observation_preserves_original_queue_and_reports_release_cancel_escape_focus()
 {
    let (mut document, mut host, path) = fixture();
    assert!(!document.has_active_range_gesture());
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let values = document.input_values.clone();
    let focused = document.renderer.document.get_focussed_node_id();
    let handlers = document.module_projection().unwrap().callback_bindings();
    for _ in 0..3 {
        assert!(document.has_active_range_gesture());
    }
    assert_eq!(document.input_values, values);
    assert_eq!(document.renderer.document.get_focussed_node_id(), focused);
    assert_eq!(
        document.module_projection().unwrap().callback_bindings(),
        handlers
    );
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("50".into()))]
    );
    document
        .update_module_range_props(controlled_value("50"))
        .unwrap();
    assert!(document.has_active_range_gesture());
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(!document.has_active_range_gesture());
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    assert!(document.has_active_range_gesture());
    document.handle_ui_event(key(NamedKey::Escape, true));
    assert!(!document.has_active_range_gesture());
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("60".into()))]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    assert!(document.has_active_range_gesture());
    document.renderer.document.clear_focus();
    assert!(!document.has_active_range_gesture());
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert!(!document.has_active_range_gesture());
    document.focus_path(&path).unwrap();
    assert!(!document.has_active_range_gesture());
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("70".into()))]
    );
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.has_active_range_gesture());
    deliver(&mut document, &mut host);
    document.handle_ui_event(UiEvent::PointerCancel(pointer(
        108.0,
        MouseEventButtons::None,
    )));
    assert!(!document.has_active_range_gesture());
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
}
#[test]
fn range_gesture_observation_never_revives_replaced_revoked_or_retired_original_owners() {
    let (mut document, mut host, _) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.has_active_range_gesture());
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    document
        .update_module_props_with_callbacks(
            controlled_value("50"),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(!document.has_active_range_gesture());
    for event in old {
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
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        28.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.has_active_range_gesture());
    host.callbacks.invalidate().unwrap();
    assert!(!document.has_active_range_gesture());
    document.close_callbacks();
    assert!(!document.has_active_range_gesture());
    let (mut document, host, _) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.has_active_range_gesture());
    use meari_native::infrastructure::native_host::{
        CloseCause, HostAccessError, HostCloseStatus, NativeDocumentHandle, NativeHostRuntime,
    };
    let generation = document.callback_scope().unwrap().source_generation;
    let handle = NativeDocumentHandle::new(document, generation).unwrap();
    let mut runtime = NativeHostRuntime::new(
        handle.clone(),
        BoundedRangeHost {
            host,
            capacity: 4,
            admitted: vec![],
            delivered: vec![],
        },
    );
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    match handle.try_with(|document| document.has_active_range_gesture()) {
        Ok(false) | Err(HostAccessError::Closed) => {}
        other => panic!("retired range observation: {other:?}"),
    }
}
