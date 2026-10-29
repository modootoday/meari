use super::*;
use meari::domain::{
    array_value::ArrayValue, host_callbacks::CallbackSignature, record_value::RecordValue,
};

const SOURCE: &str = r#"import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';
type Props={items:{key:string;number:string;front:string;back:string;label:string}[];selectedKey:string;orientation:string;label:string;emptyLabel:string;captureLabel:string;busy:boolean;empty:boolean;onSelect:(key:string)=>void};
export function Rail(props:Props){return <section aria-label={props.label}><RadioGroup value={props.selectedKey} orientation={props.orientation} onValueChange={props.onSelect} disabled={props.busy} aria-label={props.label}>{props.items.map(row=><RadioGroupItem key={row.key} value={row.key} disabled={props.busy} aria-label={row.label}><span>{row.number}</span><span>{row.front}</span><span>{row.back}</span></RadioGroupItem>)}</RadioGroup><p hidden={!props.empty}>{props.emptyLabel}</p><span>{props.captureLabel}</span><input defaultValue="keep" /></section>;}"#;

fn values(selected: &str, busy: bool) -> BTreeMap<String, Value> {
    let component = meari::parse(SOURCE, meari::application::Language::Tsx).unwrap();
    let PropType::Array(element) = &component.props[0].kind else {
        panic!()
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!()
    };
    let items = Value::Array(ArrayValue {
        element: element.as_ref().clone(),
        items: ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(index, key)| {
                Value::Record(
                    RecordValue::new(
                        schema.clone(),
                        BTreeMap::from([
                            ("key".into(), Value::String((*key).into())),
                            ("number".into(), Value::String((index + 1).to_string())),
                            ("front".into(), Value::String(format!("앞면 {key}"))),
                            ("back".into(), Value::String("原文🦀".into())),
                            ("label".into(), Value::String(format!("카드 {key}"))),
                        ]),
                    )
                    .unwrap(),
                )
            })
            .collect(),
    });
    BTreeMap::from([
        ("items".into(), items),
        ("selectedKey".into(), Value::String(selected.into())),
        ("orientation".into(), Value::String("horizontal".into())),
        ("label".into(), Value::String("카드 선택".into())),
        ("emptyLabel".into(), Value::String("카드 없음".into())),
        ("captureLabel".into(), Value::String("원문 캡처".into())),
        ("busy".into(), Value::Bool(busy)),
        ("empty".into(), Value::Bool(false)),
        (
            "onSelect".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onSelect".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ])
}

fn mount_rail(selected: &str) -> (InteractiveDocument, Host) {
    let values = values(selected, false);
    mount_program(program(SOURCE, values.clone()), values)
}

fn attr<'a>(document: &'a InteractiveDocument, path: &str, name: &str) -> Option<&'a str> {
    document
        .elements()
        .iter()
        .find(|element| element.path == path)?
        .attributes
        .get(name)
        .map(String::as_str)
}

#[test]
fn inherited_native_direction_drives_owned_keyboard_focus_and_keeps_controlled_checked() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    for (direction, expected, path) in [("rtl", "c", "0/0/2"), ("ltr", "b", "0/0/1")] {
        let source = SOURCE.replace("<section ", &format!("<section dir=\"{direction}\" "));
        let props = values("a", false);
        let (mut document, _host) = mount_program(program(&source, props.clone()), props);
        document.focus_path("0/0/0").unwrap();
        document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
            key: Key::Named(NamedKey::ArrowRight),
            code: Code::ArrowRight,
            modifiers: Modifiers::empty(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        }));
        assert!(document.last_error.is_none(), "{:?}", document.last_error);
        let pending = document.drain_callbacks();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].invocation.arguments,
            CallbackArguments::Text(expected.into())
        );
        assert_eq!(attr(&document, "0/0/0", "aria-checked"), Some("true"));
        let target = document
            .renderer
            .document
            .get_element_by_id(document.path_id(path).unwrap());
        assert_eq!(document.renderer.document.get_focussed_node_id(), target);
    }
}

#[test]
fn actual_record_rail_none_request_and_accepted_props_preserve_dirty_editor_and_a11y() {
    let (mut document, host) = mount_rail("");
    for path in ["0/0/0", "0/0/1", "0/0/2"] {
        assert_eq!(attr(&document, path, "aria-checked"), Some("false"));
    }
    assert_eq!(attr(&document, "0/0/0", "tabindex"), Some("0"));
    document.set_input_value("0/3", "dirty draft").unwrap();
    let editor = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/3").unwrap())
        .unwrap();
    let target = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0/1").unwrap())
        .unwrap();
    accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(target.as_u64()),
            data: None,
        },
    )
    .unwrap();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Text("b".into())
    );
    // A host may reject/retain this request: checked state stays with accepted props.
    assert_eq!(attr(&document, "0/0/1", "aria-checked"), Some("false"));
    assert!(host.received.borrow().is_empty());
    document
        .update_module_props_with_callbacks(
            values("b", false),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert_eq!(attr(&document, "0/0/1", "aria-checked"), Some("true"));
    assert_eq!(attr(&document, "0/0/0", "aria-checked"), Some("false"));
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0/3").unwrap()),
        Some(editor)
    );
    assert_eq!(document.input_values["0/3"], "dirty draft");
    let radio = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0/1").unwrap())
        .unwrap();
    let tree = accessibility::tree(&document);
    let node = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == radio.as_u64())
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::RadioButton);
    assert_eq!(node.toggled(), Some(accesskit::Toggled::True));
    assert_eq!(node.label(), Some("카드 b"));
}

#[test]
fn real_css_hidden_items_keyboard_disabled_and_readonly_checked_do_not_bypass_admission() {
    use blitz_dom::Document;
    use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
    use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};
    let (mut document, host) = mount_rail("a");
    let hidden = document.path_id("0/0/1").unwrap().to_owned();
    document
        .replace_stylesheet(&format!(
            "button{{width:150px;height:35px}}#{hidden}{{display:none}}"
        ))
        .unwrap();
    document.focus_path("0/0/0").unwrap();
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(NamedKey::ArrowRight),
        code: Code::ArrowRight,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    let requests = document.drain_callbacks();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].invocation.arguments,
        CallbackArguments::Text("c".into())
    );
    assert_eq!(attr(&document, "0/0/0", "aria-checked"), Some("true"));
    let focus = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/0/2").unwrap());
    assert_eq!(document.renderer.document.get_focussed_node_id(), focus);
    document.handle_ui_event(UiEvent::KeyDown(BlitzKeyEvent {
        key: Key::Named(NamedKey::Tab),
        code: Code::Tab,
        modifiers: Modifiers::empty(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    assert_eq!(attr(&document, "0/0/0", "tabindex"), Some("0"));
    assert_eq!(attr(&document, "0/0/2", "tabindex"), Some("-1"));
    document
        .update_module_props_with_callbacks(
            values("a", true),
            host.callbacks.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(
        !document
            .dispatch(Action::Activate {
                path: "0/0/2".into()
            })
            .unwrap()
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(attr(&document, "0/0/0", "aria-checked"), Some("true"));
}

#[test]
fn uncontrolled_and_readonly_groups_keep_none_distinct_from_first_tab_stop() {
    for controlled in [false, true] {
        let source = format!(
            "import {{RadioGroup,RadioGroupItem}} from '@/components/ui/radio-group';type Props={{}};export function Plain(props:Props){{return <RadioGroup {} aria-label=\"choices\"><RadioGroupItem value=\"a\" aria-label=\"A\"/><RadioGroupItem value=\"b\" aria-label=\"B\"/></RadioGroup>;}}",
            if controlled { "value=\"\"" } else { "" }
        );
        let component = meari::parse(&source, meari::application::Language::Tsx).unwrap();
        let mut document = InteractiveDocument::new(
            component,
            "button{width:30px;height:30px}",
            Viewport {
                width: 200,
                height: 150,
                dark: false,
            },
        )
        .unwrap();
        assert_eq!(attr(&document, "0/0", "tabindex"), Some("0"));
        assert_eq!(attr(&document, "0/0", "aria-checked"), Some("false"));
        assert_eq!(attr(&document, "0/1", "aria-checked"), Some("false"));
        document
            .dispatch(Action::Activate { path: "0/1".into() })
            .unwrap();
        assert_eq!(
            attr(&document, "0/1", "aria-checked"),
            Some(if controlled { "false" } else { "true" })
        );
        assert!(document.drain_callbacks().is_empty());
    }
}

#[test]
fn revoked_export_failure_restores_radio_focus_and_never_publishes_a_request() {
    let (mut document, mut host) = mount_rail("a");
    document.focus_path("0/0/0").unwrap();
    let before = document.renderer.document.get_focussed_node_id();
    let target = document.path_id("0/0/2").unwrap().to_owned();
    document
        .replace_stylesheet(&format!(
            "button{{width:150px;height:35px}}#{target}:focus{{visibility:hidden}}"
        ))
        .unwrap();
    assert!(
        document
            .dispatch(Action::Key {
                path: "0/0/0".into(),
                key: "End".into(),
                shift: false
            })
            .is_err()
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(document.renderer.document.get_focussed_node_id(), before);
    document
        .replace_stylesheet("button{width:150px;height:35px}")
        .unwrap();
    host.callbacks.invalidate().unwrap();
    assert!(
        document
            .dispatch(Action::Key {
                path: "0/0/0".into(),
                key: "End".into(),
                shift: false
            })
            .is_err()
    );
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(attr(&document, "0/0/0", "aria-checked"), Some("true"));
    assert!(host.received.borrow().is_empty());
    assert_eq!(document.renderer.document.get_focussed_node_id(), before);
}
