use meari::{
    domain::{Value, build::BuildPath},
    infrastructure::web_document::WebDocument,
    runtime::IntoValue,
};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{HtmlElement, HtmlInputElement};

#[wasm_bindgen_test]
fn browser_repeat_state_nodes_focus_dirty_value_and_deleted_listener_lifetime() {
    let entry: BuildPath = "list.tsx".to_owned().try_into().unwrap();
    let graph = meari::parse_module_graph(&entry, &BTreeMap::from([
        (entry.clone(), "import {Counter} from './counter';type Props={refs:string[]};export function List(props:Props){return <ul>{props.refs.map(reference => <Counter key={reference} label={reference}/>)}</ul>;}".into()),
        ("counter.tsx".to_owned().try_into().unwrap(), "import {useState,useId} from 'react';type Props={label:string};export function Counter(props:Props){const field=useId();const [count,setCount]=useState(0);return <li><label htmlFor={field}>{props.label}</label><input id={field}/><button onClick={()=>setCount(previous=>previous+1)}>{count}</button></li>;}".into()),
    ])).unwrap();
    let browser = web_sys::window().unwrap().document().unwrap();
    let host = browser.create_element("section").unwrap();
    browser.body().unwrap().append_child(&host).unwrap();
    let values =
        |refs: Vec<&str>| BTreeMap::<String, Value>::from([("refs".into(), refs.into_value())]);
    let mut document = WebDocument::mount_modules(
        &host,
        graph,
        values(vec!["a", "b"]),
        "",
        "web-repeat-state".into(),
        1,
    )
    .unwrap();
    let row = document.element("0/0").unwrap();
    let input: HtmlInputElement = document.element("0/0/1").unwrap().unchecked_into();
    let button: HtmlElement = document.element("0/0/2").unwrap().unchecked_into();
    input.set_value("dirty🦀");
    input.focus().unwrap();
    button.click();
    assert_eq!(button.text_content().as_deref(), Some("1"));
    document
        .update_module_props(values(vec!["b", "new", "a"]))
        .unwrap();
    assert!(
        document
            .element("0/2")
            .unwrap()
            .is_same_node(Some(row.as_ref()))
    );
    assert!(
        document
            .element("0/2/1")
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    assert!(
        browser
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    assert_eq!(input.value(), "dirty🦀");
    button.click();
    assert_eq!(button.text_content().as_deref(), Some("2"));
    assert_eq!(
        document.element("0/0/2").unwrap().text_content().as_deref(),
        Some("0")
    );
    assert!(
        document.last_error().is_none(),
        "{:?}",
        document.last_error()
    );
    let before = document.module_snapshot().unwrap();
    assert!(
        document
            .update_module_props(values(vec!["same", "same"]))
            .is_err()
    );
    assert_eq!(document.module_snapshot().unwrap(), before);
    assert!(
        document
            .element("0/2")
            .unwrap()
            .is_same_node(Some(row.as_ref()))
    );
    document.update_module_props(values(vec!["b"])).unwrap();
    assert!(!host.contains(Some(row.as_ref())));
    let before = document.module_snapshot().unwrap();
    button.click();
    assert_eq!(document.module_snapshot().unwrap(), before);
    assert!(document.last_error().is_none());
    document.close();
    host.remove();
}
