#![cfg(all(target_arch = "wasm32", feature = "wasm"))]
use meari::{
    application::hook_values::Budget,
    domain::{Value, build::BuildPath},
    infrastructure::web_document::WebDocument,
};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn props(caption: &str, disabled: bool) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("caption".into(), Value::String(caption.into())),
        ("disabled".into(), Value::Bool(disabled)),
    ])
}
fn mount() -> (web_sys::Element, WebDocument) {
    let path = |name: &str| BuildPath::try_from(name.to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &path("root.tsx"),
        &BTreeMap::from([
            (
                path("root.tsx"),
                include_str!("../../fixtures/hooks/modules/root.tsx").into(),
            ),
            (
                path("counter.tsx"),
                include_str!("../../fixtures/hooks/modules/counter.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = doc.create_element("div").unwrap();
    doc.body().unwrap().append_child(&container).unwrap();
    let mounted = WebDocument::mount_hook_modules(
        &container,
        graph,
        props("before", false),
        "",
        ("web-hooks".into(), 1),
        1,
        Budget::default(),
    )
    .unwrap();
    (container, mounted)
}
fn count(document: &WebDocument, key: &str) -> f64 {
    document
        .with_hook_state(|owner, projection| {
            let root = owner
                .snapshot(projection.instance(key).unwrap())
                .unwrap()
                .roots["state"]
                .clone();
            owner
                .values()
                .number(&owner.values().field(&root, "count").unwrap().unwrap())
                .unwrap()
        })
        .unwrap()
}

#[wasm_bindgen_test]
fn actual_web_click_props_patch_and_disabled_admission_use_the_original_heap() {
    let (container, mut document) = mount();
    let button = container.query_selector("button").unwrap().unwrap();
    button.dyn_ref::<web_sys::HtmlElement>().unwrap().click();
    assert_eq!(count(&document, "root@0/0"), 1.0);
    assert_eq!(count(&document, "root@0/1"), 0.0);
    document.update_module_props(props("after", true)).unwrap();
    assert!(
        button.is_same_node(
            container
                .query_selector("button")
                .unwrap()
                .as_ref()
                .map(|node| node.as_ref())
        )
    );
    assert_eq!(button.text_content().as_deref(), Some("after"));
    button.dyn_ref::<web_sys::HtmlElement>().unwrap().click();
    assert_eq!(count(&document, "root@0/0"), 1.0);
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn web_rejected_props_preserve_nodes_and_root_and_close_rejects_live_root_reads() {
    let (container, mut document) = mount();
    let button = container.query_selector("button").unwrap().unwrap();
    let before = document.with_hook_state(|owner, _| owner.stamp()).unwrap();
    assert!(
        document
            .update_module_props(BTreeMap::from([
                ("caption".into(), Value::Bool(true)),
                ("disabled".into(), Value::Bool(false))
            ]))
            .is_err()
    );
    assert_eq!(
        document.with_hook_state(|owner, _| owner.stamp()).unwrap(),
        before
    );
    assert_eq!(button.text_content().as_deref(), Some("before"));
    button.dyn_ref::<web_sys::HtmlElement>().unwrap().click();
    assert_eq!(count(&document, "root@0/0"), 1.0);
    document.close();
    assert!(document.with_hook_state(|_, _| ()).is_err());
    container.remove();
}
