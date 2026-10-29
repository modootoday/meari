use super::*;
#[path = "range_commit/nested_slots.rs"]
mod nested_slots;
#[path = "range_commit/parts.rs"]
mod parts;
use web_sys::{HtmlInputElement, KeyboardEvent, KeyboardEventInit, PointerEvent, PointerEventInit};
const SOURCE: &str = "type Props={disabled:boolean;onDraft:(value:string)=>void;onCommit:(value:string)=>void};export function Range(props:Props){return <main><input type=\"range\" min=\"0\" max=\"100\" step=\"10\" value=\"20\" disabled={props.disabled} onChange={event=>props.onDraft(event.currentTarget.value)} onValueCommit={props.onCommit}/></main>}";
const CONTROLLED: &str = "type Props={value:string;disabled:boolean;onDraft:(value:string)=>void;onCommit:(value:string)=>void};export function Range(props:Props){return <main><input type=\"range\" min=\"0\" max=\"100\" step=\"10\" value={props.value} disabled={props.disabled} onChange={event=>props.onDraft(event.currentTarget.value)} onValueCommit={props.onCommit}/></main>}";
fn controlled_value(value: &str) -> BTreeMap<String, Value> {
    let mut values = props(CONTROLLED);
    values.insert("value".into(), Value::String(value.into()));
    values
}

#[wasm_bindgen_test]
fn browser_range_controlled_feedback_retains_node_export_and_terminal_owner() {
    let (mut document, mut host, container, _) = mount(CONTROLLED, "range-feedback");
    document
        .update_module_range_props(controlled_value("20"))
        .unwrap();
    let input = container
        .query_selector("input")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    let old = document.drain_callbacks();
    assert_eq!(old.len(), 1);
    document
        .update_module_range_props(controlled_value("50"))
        .unwrap();
    assert!(
        input.is_same_node(
            container
                .query_selector("input")
                .unwrap()
                .as_ref()
                .map(|node| node.as_ref())
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
    document
        .update_module_range_props(controlled_value("60"))
        .unwrap();
    key(&input, "keyup", "ArrowRight");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("60".into()))]
    );
    assert!(
        input
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
fn browser_range_full_count_and_byte_queues_preserve_bounded_drain_and_continuation() {
    for byte_full in [false, true] {
        let source = SOURCE.replace(
            "</main>",
            "<textarea onChange={event=>props.onDraft(event.currentTarget.value)}/></main>",
        );
        let (mut document, mut host, container, _) = mount(
            &source,
            if byte_full {
                "range-bytes"
            } else {
                "range-count"
            },
        );
        let input = container
            .query_selector("input")
            .unwrap()
            .unwrap()
            .dyn_into::<HtmlInputElement>()
            .unwrap();
        input.focus().unwrap();
        pointer(&input, "pointerdown");
        input_value(&input, "50");
        deliver(&mut document, &mut host);
        let text = if byte_full {
            "x".repeat(1024 * 1024)
        } else {
            "50".into()
        };
        let count = if byte_full { 4 } else { 1024 };
        let filler = container
            .query_selector("textarea")
            .unwrap()
            .unwrap()
            .dyn_into::<HtmlTextAreaElement>()
            .unwrap();
        for _ in 0..count {
            filler.set_value(&text);
            filler
                .dispatch_event(&Event::new("input").unwrap())
                .unwrap();
        }
        pointer(&input, "pointerup");
        input
            .dispatch_event(&Event::new("change").unwrap())
            .unwrap();
        let first = document.drain_callbacks();
        assert_eq!(first.len(), count);
        assert!(first.iter().all(|event| event.invocation.prop == "onDraft"));
        assert!(document.has_pending_callbacks());
        assert_eq!(
            deliver(&mut document, &mut host),
            vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
        );
        assert!(!document.has_pending_callbacks());
        document.close();
        container.remove();
    }
}
fn pointer(input: &HtmlInputElement, kind: &str) {
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_pointer_id(1);
    init.set_is_primary(true);
    init.set_button(0);
    input
        .dispatch_event(&PointerEvent::new_with_pointer_event_init_dict(kind, &init).unwrap())
        .unwrap();
}
fn key(input: &HtmlInputElement, kind: &str, value: &str) {
    let init = KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(value);
    input
        .dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict(kind, &init).unwrap())
        .unwrap();
}
fn input_value(input: &HtmlInputElement, value: &str) {
    input.set_value(value);
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
}
fn deliver(document: &mut WebDocument, host: &mut Host) -> Vec<(String, CallbackArguments)> {
    let pending = document.drain_callbacks();
    let values = pending
        .iter()
        .map(|event| {
            (
                event.invocation.prop.clone(),
                event.invocation.arguments.clone(),
            )
        })
        .collect();
    for event in pending {
        host.owner
            .dispatch(
                &event.handle,
                event.invocation.arguments,
                &mut host.registry,
            )
            .unwrap();
    }
    values
}
#[wasm_bindgen_test]
fn browser_range_pointer_actual_value_reaches_distinct_host_commit_once() {
    let (mut document, mut host, container, _) = mount(SOURCE, "range-commit-pointer");
    let input = container
        .query_selector("input[type=range]")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    assert!(host.requests.borrow().is_empty());
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("50".into()))]
    );
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert!(deliver(&mut document, &mut host).is_empty());
    document.close();
    container.remove();
}
#[wasm_bindgen_test]
fn browser_range_key_repeat_and_cancel_have_no_extra_commit() {
    let (mut document, mut host, container, _) = mount(SOURCE, "range-commit-key");
    let input = container
        .query_selector("input[type=range]")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.focus().unwrap();
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "30");
    key(&input, "keydown", "ArrowRight");
    input_value(&input, "40");
    assert_eq!(deliver(&mut document, &mut host).len(), 2);
    key(&input, "keyup", "ArrowRight");
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("40".into()))]
    );
    key(&input, "keyup", "ArrowRight");
    assert!(deliver(&mut document, &mut host).is_empty());
    pointer(&input, "pointerdown");
    input_value(&input, "60");
    deliver(&mut document, &mut host);
    pointer(&input, "pointercancel");
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert!(deliver(&mut document, &mut host).is_empty());
    document.close();
    container.remove();
}
#[wasm_bindgen_test]
fn browser_range_disabled_and_owner_close_never_promote_old_commit() {
    let (mut document, mut host, container, _) = mount(SOURCE, "range-commit-owner");
    let input = container
        .query_selector("input[type=range]")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    let old = document.drain_callbacks();
    document.close_callbacks();
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert!(document.drain_callbacks().is_empty());
    for event in old {
        assert!(
            host.owner
                .dispatch(
                    &event.handle,
                    event.invocation.arguments,
                    &mut host.registry
                )
                .is_err()
        );
    }
    input.set_disabled(true);
    pointer(&input, "pointerdown");
    pointer(&input, "pointerup");
    assert!(document.drain_callbacks().is_empty());
    document.close();
    container.remove();
}

#[wasm_bindgen_test]
fn browser_range_display_text_feedback_and_failed_authority_update_keep_original_gesture() {
    let source = CONTROLLED
        .replace("value:string;", "value:string;preview:string;")
        .replace(
            "</main>",
            "<span id=\"preview\">{props.preview}</span></main>",
        );
    let (mut document, mut host, container, _) = mount(&source, "range-display-feedback");
    let values = |value: &str, preview: &str| {
        let mut values = props(&source);
        values.insert("value".into(), Value::String(value.into()));
        values.insert("preview".into(), Value::String(preview.into()));
        values
    };
    document
        .update_module_range_props(values("20", "value"))
        .unwrap();
    let input = container
        .query_selector("input")
        .unwrap()
        .unwrap()
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    let label = container.query_selector("#preview").unwrap().unwrap();
    input.focus().unwrap();
    pointer(&input, "pointerdown");
    input_value(&input, "50");
    deliver(&mut document, &mut host);
    let paths = BTreeSet::from(["0/1/0".into()]);
    assert!(
        document
            .update_module_range_props(values("50", "00:00:50.000"))
            .is_err()
    );
    let mut changed = values("50", "00:00:50.000");
    changed.insert("onDraft".into(), changed["onCommit"].clone());
    assert!(
        document
            .update_module_range_props_with_text(changed, &paths)
            .is_err()
    );
    let mut disabled = values("50", "00:00:50.000");
    disabled.insert("disabled".into(), Value::Bool(true));
    assert!(
        document
            .update_module_range_props_with_text(disabled, &paths)
            .is_err()
    );
    document
        .update_module_range_props_with_text(values("50", "00:00:50.000"), &paths)
        .unwrap();
    assert_eq!(label.text_content().as_deref(), Some("00:00:50.000"));
    assert!(
        label.is_same_node(
            container
                .query_selector("#preview")
                .unwrap()
                .as_ref()
                .map(|node| node.as_ref())
        )
    );
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    pointer(&input, "pointerdown");
    input_value(&input, "80");
    deliver(&mut document, &mut host);
    document
        .update_module_props_with_callbacks(
            values("80", "00:01:20.000"),
            host.owner.export_bindings().unwrap(),
        )
        .unwrap();
    pointer(&input, "pointerup");
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    assert!(deliver(&mut document, &mut host).is_empty());
    document.close();
    container.remove();
}
