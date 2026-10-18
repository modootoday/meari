use meari::{application::Language, parse};
use meari_native::{
    application::file_picker::{FilePicker, FileRequest, SelectedFile},
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn doc(body: &str) -> InteractiveDocument {
    InteractiveDocument::new(
        parse(
            &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
            Language::Tsx,
        )
        .unwrap(),
        "button{min-width:30px;min-height:20px}",
        Viewport {
            width: 400,
            height: 300,
            dark: false,
        },
    )
    .unwrap()
}
struct Picker {
    result: Option<Vec<SelectedFile>>,
    requests: Vec<FileRequest>,
}
impl FilePicker for Picker {
    fn select(&mut self, request: &FileRequest) -> Result<Option<Vec<SelectedFile>>, String> {
        self.requests.push(request.clone());
        Ok(self.result.take())
    }
}
fn file(name: &str) -> SelectedFile {
    SelectedFile {
        name: name.into(),
        path: name.into(),
    }
}
fn node(doc: &InteractiveDocument, id: &str) -> accesskit::Node {
    let target = doc
        .renderer
        .document
        .get_element_by_id(id)
        .unwrap()
        .as_u64();
    accessibility::tree(doc)
        .nodes
        .into_iter()
        .find(|(id, _)| id.0 == target)
        .unwrap()
        .1
}

#[test]
fn file_selection_cancel_reselect_clear_and_accessible_value_share_one_contract() {
    let mut doc = doc(
        r#"<Field><FieldLabel htmlFor="file">Picture</FieldLabel><Input id="file" type="file" multiple accept=".png,.jpg" aria-describedby="help"/><FieldDescription id="help">Choose pictures</FieldDescription></Field>"#,
    );
    let mut picker = Picker {
        result: Some(vec![file("one.png"), file("two.jpg")]),
        requests: vec![],
    };
    doc.dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    doc.process_file_requests(&mut picker).unwrap();
    assert_eq!(
        picker.requests,
        vec![FileRequest {
            multiple: true,
            accept: ".png,.jpg".into()
        }]
    );
    assert_eq!(doc.file_values["0/1"].len(), 2);
    let a11y = node(&doc, "file");
    assert_eq!(a11y.role(), accesskit::Role::Button);
    assert_eq!(a11y.label(), Some("Picture"));
    assert_eq!(a11y.value(), Some("one.png, two.jpg"));
    assert_eq!(a11y.described_by().len(), 1);
    assert!(a11y.supports_action(accesskit::Action::Click));
    assert!(!a11y.supports_action(accesskit::Action::SetValue));
    doc.choose_files("0/1", &mut picker).unwrap();
    assert_eq!(doc.file_values["0/1"].len(), 2);
    picker.result = Some(vec![file("replacement.png")]);
    doc.choose_files("0/1", &mut picker).unwrap();
    assert_eq!(doc.file_values["0/1"], vec![file("replacement.png")]);
    picker.result = Some(vec![]);
    doc.choose_files("0/1", &mut picker).unwrap();
    assert!(doc.file_values["0/1"].is_empty());
    assert_eq!(doc.session.focus.as_deref(), Some("0/1"));
    assert!(doc.set_input_value("0/1", "arbitrary path").is_err());
}

#[test]
fn disabled_and_single_file_inputs_reject_invalid_changes_without_overwriting_selection() {
    let mut doc = doc(
        r#"<div><Input id="single" type="file"/><Input id="blocked" type="file" disabled/></div>"#,
    );
    let mut picker = Picker {
        result: Some(vec![file("a"), file("b")]),
        requests: vec![],
    };
    assert!(doc.choose_files("0/0", &mut picker).is_err());
    assert!(doc.file_values.is_empty());
    assert!(doc.choose_files("0/1", &mut picker).is_err());
    assert_eq!(picker.requests.len(), 1);
    assert!(!node(&doc, "blocked").supports_action(accesskit::Action::Click));
}

#[test]
fn checkbox_and_switch_preserve_state_across_patches_and_expose_toggle_semantics() {
    let mut doc = doc(
        r#"<div><Checkbox id="c"/><Label htmlFor="c">Terms</Label><Switch id="s" defaultChecked aria-label="Updates"/><Checkbox id="disabled" disabled/></div>"#,
    );
    assert_eq!(node(&doc, "c").label(), Some("Terms"));
    assert_eq!(node(&doc, "c").toggled(), Some(accesskit::Toggled::False));
    doc.dispatch(Action::Activate { path: "0/0".into() })
        .unwrap();
    doc.dispatch(Action::Key {
        path: "0/2".into(),
        key: " ".into(),
        shift: false,
    })
    .unwrap();
    assert_eq!(node(&doc, "c").toggled(), Some(accesskit::Toggled::True));
    assert_eq!(node(&doc, "s").role(), accesskit::Role::Switch);
    assert_eq!(node(&doc, "s").toggled(), Some(accesskit::Toggled::False));
    assert!(
        !doc.dispatch(Action::Activate { path: "0/3".into() })
            .unwrap()
    );
    assert!(!node(&doc, "disabled").supports_action(accesskit::Action::Click));
}

#[test]
fn use_id_binds_label_and_input_in_the_native_document() {
    let source=parse(r#"import {useId} from "react";type Props={};export function Demo(props:Props){const id=useId();return <Field><FieldLabel htmlFor={id}>Generated</FieldLabel><Input id={id}/></Field>;}"#,Language::Tsx).unwrap();
    let doc = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 400,
            height: 300,
            dark: false,
        },
    )
    .unwrap();
    assert_eq!(node(&doc, "bridge-hook-id").label(), Some("Generated"));
}

#[test]
fn native_key_events_and_accesskit_click_reach_the_same_toggle_and_file_requests() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    let mut doc = doc(
        r#"<div><Checkbox id="toggle" aria-label="Terms"/><Input id="file" type="file" aria-label="Picture"/></div>"#,
    );
    doc.focus_path("0/0").unwrap();
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
    doc.handle_ui_event(key(Key::Character(" ".into()), Code::Space));
    assert!(doc.last_error.is_none());
    assert_eq!(
        node(&doc, "toggle").toggled(),
        Some(accesskit::Toggled::True)
    );
    let target = doc
        .renderer
        .document
        .get_element_by_id("toggle")
        .unwrap()
        .as_u64();
    accessibility::act(
        &mut doc,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(target),
            data: None,
        },
    )
    .unwrap();
    assert_eq!(
        node(&doc, "toggle").toggled(),
        Some(accesskit::Toggled::False)
    );
    doc.focus_path("0/1").unwrap();
    doc.handle_ui_event(key(Key::Named(NamedKey::Enter), Code::Enter));
    assert!(doc.last_error.is_none());
    assert_eq!(doc.file_requests, vec!["0/1"]);
    assert!(doc.activation_requests.is_empty());
}
