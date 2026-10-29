use meari::{application::Language, infrastructure::web_document::WebDocument};
use std::{cell::RefCell, collections::BTreeMap};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, HtmlInputElement};

fn container() -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("section").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

#[wasm_bindgen_test]
fn browser_document_owns_host_observation_lifetime_without_dispatching_component_state() {
    let host = container();
    let source = meari::parse(
        include_str!("../../fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "",
        "host-observation-owner".into(),
        1,
    )
    .unwrap();
    let initial = document.snapshot().unwrap();
    assert!(document.subscribe_media("(min-width:0px)").is_err());
    assert!(document.take_host_observation().is_none());
    document.observe_host().unwrap();
    document.observe_host().unwrap();
    document.take_host_observation().unwrap().unwrap();
    assert_eq!(document.refresh_host_environment().unwrap().revision, 1);
    let subscription = document.subscribe_media("(min-width:0px)").unwrap();
    assert!(subscription.matches);
    let window = web_sys::window().unwrap();
    for _ in 0..3 {
        window
            .dispatch_event(&web_sys::Event::new("resize").unwrap())
            .unwrap();
    }
    let observed = document.poll_host_environment().unwrap().unwrap();
    assert_eq!(observed.revision, 2);
    assert_eq!(
        observed.environment.device_scale,
        window.device_pixel_ratio()
    );
    assert!(observed.media_changes.is_empty());
    window
        .dispatch_event(&web_sys::Event::new("resize").unwrap())
        .unwrap();
    assert_eq!(document.refresh_host_environment().unwrap().revision, 3);
    assert!(document.poll_host_environment().is_none());
    assert!(document.take_host_observation().is_none());
    assert_eq!(document.snapshot().unwrap(), initial);
    assert!(document.unsubscribe_media(subscription.subscription));
    assert!(!document.unsubscribe_media(subscription.subscription));
    assert!(document.stop_observing_host());
    assert!(!document.stop_observing_host());
    window
        .dispatch_event(&web_sys::Event::new("resize").unwrap())
        .unwrap();
    assert!(document.take_host_observation().is_none());
    document.observe_host().unwrap();
    document.close();
    document.close();
    assert!(document.take_host_observation().is_none());
    assert!(document.observe_host().is_err());
    host.remove();
}
fn click(document: &WebDocument, path: &str) {
    document
        .element(path)
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    assert_eq!(document.last_error(), None);
}

#[wasm_bindgen_test]
fn browser_media_state_subscription_patches_disabled_state_and_preserves_dirty_editor() {
    let host = container();
    let source = meari::parse(
        include_str!("../../fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "",
        "media-state-owner".into(),
        1,
    )
    .unwrap();
    document.observe_host().unwrap();
    assert_eq!(
        document.poll_host_environment().unwrap().unwrap().revision,
        1
    );
    let input: HtmlInputElement = document.element("0/4").unwrap().dyn_into().unwrap();
    input.set_value("media draft");
    input.focus().unwrap();
    let before = document.snapshot().unwrap();
    assert!(
        document
            .subscribe_media_state("(min-width:0px)", "count".into())
            .is_err()
    );
    assert_eq!(document.snapshot().unwrap(), before);
    let subscription = document
        .subscribe_media_state("(min-width:0px)", "disabled".into())
        .unwrap();
    assert!(subscription.matches);
    let button: web_sys::HtmlButtonElement = document.element("0/1").unwrap().dyn_into().unwrap();
    assert!(button.disabled());
    assert_eq!(input.value(), "media draft");
    assert!(input.is_same_node(Some(document.element("0/4").unwrap().as_ref())));
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
    let bound = document.snapshot().unwrap();
    button
        .set_attribute("data-external-change", "blocked")
        .unwrap();
    assert!(document.refresh_host_environment().is_err());
    assert_eq!(document.snapshot().unwrap(), bound);
    button.remove_attribute("data-external-change").unwrap();
    assert_eq!(document.refresh_host_environment().unwrap().revision, 2);
    assert!(document.unsubscribe_media(subscription.subscription));
    click(&document, "0/3");
    assert!(!button.disabled());
    assert_eq!(document.refresh_host_environment().unwrap().revision, 3);
    assert!(!button.disabled());
    assert_eq!(input.value(), "media draft");
    drop(document);
    host.remove();
}

#[wasm_bindgen_test]
fn browser_dom_state_patch_retains_dirty_editor_focus_and_text_nodes() {
    let host = container();
    let source = meari::parse(
        include_str!("../../fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "#status { color: rgb(12, 34, 56); }",
        "web-counter".into(),
        1,
    )
    .unwrap();
    let input: HtmlInputElement = host
        .query_selector("#name")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    input.set_value("retained Web editor");
    input.focus().unwrap();
    let units = host.query_selector("#units").unwrap().unwrap();
    click(&document, "0/1");
    click(&document, "0/1");
    click(&document, "0/2");
    assert_eq!(
        host.query_selector("#status")
            .unwrap()
            .unwrap()
            .text_content()
            .unwrap(),
        "Count 2 units ready🦀"
    );
    assert!(
        units.is_same_node(
            host.query_selector("#units")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    assert_eq!(input.value(), "retained Web editor");
    assert!(
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    let style = web_sys::window()
        .unwrap()
        .get_computed_style(&host.query_selector("#status").unwrap().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        style.get_property_value("color").unwrap(),
        "rgb(12, 34, 56)"
    );
    click(&document, "0/3");
    let revision = document.snapshot().unwrap().revision;
    click(&document, "0/1");
    assert_eq!(document.snapshot().unwrap().revision, revision);
    click(&document, "0/3");
    click(&document, "0/1");
    assert_eq!(
        host.query_selector("#status")
            .unwrap()
            .unwrap()
            .text_content()
            .unwrap(),
        "Count 3 units ready🦀"
    );
    let button = document
        .element("0/1")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap();
    let revision = document.snapshot().unwrap().revision;
    document.close();
    button.click();
    assert_eq!(document.snapshot().unwrap().revision, revision);
    assert!(host.first_child().is_none());
    host.remove();
}

#[wasm_bindgen_test]
fn browser_dom_failed_dispatch_and_external_mutation_keep_last_state() {
    let host = container();
    let source = meari::parse("import {useState} from 'react';type Props={};export function Demo(props:Props){const [count,setCount]=useState(0);return <div><p>{count}</p><button onClick={()=>setCount(count/0)}>Bad</button><button onClick={()=>setCount(previous=>previous+1)}>Good</button></div>;}",Language::Tsx).unwrap();
    let document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "",
        "web-failure".into(),
        1,
    )
    .unwrap();
    let initial = document.snapshot().unwrap();
    document
        .element("0/1")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    assert!(document.last_error().is_some());
    assert_eq!(document.snapshot().unwrap(), initial);
    document
        .element("0/0")
        .unwrap()
        .set_text_content(Some("external"));
    document
        .element("0/2")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    assert!(document.last_error().is_some());
    assert_eq!(document.snapshot().unwrap(), initial);
    drop(document);
    assert!(host.first_child().is_none());
    host.remove();
}

#[wasm_bindgen_test]
fn browser_dom_mixed_hook_label_and_updater_match_native_result() {
    let host = container();
    let source = meari::parse(
        include_str!("../../fixtures/mixed-hooks.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let document =
        WebDocument::mount(&host, &source, &BTreeMap::new(), "", "web-mixed".into(), 1).unwrap();
    let input = document.element("0/1").unwrap();
    assert_eq!(
        document.element("0/0").unwrap().get_attribute("for"),
        input.get_attribute("id")
    );
    click(&document, "0/4");
    click(&document, "0/3");
    assert_eq!(
        document.element("0/2").unwrap().text_content().unwrap(),
        "1"
    );
    assert!(input.is_same_node(Some(document.element("0/1").unwrap().as_ref())));
    drop(document);
    host.remove();
}

#[wasm_bindgen_test]
fn browser_dom_mount_scopes_separate_hook_ids_and_reject_duplicate_owners() {
    let host = container();
    let source = meari::parse(
        include_str!("../../fixtures/mixed-hooks.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let first =
        WebDocument::mount(&host, &source, &BTreeMap::new(), "", "scope-one".into(), 1).unwrap();
    let second =
        WebDocument::mount(&host, &source, &BTreeMap::new(), "", "scope-two".into(), 1).unwrap();
    assert_ne!(
        first.element("0/1").unwrap().id(),
        second.element("0/1").unwrap().id()
    );
    assert_eq!(
        first.element("0/0").unwrap().get_attribute("for"),
        first.element("0/1").unwrap().get_attribute("id")
    );
    let mut reordered = source.clone();
    reordered.hook_order.swap(0, 1);
    assert!(
        first
            .prepare_source_replacement(&reordered, &BTreeMap::new())
            .is_err()
    );
    assert!(
        WebDocument::mount(&host, &source, &BTreeMap::new(), "", "scope-one".into(), 1).is_err()
    );
    drop(first);
    drop(second);
    host.remove();
}

thread_local! { static LIVE:RefCell<Option<WebDocument>> = const { RefCell::new(None) }; }

#[wasm_bindgen_test]
fn browser_source_replacement_retains_editor_and_rejects_stale_or_changed_identity() {
    let host = container();
    let original = include_str!("../../fixtures/state-counter.tsx");
    let source = meari::parse(original, Language::Tsx).unwrap();
    let mut document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "",
        "web-source-reload".into(),
        1,
    )
    .unwrap();
    click(&document, "0/1");
    let input: HtmlInputElement = document.element("0/4").unwrap().dyn_into().unwrap();
    input.set_value("retained reload draft");
    input.focus().unwrap();
    let next = original
        .replace("Count ", "Total ")
        .replace("previous + 1", "previous + 10");
    let next = meari::parse(&next, Language::Tsx).unwrap();
    let prepared = document
        .prepare_source_replacement(&next, &BTreeMap::new())
        .unwrap();
    document.commit_source_replacement(prepared).unwrap();
    assert!(input.is_same_node(Some(document.element("0/4").unwrap().as_ref())));
    assert_eq!(input.value(), "retained reload draft");
    assert!(
        web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    click(&document, "0/1");
    assert_eq!(
        host.query_selector("#status")
            .unwrap()
            .unwrap()
            .text_content()
            .as_deref(),
        Some("Total 11 units")
    );
    let prepared = document
        .prepare_source_replacement(&next, &BTreeMap::new())
        .unwrap();
    click(&document, "0/1");
    assert!(document.commit_source_replacement(prepared).is_err());
    let changed = meari::parse(
        &original.replace("id=\"name\"", "id=\"renamed\""),
        Language::Tsx,
    )
    .unwrap();
    assert!(
        document
            .prepare_source_replacement(&changed, &BTreeMap::new())
            .is_err()
    );
    assert_eq!(input.value(), "retained reload draft");
    drop(document);
    host.remove();
}

#[wasm_bindgen_test]
fn browser_dom_live_keyboard_story() {
    let host = container();
    let source = meari::parse("import {useState} from 'react';type Props={};export function WebKeyboard(props:Props){const [count,setCount]=useState(0);return <section aria-label=\"Meari Web runtime\"><p role=\"status\" aria-label=\"Web count\">{count}</p><button onClick={()=>setCount(previous=>previous+1)}>Meari Web increment</button></section>;}",Language::Tsx).unwrap();
    let document = WebDocument::mount(
        &host,
        &source,
        &BTreeMap::new(),
        "[aria-label='Meari Web runtime'] { border: 1px solid #888; padding: 12px; }",
        "web-keyboard".into(),
        1,
    )
    .unwrap();
    LIVE.with_borrow_mut(|slot| *slot = Some(document));
}
