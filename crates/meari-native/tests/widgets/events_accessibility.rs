use blitz_dom::Document;
use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
use meari::{application::Language, domain::Value};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn document() -> InteractiveDocument {
    let source = "type Props = {}; export function Demo(props: Props) {return <div><Tabs defaultValue=\"one\"><TabsList><TabsTrigger id=\"one\" value=\"one\">One</TabsTrigger><TabsTrigger id=\"disabled\" value=\"two\" disabled={true}>Disabled</TabsTrigger><TabsTrigger id=\"three\" value=\"three\">Three</TabsTrigger></TabsList><TabsContent value=\"one\">First</TabsContent><TabsContent value=\"three\">Third</TabsContent></Tabs><Label htmlFor=\"name\">Your name</Label><Input id=\"name\" defaultValue=\"Initial\"/><Input id=\"readonly\" readOnly={true}/></div>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    InteractiveDocument::new(
        component,
        "[hidden]{display:none!important}",
        Viewport {
            width: 600,
            height: 400,
            dark: false,
        },
    )
    .unwrap()
}

#[test]
fn ordinary_button_invocations_emit_shared_activation_requests() {
    let component=meari::parse("type Props={};export function Demo(props:Props){return <Button id=\"save\">Save</Button>;}",Language::Tsx).unwrap();
    let mut doc = InteractiveDocument::new(
        component,
        "",
        Viewport {
            width: 400,
            height: 300,
            dark: false,
        },
    )
    .unwrap();
    let id = doc.renderer.document.get_element_by_id("save").unwrap();
    let request = accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(id.as_u64()),
        data: None,
    };
    accessibility::act(&mut doc, &request).unwrap();
    assert_eq!(doc.activation_requests, vec!["0"]);
}

#[test]
fn native_key_event_reaches_roving_state_and_dom_focus() {
    let mut doc = document();
    let one = doc.renderer.document.get_element_by_id("one").unwrap();
    doc.renderer.document.set_focus_to(one);
    doc.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(NamedKey::ArrowRight),
        code: Code::ArrowRight,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    assert!(doc.last_error.is_none(), "{:?}", doc.last_error);
    assert_eq!(doc.session.widgets["0/0"].value, "three");
    assert_eq!(
        doc.renderer.document.get_focussed_node_id(),
        doc.renderer.document.get_element_by_id("three")
    );
}

#[test]
fn accessibility_actions_share_widget_state_and_editable_input_adapter() {
    let mut doc = document();
    let three = doc.renderer.document.get_element_by_id("three").unwrap();
    let request = |action, target, data| accesskit::ActionRequest {
        action,
        target_tree: accesskit::TreeId::ROOT,
        target_node: accesskit::NodeId(target),
        data,
    };
    accessibility::act(
        &mut doc,
        &request(accesskit::Action::Focus, three.as_u64(), None),
    )
    .unwrap();
    // Repeated focus succeeds even when the engine reports no focus *change*.
    accessibility::act(
        &mut doc,
        &request(accesskit::Action::Focus, three.as_u64(), None),
    )
    .unwrap();
    accessibility::act(
        &mut doc,
        &request(accesskit::Action::Click, three.as_u64(), None),
    )
    .unwrap();
    assert_eq!(doc.session.widgets["0/0"].value, "three");
    let name = doc.renderer.document.get_element_by_id("name").unwrap();
    accessibility::act(
        &mut doc,
        &request(
            accesskit::Action::SetValue,
            name.as_u64(),
            Some(accesskit::ActionData::Value("한글🦀".into())),
        ),
    )
    .unwrap();
    let tree = accessibility::tree(&doc);
    let input = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == name.as_u64())
        .unwrap()
        .1;
    assert_eq!(input.label(), Some("Your name"));
    assert_eq!(input.value(), Some("한글🦀"));
    let disabled = doc.renderer.document.get_element_by_id("disabled").unwrap();
    assert!(
        accessibility::act(
            &mut doc,
            &request(accesskit::Action::Click, disabled.as_u64(), None)
        )
        .is_err()
    );
    let readonly = doc.renderer.document.get_element_by_id("readonly").unwrap();
    assert!(
        accessibility::act(
            &mut doc,
            &request(
                accesskit::Action::SetValue,
                readonly.as_u64(),
                Some(accesskit::ActionData::Value("wrong".into()))
            )
        )
        .is_err()
    );
}

#[test]
fn bound_rust_values_and_input_dom_survive_unrelated_widget_updates() {
    let mut doc = document();
    let name = doc.renderer.document.get_element_by_id("name").unwrap();
    let path = doc
        .elements()
        .iter()
        .find(|e| e.id == "name")
        .unwrap()
        .path
        .clone();
    doc.set_input_value(&path, "persistent").unwrap();
    doc.dispatch(Action::Activate {
        path: "0/0/0/2".into(),
    })
    .unwrap();
    assert_eq!(doc.renderer.document.get_element_by_id("name"), Some(name));
    assert_eq!(doc.input_values[&path], "persistent");
    let source = meari::parse("type Props = {label:string;};export function Demo(props:Props){return <Label>{props.label}</Label>;}",Language::Tsx).unwrap();
    let bound = meari_native::application::bind(
        &source,
        &std::collections::BTreeMap::from([("label".into(), Value::String("Bound".into()))]),
    )
    .unwrap();
    assert!(bound.props.is_empty());
}
