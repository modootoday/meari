use super::*;
#[path = "../../../../fixtures/range-parts/support.rs"]
mod support;
fn fixture(
    name: &str,
) -> (
    WebDocument,
    Host,
    Element,
    HtmlInputElement,
    BTreeSet<String>,
) {
    let program = support::program(name);
    assert_eq!(support::instances(program.project().unwrap()).len(), 3);
    let paths = support::display_paths(&program.project().unwrap().component.root);
    let (document, host, container, _) = mount_program(program, name);
    let style = container
        .owner_document()
        .unwrap()
        .create_element("style")
        .unwrap();
    style.set_text_content(Some(".meari-parts-compact{width:72px;color:#ff0000;--meari-range-thumb-color:#00ff00;--meari-range-fill-color:#ff0000;--meari-range-track-color:#0000ff}"));
    container.append_child(&style).unwrap();
    let input = container
        .query_selector("input[aria-label='parts-range']")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    (document, host, container, input, paths)
}
fn update(document: &mut WebDocument, value: &str, paths: &BTreeSet<String>) {
    let slot = document
        .node_slot(
            "workspace",
            support::reference(),
            support::reader_props(value),
        )
        .unwrap();
    document
        .update_module_range_props_with_text(support::root_props(slot), paths)
        .unwrap();
}
fn derived(container: &Element) -> String {
    container
        .query_selector("style[data-meari-range-parts-style]")
        .unwrap()
        .unwrap()
        .text_content()
        .unwrap()
}
#[wasm_bindgen_test]
fn browser_compact_parts_sync_actual_ratio_without_mutating_declared_input_style_or_callback_lease()
{
    let (mut document, mut host, container, input, paths) = fixture("web-parts-feedback");
    let scope = document.callback_scope().unwrap();
    let original_style = input.get_attribute("style");
    let computed = web_sys::window()
        .unwrap()
        .get_computed_style(&input)
        .unwrap()
        .unwrap();
    assert_eq!(computed.get_property_value("height").unwrap(), "32px");
    assert_eq!(computed.get_property_value("width").unwrap(), "72px");
    assert!(
        computed
            .get_property_value("background-size")
            .unwrap()
            .contains("10px")
    );
    assert!(
        computed
            .get_property_value("background-size")
            .unwrap()
            .contains("3px")
    );
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    assert_eq!(
        old[0].invocation.arguments,
        CallbackArguments::Text("50".into())
    );
    assert!(derived(&container).contains("--meari-range-position:50%"));
    update(&mut document, "50", &paths);
    assert_eq!(input.get_attribute("style"), original_style);
    assert_eq!(document.callback_scope(), Some(scope));
    assert!(
        input.is_same_node(
            container
                .query_selector("input")
                .unwrap()
                .as_ref()
                .map(|element| element.as_ref())
        )
    );
    for event in old {
        host.owner
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "60");
    deliver(&mut document, &mut host);
    update(&mut document, "60", &paths);
    assert!(derived(&container).contains("--meari-range-position:60%"));
    key(&input, "keyup", "ArrowRight");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("60".into()))]
    );
    assert!(
        container
            .owner_document()
            .unwrap()
            .active_element()
            .unwrap()
            .is_same_node(Some(input.as_ref()))
    );
    document.close();
    container.remove();
}
#[wasm_bindgen_test]
fn browser_compact_parts_controlled_updates_keep_frame_guards_and_remove_owned_style_on_close() {
    let (mut document, mut host, container, input, paths) = fixture("web-parts-controlled");
    update(&mut document, "100", &paths);
    assert_eq!(input.value(), "100");
    assert!(derived(&container).contains("--meari-range-position:100%"));
    update(&mut document, "0", &paths);
    assert_eq!(input.value(), "0");
    assert!(derived(&container).contains("--meari-range-position:0%"));
    let mut disabled = support::reader_props("0");
    disabled.insert("disabled".into(), Value::Bool(true));
    let slot = document
        .node_slot("workspace", support::reference(), disabled)
        .unwrap();
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(slot), &paths)
            .is_err()
    );
    assert_eq!(input.value(), "0");
    assert!(deliver(&mut document, &mut host).is_empty());
    document.close();
    assert!(
        container
            .query_selector("style[data-meari-range-parts-style]")
            .unwrap()
            .is_none()
    );
    container.remove();
}
