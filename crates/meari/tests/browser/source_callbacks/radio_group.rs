use super::*;
use meari::domain::{
    array_value::ArrayValue, host_callbacks::CallbackSignature, record_value::RecordValue,
};
use web_sys::{HtmlInputElement, KeyboardEvent, KeyboardEventInit};

const SOURCE: &str = r#"import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';type Props={items:{key:string;number:string;front:string;back:string;label:string}[];selectedKey:string;orientation:string;busy:boolean;onSelect:(key:string)=>void};export function Rail(props:Props){return <section><RadioGroup value={props.selectedKey} orientation={props.orientation} disabled={props.busy} onValueChange={props.onSelect} aria-label="Cards">{props.items.map(row=><RadioGroupItem key={row.key} value={row.key} aria-label={row.label}><span>{row.number}</span><span>{row.front}</span><span>{row.back}</span></RadioGroupItem>)}</RadioGroup><input defaultValue="keep"/></section>;}"#;

fn values(selected: &str, busy: bool) -> BTreeMap<String, Value> {
    let component = meari::parse(SOURCE, meari::application::Language::Tsx).unwrap();
    let PropType::Array(element) = &component.props[0].kind else {
        panic!()
    };
    let PropType::Record(schema) = element.as_ref() else {
        panic!()
    };
    BTreeMap::from([
        (
            "items".into(),
            Value::Array(ArrayValue {
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
                                    ("number".into(), Value::String(index.to_string())),
                                    ("front".into(), Value::String(format!("앞면 {key}"))),
                                    ("back".into(), Value::String("原文🦀".into())),
                                    ("label".into(), Value::String(format!("카드 {key}"))),
                                ]),
                            )
                            .unwrap(),
                        )
                    })
                    .collect(),
            }),
        ),
        ("selectedKey".into(), Value::String(selected.into())),
        ("orientation".into(), Value::String("horizontal".into())),
        ("busy".into(), Value::Bool(busy)),
        (
            "onSelect".into(),
            Value::CallbackReference(CallbackReference {
                binding: "onSelect".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ])
}

fn mounted(selected: &str) -> Mounted {
    let entry: BuildPath = "rail.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), SOURCE.into())]))
            .unwrap();
    let program =
        ModuleProgram::mount(graph, values(selected, false), "web-record-radio".into(), 1).unwrap();
    mount_program(program, "web-record-radio")
}
fn item(container: &Element, value: &str) -> Element {
    container
        .query_selector(&format!("[role=radio][data-meari-radio-value='{value}']"))
        .unwrap()
        .unwrap()
}
fn key(element: &Element, name: &str) {
    let init = KeyboardEventInit::new();
    init.set_key(name);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let event = KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    element.dispatch_event(&event).unwrap();
}

#[wasm_bindgen_test]
fn inherited_web_direction_drives_owned_keyboard_focus_and_keeps_controlled_checked() {
    for (direction, expected) in [("rtl", "c"), ("ltr", "b")] {
        let source = SOURCE.replace("<section>", &format!("<section dir=\"{direction}\">"));
        let entry: BuildPath = "rail.tsx".to_owned().try_into().unwrap();
        let graph =
            meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source)])).unwrap();
        let program =
            ModuleProgram::mount(graph, values("a", false), "web-radio-direction".into(), 1)
                .unwrap();
        let (mut document, _host, container, _) = mount_program(program, "web-radio-direction");
        let first = item(&container, "a");
        first.dyn_ref::<HtmlElement>().unwrap().focus().unwrap();
        key(&first, "ArrowRight");
        let pending = document.drain_callbacks();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].invocation.arguments,
            CallbackArguments::Text(expected.into())
        );
        assert_eq!(first.get_attribute("aria-checked").as_deref(), Some("true"));
        assert!(
            container
                .owner_document()
                .unwrap()
                .active_element()
                .unwrap()
                .is_same_node(Some(item(&container, expected).as_ref()))
        );
        document.close();
        container.remove();
    }
}

#[wasm_bindgen_test]
fn record_radio_click_requests_then_accepted_props_update_checked_without_losing_dirty_input() {
    let (mut document, host, container, _) = mounted("");
    let button = item(&container, "b");
    let input = container
        .query_selector("input")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.set_value("dirty draft");
    button.dyn_ref::<HtmlElement>().unwrap().click();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Text("b".into())
    );
    assert_eq!(
        button.get_attribute("aria-checked").as_deref(),
        Some("false")
    );
    assert!(host.requests.borrow().is_empty());
    document
        .update_module_props_with_callbacks(
            values("b", false),
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    assert!(button.is_same_node(Some(item(&container, "b").as_ref())));
    assert_eq!(
        button.get_attribute("aria-checked").as_deref(),
        Some("true")
    );
    assert_eq!(
        item(&container, "a")
            .get_attribute("aria-checked")
            .as_deref(),
        Some("false")
    );
    assert_eq!(input.value(), "dirty draft");
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn radio_keyboard_skips_computed_hidden_items_and_disabled_props_block_requests() {
    let (mut document, host, container, _) = mounted("a");
    let doc = container.owner_document().unwrap();
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some("[role=radio][data-meari-radio-value=b]{display:none}"));
    container.append_child(&style).unwrap();
    let first = item(&container, "a");
    first.dyn_ref::<HtmlElement>().unwrap().focus().unwrap();
    key(&first, "ArrowRight");
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Text("c".into())
    );
    assert_eq!(first.get_attribute("aria-checked").as_deref(), Some("true"));
    assert!(
        doc.active_element()
            .unwrap()
            .is_same_node(Some(item(&container, "c").as_ref()))
    );
    container
        .query_selector("input")
        .unwrap()
        .unwrap()
        .dyn_ref::<HtmlElement>()
        .unwrap()
        .focus()
        .unwrap();
    assert_eq!(first.get_attribute("tabindex").as_deref(), Some("0"));
    assert_eq!(
        item(&container, "c").get_attribute("tabindex").as_deref(),
        Some("-1")
    );
    document
        .update_module_props_with_callbacks(
            values("a", true),
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    key(&item(&container, "c"), "Home");
    assert!(document.drain_callbacks().is_empty());
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn revoked_radio_callback_restores_previous_focus_and_checked_frame() {
    let (mut document, mut host, container, _) = mounted("a");
    let first = item(&container, "a");
    first.dyn_ref::<HtmlElement>().unwrap().focus().unwrap();
    let style = container
        .owner_document()
        .unwrap()
        .create_element("style")
        .unwrap();
    style.set_text_content(Some(
        "[role=radio][data-meari-radio-value=c]:focus{visibility:hidden}",
    ));
    container.append_child(&style).unwrap();
    key(&first, "End");
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(first.get_attribute("tabindex").as_deref(), Some("0"));
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(first.as_ref()))
    );
    style.remove();
    host.owner.invalidate().unwrap();
    key(&first, "End");
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(first.get_attribute("aria-checked").as_deref(), Some("true"));
    assert_eq!(first.get_attribute("tabindex").as_deref(), Some("0"));
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(first.as_ref()))
    );
    assert!(document.last_error().is_some());
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn uncontrolled_web_radio_retains_selection_and_node_across_default_prop_and_order_changes() {
    use meari::runtime::IntoValue;
    let source = "import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';type Props={selected:string;items:string[]};export function Choices(props:Props){return <RadioGroup defaultValue={props.selected} aria-label=\"Choices\">{props.items.map(item=><RadioGroupItem key={item} value={item} aria-label={item}/>)}</RadioGroup>;}";
    let entry: BuildPath = "choices.tsx".to_owned().try_into().unwrap();
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source.into())]))
            .unwrap();
    let values = |selected: &str, items: Vec<&str>| {
        BTreeMap::from([
            ("selected".into(), Value::String(selected.into())),
            ("items".into(), items.into_value()),
        ])
    };
    let container = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("section")
        .unwrap();
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .append_child(&container)
        .unwrap();
    let mut document = WebDocument::mount_modules(
        &container,
        graph,
        values("", vec!["a", "b"]),
        "button{width:50px;height:35px}",
        "uncontrolled-radio".into(),
        1,
    )
    .unwrap();
    let button = item(&container, "b");
    assert_eq!(
        button.get_attribute("aria-checked").as_deref(),
        Some("false")
    );
    button.dyn_ref::<HtmlElement>().unwrap().click();
    assert_eq!(
        button.get_attribute("aria-checked").as_deref(),
        Some("true")
    );
    document
        .update_module_props(values("a", vec!["b", "new", "a"]))
        .unwrap();
    assert!(button.is_same_node(Some(item(&container, "b").as_ref())));
    assert_eq!(
        button.get_attribute("aria-checked").as_deref(),
        Some("true")
    );
    assert_eq!(button.get_attribute("tabindex").as_deref(), Some("0"));
    assert_eq!(
        item(&container, "a")
            .get_attribute("aria-checked")
            .as_deref(),
        Some("false")
    );
    document.close();
    container.remove();
}
