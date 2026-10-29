use meari::{
    application::Language,
    domain::{component_state::StateValue, input_event::InputPayload},
};
use meari_native::{domain::Viewport, infrastructure::document::InteractiveDocument};

fn mount() -> InteractiveDocument {
    let source = meari::parse(
        include_str!("../../meari/fixtures/input-state.tsx"),
        Language::Tsx,
    )
    .unwrap();
    InteractiveDocument::new(
        source,
        "input{width:150px;height:30px}",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )
    .unwrap()
}

fn text(document: &InteractiveDocument) -> String {
    document
        .renderer
        .document
        .get_node(
            document
                .renderer
                .document
                .get_element_by_id("editor")
                .unwrap(),
        )
        .unwrap()
        .element_data()
        .unwrap()
        .text_input_data()
        .unwrap()
        .editor
        .raw_text()
        .to_string()
}

fn checked(document: &InteractiveDocument) -> bool {
    document
        .renderer
        .document
        .get_node(
            document
                .renderer
                .document
                .get_element_by_id("check")
                .unwrap(),
        )
        .unwrap()
        .element_data()
        .unwrap()
        .checkbox_input_checked()
        .unwrap()
}

#[test]
fn typed_source_input_updates_real_editor_checkbox_state_and_accessibility_without_remount() {
    let mut document = mount();
    let editor = document
        .renderer
        .document
        .get_element_by_id("editor")
        .unwrap();
    document.focus_path("0/0").unwrap();
    for value in ["한글🦀", "", "updated"] {
        document.set_input_value("0/0", value).unwrap();
        assert_eq!(text(&document), value);
        assert_eq!(
            document.component_state().unwrap().values["text"],
            StateValue::String(value.into())
        );
        assert_eq!(
            document.renderer.document.get_element_by_id("editor"),
            Some(editor)
        );
        assert_eq!(
            document.renderer.document.get_focussed_node_id(),
            Some(editor)
        );
        let tree = meari_native::infrastructure::accessibility::tree(&document);
        assert_eq!(
            tree.nodes
                .iter()
                .find(|(id, _)| id.0 == editor.as_u64())
                .unwrap()
                .1
                .value(),
            Some(value)
        );
    }
    for value in [true, false, true] {
        document
            .dispatch_source_input("0/1", &InputPayload::Checked(value))
            .unwrap();
        assert_eq!(checked(&document), value);
        assert_eq!(
            document.component_state().unwrap().values["checked"],
            StateValue::Bool(value)
        );
    }
}

#[test]
fn bad_payload_and_missing_dom_leave_model_and_editor_unchanged() {
    let mut document = mount();
    document.set_input_value("0/0", "before").unwrap();
    let previous = document.component_state().unwrap();
    assert!(
        document
            .dispatch_source_input("0/0", &InputPayload::Checked(true))
            .is_err()
    );
    assert_eq!(document.component_state().unwrap(), previous);
    assert_eq!(text(&document), "before");
    let node = document
        .renderer
        .document
        .get_element_by_id("check")
        .unwrap();
    let name = blitz_dom::QualName::new(
        None,
        blitz_dom::Namespace::from(""),
        blitz_dom::LocalName::from("id"),
    );
    document
        .renderer
        .document
        .mutate()
        .set_attribute(node, name, "external-check");
    assert!(document.set_input_value("0/0", "must not commit").is_err());
    assert_eq!(document.component_state().unwrap(), previous);
    assert_eq!(text(&document), "before");
}

#[test]
fn engine_keyboard_and_accesskit_payloads_reach_the_same_source_state() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers};
    let mut document = mount();
    document.focus_path("0/0").unwrap();
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Character("a".into()),
        code: Code::KeyA,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: Some("a".into()),
    }));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    assert_eq!(text(&document), "a");
    assert_eq!(
        document.component_state().unwrap().values["text"],
        StateValue::String("a".into())
    );
    let editor = document
        .renderer
        .document
        .get_element_by_id("editor")
        .unwrap();
    meari_native::infrastructure::accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::SetValue,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(editor.as_u64()),
            data: Some(accesskit::ActionData::Value("접근성🦀".into())),
        },
    )
    .unwrap();
    assert_eq!(text(&document), "접근성🦀");
    assert_eq!(
        document.component_state().unwrap().values["text"],
        StateValue::String("접근성🦀".into())
    );
}
