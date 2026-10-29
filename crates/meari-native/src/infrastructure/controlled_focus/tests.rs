use super::*;
use crate::domain::{Viewport, widgets::Action};
use meari::domain::Component;

fn component(open: bool, modal: bool, hidden: bool) -> Component {
    meari::parse(&format!("type Props={{}};export function Panel(props:Props){{return <Dialog open={{{open}}} modal={{{modal}}}><DialogTrigger>open</DialogTrigger><DialogContent forceMount hidden={{{hidden}}} aria-label=\"inspector\"><Input defaultValue=\"draft\"/><DialogClose>close</DialogClose></DialogContent></Dialog>;}}"), meari::application::Language::Tsx).unwrap()
}

fn document(component: Component) -> InteractiveDocument {
    InteractiveDocument::new(
        component,
        "button,input{width:120px;height:32px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
    )
    .unwrap()
}

fn focused(document: &InteractiveDocument, path: &str) -> bool {
    let expected = document
        .path_id(path)
        .and_then(|id| document.renderer.document.get_element_by_id(id))
        .expect("expected focus target must exist");
    document.renderer.document.get_focussed_node_id() == Some(expected)
}

#[test]
fn accepted_modal_props_focus_content_then_restore_trigger_without_replacing_input() {
    let mut document = document(component(false, true, true));
    document.focus_path("0/0").unwrap();
    let input = document
        .path_id("0/1/0")
        .and_then(|id| document.renderer.document.get_element_by_id(id))
        .unwrap();
    assert!(
        document
            .dispatch(Action::Activate { path: "0/0".into() })
            .is_err()
    );
    assert!(!document.session.widgets["0"].open);
    assert!(focused(&document, "0/0"));
    document
        .commit_component_projection(component(true, true, false))
        .unwrap();
    assert!(focused(&document, "0/1/0"));
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0/1/0").unwrap()),
        Some(input)
    );
    document
        .commit_component_projection(component(false, true, true))
        .unwrap();
    assert!(focused(&document, "0/0"));
    assert!(!document.session.widgets["0"].open);
}

#[test]
fn hidden_modal_accept_failure_restores_previous_props_and_actual_focus() {
    let mut document = document(component(false, true, true));
    document.focus_path("0/0").unwrap();
    assert!(
        document
            .commit_component_projection(component(true, true, true))
            .is_err()
    );
    assert!(!document.session.widgets["0"].open);
    assert!(focused(&document, "0/0"));
    assert!(!document.source_input_poisoned);
}

#[test]
fn nonmodal_accept_keeps_existing_focus() {
    let mut document = document(component(false, false, true));
    document.focus_path("0/0").unwrap();
    document
        .commit_component_projection(component(true, false, false))
        .unwrap();
    assert!(focused(&document, "0/0"));
    assert!(
        !document
            .elements()
            .iter()
            .find(|element| element.path == "0/1")
            .unwrap()
            .attributes
            .contains_key("aria-modal")
    );
}

#[test]
fn actual_toggle_keydown_preserves_enter_vs_space_without_local_commit() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    let source = meari::parse(
        "type Props={};export function Panel(props:Props){return <Switch checked={false} aria-label=\"test\"/>;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let mut document = document(source);
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
    assert!(
        document
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("scoped Boolean intent dispatcher"))
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(
        document.elements()[0]
            .attributes
            .get("aria-checked")
            .map(String::as_str),
        Some("false")
    );
}

#[test]
fn composed_alias_has_the_same_disabled_admission_as_its_canonical_control() {
    let source = meari::parse("type Props={};export function Panel(props:Props){return <Dialog><DialogTrigger asChild><Button disabled>open</Button></DialogTrigger><DialogContent aria-label=\"test\"/></Dialog>;}",meari::application::Language::Tsx).unwrap();
    let document = document(source);
    assert!(
        document
            .admit_callback_target("0/0", Some("button"), false)
            .is_err()
    );
    assert!(
        document
            .admit_callback_target("0/0/0", Some("button"), false)
            .is_err()
    );
    assert_eq!(document.path_id("0/0"), document.path_id("0/0/0"));
}

#[test]
fn composed_mixed_text_updates_keep_button_and_child_span_identity() {
    let definition = meari::parse("type Props={before:string;after:string};export function Panel(props:Props){return <Dialog><DialogTrigger asChild><Button>{props.before}<span>icon</span>{props.after}</Button></DialogTrigger><DialogContent aria-label=\"test\"/></Dialog>;}",meari::application::Language::Tsx).unwrap();
    let bound = |before: &str, after: &str| {
        meari::application::binding::bind(
            &definition,
            &std::collections::BTreeMap::from([
                ("before".into(), meari::domain::Value::String(before.into())),
                ("after".into(), meari::domain::Value::String(after.into())),
            ]),
        )
        .unwrap()
    };
    let mut document = document(bound("before", "after"));
    let button = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0").unwrap())
        .unwrap();
    let span = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0/0/1").unwrap())
        .unwrap();
    document
        .commit_component_projection(bound("changed-before", "changed-after"))
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0/0/0").unwrap()),
        Some(button)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0/0/0/1").unwrap()),
        Some(span)
    );
    assert_eq!(
        document
            .renderer
            .document
            .get_node(button)
            .unwrap()
            .text_content(),
        "changed-beforeiconchanged-after"
    );
}
