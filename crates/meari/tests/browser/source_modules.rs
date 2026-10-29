use meari::{
    domain::{Value, build::BuildPath, component_state::StateValue},
    infrastructure::web_document::WebDocument,
};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, HtmlInputElement};

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn host() -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("section").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

#[wasm_bindgen_test]
fn browser_module_click_props_and_failed_patch_keep_state_input_focus_and_listener_lifetime() {
    let sources = BTreeMap::from([
        (path("parent.tsx"), "import { Counter } from './counter'; type Props={caption:string}; export function Parent(props:Props){return <main><Counter caption={props.caption}/><Counter caption={props.caption}/></main>; }".into()),
        (path("counter.tsx"), "import { useState,useId } from 'react'; type Props={caption:string}; export function Counter(props:Props){const field=useId(); const [count,setCount]=useState(0); return <section><label htmlFor={field}>{props.caption}</label><input id={field}/><button onClick={() => setCount(previous => previous+1)}>{count}</button></section>; }".into()),
    ]);
    let graph = meari::parse_module_graph(&path("parent.tsx"), &sources).unwrap();
    let host = host();
    let mut document = WebDocument::mount_modules(
        &host,
        graph,
        BTreeMap::from([("caption".into(), Value::String("Before".into()))]),
        "",
        "browser-modules".into(),
        1,
    )
    .unwrap();
    let input: HtmlInputElement = document.element("0/0/1").unwrap().unchecked_into();
    input.set_value("dirty🦀");
    input.focus().unwrap();
    let button: HtmlElement = document.element("0/0/2").unwrap().unchecked_into();
    button.click();
    assert!(document.last_error().is_none());
    let before = document.module_snapshot().unwrap();
    assert_eq!(
        before["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    assert_eq!(
        before["root@0/1"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(0.0).unwrap())
    );
    document
        .update_module_props(BTreeMap::from([(
            "caption".into(),
            Value::String("After".into()),
        )]))
        .unwrap();
    assert_eq!(
        document.element("0/0/0").unwrap().text_content().as_deref(),
        Some("After")
    );
    assert_eq!(input.value(), "dirty🦀");
    assert!(
        input.is_same_node(
            document
                .element("0/0/1")
                .ok()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert!(
        input.is_same_node(
            web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .active_element()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    let other = document.element("0/1").unwrap();
    other.set_attribute("data-external", "tampered").unwrap();
    let baseline = document.module_snapshot().unwrap();
    button.click();
    assert!(document.last_error().is_some());
    assert_eq!(document.module_snapshot().unwrap(), baseline);
    other.remove_attribute("data-external").unwrap();
    button.click();
    assert!(document.last_error().is_none());
    assert_ne!(document.module_snapshot().unwrap(), baseline);
    document.close();
    button.click();
    assert!(document.module_snapshot().is_err());
    assert_eq!(host.child_nodes().length(), 0);
    host.remove();
}

#[wasm_bindgen_test]
fn browser_module_inputs_are_rejected_before_mount_until_the_property_adapter_exists() {
    let graph = meari::parse_module_graph(
        &path("input.tsx"),
        &BTreeMap::from([(
            path("input.tsx"),
            include_str!("../../fixtures/input-state.tsx").into(),
        )]),
    )
    .unwrap();
    let host = host();
    assert!(
        WebDocument::mount_modules(
            &host,
            graph,
            BTreeMap::new(),
            "",
            "browser-module-input".into(),
            1
        )
        .is_err()
    );
    assert_eq!(host.child_nodes().length(), 0);
    host.remove();
}
