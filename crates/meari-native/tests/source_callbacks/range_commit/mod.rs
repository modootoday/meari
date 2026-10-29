use super::*;
#[path = "gesture_observation.rs"]
mod gesture_observation;
#[path = "nested_slots.rs"]
mod nested_slots;
#[path = "parts.rs"]
mod parts;
use blitz_dom::Document;
use blitz_traits::events::*;
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
const SOURCE: &str = "type Props={disabled:boolean;onDraft:(value:string)=>void;onCommit:(value:string)=>void};export function Range(props:Props){return <main><input id=\"range\" type=\"range\" min=\"0\" max=\"100\" step=\"10\" value=\"20\" disabled={props.disabled} onChange={event=>props.onDraft(event.currentTarget.value)} onValueCommit={props.onCommit}/></main>}";
fn fixture() -> (InteractiveDocument, Host, String) {
    let (mut document, host) = mount(SOURCE);
    document
        .replace_stylesheet("input{position:fixed;left:20px;top:20px;width:176px;height:24px}")
        .unwrap();
    let path = document
        .elements()
        .iter()
        .find(|node| node.id == "range")
        .unwrap()
        .path
        .clone();
    (document, host, path)
}
fn pointer(x: f32, buttons: MouseEventButtons) -> BlitzPointerEvent {
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: PointerCoords {
            page_x: x,
            page_y: 30.0,
            client_x: x,
            client_y: 30.0,
            screen_x: x,
            screen_y: 30.0,
        },
        button: MouseEventButton::Main,
        buttons,
        mods: Modifiers::empty(),
        details: Default::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    }
}
fn key(named: NamedKey, down: bool) -> UiEvent {
    let key = BlitzKeyEvent {
        key: Key::Named(named),
        code: Code::ArrowRight,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: if down {
            KeyState::Pressed
        } else {
            KeyState::Released
        },
        text: None,
    };
    if down {
        UiEvent::KeyDown(key)
    } else {
        UiEvent::KeyUp(key)
    }
}
fn deliver(
    document: &mut InteractiveDocument,
    host: &mut Host,
) -> Vec<(String, CallbackArguments)> {
    let pending = document.drain_callbacks();
    let observed = pending
        .iter()
        .map(|event| {
            (
                event.invocation.prop.clone(),
                event.invocation.arguments.clone(),
            )
        })
        .collect();
    for event in pending {
        host.callbacks
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    observed
}
#[test]
fn native_range_draft_and_actual_commit_are_distinct_typed_host_text_intents() {
    let (mut document, mut host, _) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    assert!(
        host.received.borrow().is_empty(),
        "no registry reentry from renderer"
    );
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
    assert_eq!(host.received.borrow().len(), 2);
}
#[test]
fn native_range_key_repeat_keyup_and_cancel_do_not_duplicate_commit() {
    let (mut document, mut host, path) = fixture();
    document.focus_path(&path).unwrap();
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    let draft = deliver(&mut document, &mut host);
    assert_eq!(
        draft,
        vec![
            ("onDraft".into(), CallbackArguments::Text("30".into())),
            ("onDraft".into(), CallbackArguments::Text("40".into()))
        ]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("40".into()))]
    );
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert!(deliver(&mut document, &mut host).is_empty());
    document.handle_ui_event(key(NamedKey::ArrowRight, true));
    deliver(&mut document, &mut host);
    document.handle_ui_event(key(NamedKey::Escape, true));
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert!(deliver(&mut document, &mut host).is_empty());
}
#[test]
fn native_range_queue_capacity_retains_original_commit_until_one_admission() {
    let (mut document, mut host, path) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    deliver(&mut document, &mut host);
    for _ in 0..1024 {
        document.enqueue_callback_text(&path, "50").unwrap();
    }
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(document.last_error.is_some());
    let admitted = document.drain_callbacks();
    assert_eq!(admitted.len(), 1024);
    assert!(
        admitted
            .iter()
            .all(|event| event.invocation.prop == "onDraft")
    );
    assert!(document.has_pending_callbacks());
    let terminal = document.drain_callbacks();
    assert_eq!(terminal.len(), 1);
    assert!(!document.has_pending_callbacks());
    let mut commits = terminal
        .into_iter()
        .filter(|event| event.invocation.prop == "onCommit");
    let event = commits.next().unwrap();
    assert!(commits.next().is_none());
    assert_eq!(
        event.invocation.arguments,
        CallbackArguments::Text("50".into())
    );
    host.callbacks
        .dispatch(
            &event.handle,
            event.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
}
#[test]
fn native_range_disabled_and_revoked_callback_owner_cancel_without_commit() {
    let (mut document, mut host, _) = fixture();
    let mut disabled = props(SOURCE);
    disabled.insert("disabled".into(), Value::Bool(true));
    document
        .update_module_props_with_callbacks(disabled, host.callbacks.export_bindings().unwrap())
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(deliver(&mut document, &mut host).is_empty());
    document
        .update_module_props_with_callbacks(
            props(SOURCE),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let old = document.drain_callbacks();
    document.close_callbacks();
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert!(document.drain_callbacks().is_empty());
    for event in old {
        assert!(
            host.callbacks
                .dispatch(
                    &event.handle,
                    event.invocation.arguments,
                    &mut host.registry
                )
                .is_err()
        );
    }
}

const CONTROLLED: &str = "type Props={value:string;disabled:boolean;onDraft:(value:string)=>void;onCommit:(value:string)=>void};export function Range(props:Props){return <main><input id=\"range\" type=\"range\" min=\"0\" max=\"100\" step=\"10\" value={props.value} disabled={props.disabled} onChange={event=>props.onDraft(event.currentTarget.value)} onValueCommit={props.onCommit}/></main>}";
fn controlled_value(value: &str) -> BTreeMap<String, Value> {
    let mut values = props(CONTROLLED);
    values.insert("value".into(), Value::String(value.into()));
    values
}
#[test]
fn native_range_controlled_feedback_retains_original_export_and_terminal_owner() {
    let (mut document, mut host) = mount(CONTROLLED);
    document
        .update_module_range_props(controlled_value("20"))
        .unwrap();
    document
        .replace_stylesheet("input{position:fixed;left:20px;top:20px;width:176px;height:24px}")
        .unwrap();
    let original = document
        .renderer
        .document
        .get_element_by_id("range")
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    document
        .update_module_range_props(controlled_value("50"))
        .unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id("range"),
        Some(original)
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
    deliver(&mut document, &mut host);
    document
        .update_module_range_props(controlled_value("60"))
        .unwrap();
    document.handle_ui_event(key(NamedKey::ArrowRight, false));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("60".into()))]
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(original)
    );
    assert!(!document.has_pending_callbacks());
}
#[test]
fn native_range_continuity_rejects_authority_and_disabled_changes_without_replacing_export() {
    let (mut document, mut host) = mount(CONTROLLED);
    document
        .update_module_range_props(controlled_value("20"))
        .unwrap();
    document
        .replace_stylesheet("input{position:fixed;left:20px;top:20px;width:176px;height:24px}")
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let mut changed = controlled_value("50");
    changed.insert("onDraft".into(), changed["onCommit"].clone());
    assert!(document.update_module_range_props(changed).is_err());
    let mut disabled = controlled_value("50");
    disabled.insert("disabled".into(), Value::Bool(true));
    assert!(document.update_module_range_props(disabled).is_err());
    assert_eq!(deliver(&mut document, &mut host).len(), 1);
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        188.0,
        MouseEventButtons::Primary,
    )));
    deliver(&mut document, &mut host);
    document
        .update_module_props_with_callbacks(
            controlled_value("100"),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    document.handle_ui_event(UiEvent::PointerUp(pointer(188.0, MouseEventButtons::None)));
    assert!(
        deliver(&mut document, &mut host).is_empty(),
        "replacement export must cancel the old gesture"
    );
}
#[test]
fn native_range_full_byte_queue_does_not_expand_a_drain_for_terminal_commit() {
    let (mut document, mut host, path) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    deliver(&mut document, &mut host);
    let text = "x".repeat(1024 * 1024);
    for _ in 0..4 {
        document.enqueue_callback_text(&path, &text).unwrap();
    }
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    let first = document.drain_callbacks();
    assert_eq!(first.len(), 4);
    assert!(
        first
            .iter()
            .all(|event| event.invocation.arguments == CallbackArguments::Text(text.clone()))
    );
    assert!(document.has_pending_callbacks());
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    assert!(!document.has_pending_callbacks());
}

#[test]
fn native_range_explicit_display_text_feedback_preserves_drag_and_updates_preview() {
    let source = CONTROLLED
        .replace("value:string;", "value:string;preview:string;")
        .replace(
            "</main>",
            "<span id=\"preview\">{props.preview}</span></main>",
        );
    let (mut document, mut host) = mount(&source);
    let values = |value: &str, preview: &str| {
        let mut values = props(&source);
        values.insert("value".into(), Value::String(value.into()));
        values.insert("preview".into(), Value::String(preview.into()));
        values
    };
    document
        .update_module_range_props(values("20", "value"))
        .unwrap();
    document
        .replace_stylesheet("input{position:fixed;left:20px;top:20px;width:176px;height:24px}")
        .unwrap();
    let original = document
        .renderer
        .document
        .get_element_by_id("range")
        .unwrap();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    deliver(&mut document, &mut host);
    assert!(
        document
            .update_module_range_props(values("50", "00:00:50.000"))
            .is_err()
    );
    document
        .update_module_range_props_with_text(
            values("50", "00:00:50.000"),
            &BTreeSet::from(["0/1/0".into()]),
        )
        .unwrap();
    assert_eq!(
        document.renderer.document.get_element_by_id("range"),
        Some(original)
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    let meari::domain::NodeKind::Element { children, .. } = &document.source.root.kind else {
        panic!("range root missing");
    };
    let meari::domain::NodeKind::Element { children, .. } = &children[1].kind else {
        panic!("preview missing");
    };
    assert_eq!(
        children[0].kind,
        meari::domain::NodeKind::Text(Value::String("00:00:50.000".into()))
    );
}

struct BoundedRangeHost {
    host: Host,
    capacity: usize,
    admitted: Vec<(
        meari::application::host_callbacks::CallbackHandle,
        CallbackArguments,
    )>,
    delivered: Vec<CallbackArguments>,
}
impl meari_native::infrastructure::native_host::NativeWindowHost for BoundedRangeHost {
    type Error = std::io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &meari_native::infrastructure::native_host::HostStamp,
        callback: &meari::application::host_callbacks::PendingCallback,
    ) -> meari_native::infrastructure::native_host::IntentAdmission<Self::Error> {
        use meari_native::infrastructure::native_host::IntentAdmission;
        if self.admitted.len() >= self.capacity {
            return IntentAdmission::Capacity;
        }
        self.admitted.push((
            callback.handle.clone(),
            callback.invocation.arguments.clone(),
        ));
        IntentAdmission::Accepted
    }
    fn after_dispatch(
        &mut self,
        _: meari_native::infrastructure::native_host::HostTurn,
    ) -> Result<meari_native::infrastructure::native_host::HostEffects, Self::Error> {
        for (handle, arguments) in self.admitted.drain(..) {
            self.host
                .callbacks
                .dispatch(&handle, arguments.clone(), &mut self.host.registry)
                .map_err(|error| std::io::Error::other(error.message))?;
            self.delivered.push(arguments);
        }
        Ok(Default::default())
    }
    fn close(
        &mut self,
        _: meari_native::infrastructure::native_host::CloseRequest<()>,
    ) -> meari_native::infrastructure::native_host::CloseDecision<(), Self::Error> {
        meari_native::infrastructure::native_host::CloseDecision::HostCleanupComplete {
            checkpoint: (),
        }
    }
}
#[test]
fn native_range_full_queue_keeps_terminal_behind_original_runtime_backpressure_tail() {
    use meari_native::infrastructure::native_host::{
        HostTurnReason, NativeDocumentHandle, NativeHostRuntime,
    };
    let (mut document, mut host, path) = fixture();
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    deliver(&mut document, &mut host);
    for _ in 0..1024 {
        document.enqueue_callback_text(&path, "draft-tail").unwrap();
    }
    document.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    let generation = document.callback_scope().unwrap().source_generation;
    let handle = NativeDocumentHandle::new(document, generation).unwrap();
    let consumer = BoundedRangeHost {
        host,
        capacity: 0,
        admitted: vec![],
        delivered: vec![],
    };
    let mut runtime = NativeHostRuntime::new(handle.clone(), consumer);
    for _ in 0..2 {
        let receipt = runtime.after_dispatch(HostTurnReason::Completion).unwrap();
        assert_eq!(receipt.remaining_callbacks, 1024);
        assert_eq!(receipt.admitted_this_pass, 0);
        assert!(runtime.host().delivered.is_empty());
        assert!(
            handle
                .try_with(|document| document.has_pending_callbacks())
                .unwrap()
        );
    }
    runtime.host_mut().capacity = 1024;
    let receipt = runtime.after_dispatch(HostTurnReason::Completion).unwrap();
    assert_eq!(receipt.remaining_callbacks, 0);
    assert_eq!(runtime.host().delivered.len(), 1024);
    assert!(
        runtime
            .host()
            .delivered
            .iter()
            .all(|argument| *argument == CallbackArguments::Text("draft-tail".into()))
    );
    assert!(
        handle
            .try_with(|document| document.has_pending_callbacks())
            .unwrap()
    );
    let receipt = runtime.after_dispatch(HostTurnReason::Completion).unwrap();
    assert_eq!(receipt.accepted_callbacks, 1);
    assert_eq!(
        runtime.host().delivered.last(),
        Some(&CallbackArguments::Text("50".into()))
    );
    assert!(
        !handle
            .try_with(|document| document.has_pending_callbacks())
            .unwrap()
    );
    assert_eq!(
        runtime
            .after_dispatch(HostTurnReason::Completion)
            .unwrap()
            .accepted_callbacks,
        0
    );
}
