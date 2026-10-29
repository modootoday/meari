//! Actual Native event/DOM candidates; synthetic events do not certify OS pointer capture.
use blitz_dom::Document;
use blitz_traits::events::*;
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
use meari::{application::Language, domain::Value};
use meari_native::{domain::Viewport, infrastructure::document::InteractiveDocument};

fn document(disabled: bool) -> InteractiveDocument {
    let source=meari::parse("type Props={disabled:boolean};export function Range(props:Props){return <main><label htmlFor=\"range\">Value</label><input id=\"range\" aria-label=\"Value\" type=\"range\" min=\"0\" max=\"100\" step=\"10\" value=\"20\" disabled={props.disabled}/></main>}",Language::Tsx).unwrap();
    let bound = meari_native::application::bind(
        &source,
        &std::collections::BTreeMap::from([("disabled".into(), Value::Bool(disabled))]),
    )
    .unwrap();
    InteractiveDocument::new(
        bound,
        "input{position:fixed;left:20px;top:20px;width:176px;height:24px}",
        Viewport {
            width: 400,
            height: 200,
            dark: false,
        },
    )
    .unwrap()
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
fn key() -> UiEvent {
    UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(NamedKey::ArrowRight),
        code: Code::ArrowRight,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    })
}
fn value(doc: &InteractiveDocument) -> String {
    let id = doc.renderer.document.get_element_by_id("range").unwrap();
    doc.renderer
        .document
        .get_node(id)
        .unwrap()
        .attr(blitz_dom::LocalName::from("value"))
        .unwrap()
        .to_owned()
}
#[test]
fn native_range_pointer_keyboard_focus_and_disabled_use_actual_dom_value() {
    let mut doc = document(false);
    let id = doc.renderer.document.get_element_by_id("range").unwrap();
    doc.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert_eq!(value(&doc), "50");
    assert_eq!(doc.renderer.document.get_focussed_node_id(), Some(id));
    doc.handle_ui_event(UiEvent::PointerUp(pointer(108.0, MouseEventButtons::None)));
    doc.handle_ui_event(key());
    assert_eq!(value(&doc), "60");
    assert!(doc.last_error.is_none(), "{:?}", doc.last_error);
    let mut disabled = document(true);
    disabled.handle_ui_event(UiEvent::PointerDown(pointer(
        188.0,
        MouseEventButtons::Primary,
    )));
    assert_eq!(value(&disabled), "20");
}
#[test]
fn leaving_document_cancels_drag_without_late_value_or_focus_changes() {
    let mut doc = document(false);
    doc.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    let original = value(&doc);
    doc.handle_ui_event(UiEvent::PointerLeave(pointer(
        108.0,
        MouseEventButtons::None,
    )));
    doc.handle_ui_event(UiEvent::PointerMove(pointer(
        188.0,
        MouseEventButtons::Primary,
    )));
    assert_eq!(value(&doc), original);
}

#[test]
fn range_actual_value_reaches_existing_typed_text_source_input() {
    let source=meari::parse("import {useState} from 'react';type Props={};export function Range(props:Props){const [value,setValue]=useState('20');return <input id=\"range\" type=\"range\" min=\"0\" max=\"100\" step=\"10\" value={value} onChange={event=>setValue(event.currentTarget.value)}/>}",Language::Tsx).unwrap();
    let mut doc = InteractiveDocument::new(
        source,
        "input{position:fixed;left:20px;top:20px;width:176px;height:24px}",
        Viewport {
            width: 400,
            height: 200,
            dark: false,
        },
    )
    .unwrap();
    doc.handle_ui_event(UiEvent::PointerDown(pointer(
        108.0,
        MouseEventButtons::Primary,
    )));
    assert!(doc.last_error.is_none(), "{:?}", doc.last_error);
    assert_eq!(
        doc.component_state().unwrap().values["value"],
        meari::domain::component_state::StateValue::String("50".into())
    );
    assert_eq!(value(&doc), "50");
    let id = doc.renderer.document.get_element_by_id("range").unwrap();
    assert_eq!(doc.renderer.document.get_focussed_node_id(), Some(id));
    doc.renderer.document.mutate().set_attribute(
        id,
        blitz_dom::QualName::new(None, "".into(), "type".into()),
        "checkbox",
    );
    let error = doc
        .dispatch_source_input(
            "0",
            &meari::domain::input_event::InputPayload::Text("80".into()),
        )
        .unwrap_err();
    assert!(
        error.contains("source range input DOM kind changed"),
        "{error}"
    );
    assert_eq!(
        doc.component_state().unwrap().values["value"],
        meari::domain::component_state::StateValue::String("50".into())
    );
    assert_eq!(doc.renderer.document.get_element_by_id("range"), Some(id));
}
