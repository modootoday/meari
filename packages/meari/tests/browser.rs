#![cfg(all(target_arch = "wasm32", feature = "wasm"))]
use meari::{
    application::Language,
    wasm::{convert_source, prepare_native_plan},
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);
#[path = "browser/application_reload.rs"]
mod application_reload;
#[path = "browser/dom.rs"]
mod dom;
#[path = "browser/node_slots.rs"]
mod node_slots;
#[path = "browser/repeat.rs"]
mod repeat;
#[path = "browser/source_callbacks.rs"]
mod source_callbacks;
#[path = "browser/source_modules.rs"]
mod source_modules;

#[wasm_bindgen_test]
fn browser_build_graph_uses_canonical_sha256_and_rejects_cycles() {
    use meari::domain::build::{BuildGraph, BuildPath, ContentHash, InputFile, InputRole};
    use std::collections::{BTreeMap, BTreeSet};
    assert_eq!(
        ContentHash::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let entry = BuildPath::try_from("view.tsx".to_owned()).unwrap();
    let mut graph = BuildGraph {
        entry: entry.clone(),
        files: BTreeMap::from([(
            entry.clone(),
            InputFile {
                role: InputRole::Component,
                content: ContentHash::of(b"abc"),
                length: 3,
                dependencies: BTreeSet::new(),
                license: None,
            },
        )]),
    };
    let json = serde_json::to_vec(&graph).unwrap();
    assert_eq!(graph.fingerprint().unwrap(), ContentHash::of(&json));
    assert_eq!(graph, serde_json::from_slice::<BuildGraph>(&json).unwrap());
    graph
        .files
        .get_mut(&entry)
        .unwrap()
        .dependencies
        .insert(entry);
    assert!(graph.validate().is_err());
}

#[wasm_bindgen_test]
fn browser_state_expression_callbacks_preserve_render_closures() {
    use meari::application::state_program::StateProgram;
    let component =
        meari::parse(include_str!("../fixtures/state-capture.tsx"), Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let mut program = StateProgram::mount(component, "browser-closures".into(), 1).unwrap();
    let projection = program.project().unwrap();
    let capture = projection.events["0/2"].clone();
    let updater = projection.events["0/3"].clone();
    program.dispatch(&capture).unwrap();
    program.dispatch(&capture).unwrap();
    program.dispatch(&projection.events["0/4"]).unwrap();
    program.dispatch(&updater).unwrap();
    let meari::domain::component_state::StateValue::Number(count) =
        &program.snapshot().unwrap().values["count"]
    else {
        panic!()
    };
    assert_eq!(count.as_f64(), Some(4.0));
}

#[wasm_bindgen_test]
fn browser_source_state_roundtrip_and_functional_setters_use_shared_runtime() {
    use meari::application::state_program::StateProgram;
    let source = include_str!("../fixtures/state-counter.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::convert(source, Language::Tsx, Language::Rust).unwrap();
    let restored = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&restored));
    let mut program = StateProgram::mount(component, "browser-state-source".into(), 1).unwrap();
    let projection = program.project().unwrap();
    let event = projection
        .events
        .values()
        .find(|event| event.setter == "setCount")
        .unwrap()
        .clone();
    program.dispatch(&event).unwrap();
    program.dispatch(&event).unwrap();
    let meari::domain::component_state::StateValue::Number(count) =
        &program.snapshot().unwrap().values["count"]
    else {
        panic!()
    };
    assert_eq!(count.as_f64(), Some(2.0));
}

#[wasm_bindgen_test]
fn browser_component_instances_share_typed_actions_and_reject_stale_lifetimes() {
    use meari::{
        application::component_state::ComponentState,
        domain::component_state::{InstanceKey, StateAction, StateSchema, StateValue},
    };
    let mut state = ComponentState::new("browser-component".into(), 1).unwrap();
    let schema: StateSchema =
        serde_json::from_value(serde_json::json!({"revision":"state-v1","slots":{
            "open":{"kind":"bool","initial":{"type":"bool","value":false}},
            "count":{"kind":"number","initial":{"type":"number","value":0}}
        }}))
        .unwrap();
    let handle = state
        .mount(
            None,
            InstanceKey::Explicit("root".into()),
            "app".into(),
            schema.clone(),
        )
        .unwrap();
    let commit = state
        .dispatch(
            &handle,
            0,
            &[
                StateAction::Toggle {
                    slot: "open".into(),
                },
                StateAction::Add {
                    slot: "count".into(),
                    delta: 1.into(),
                },
            ],
        )
        .unwrap();
    assert_eq!(commit.values["open"], StateValue::Bool(true));
    assert_eq!(commit.values["count"], StateValue::Number(1.into()));
    assert_eq!(
        state
            .mount(
                None,
                InstanceKey::Explicit("root".into()),
                "app".into(),
                schema
            )
            .unwrap(),
        handle
    );
    state.unmount(&handle).unwrap();
    assert!(
        state
            .dispatch(
                &handle,
                1,
                &[StateAction::Toggle {
                    slot: "open".into()
                }]
            )
            .is_err()
    );
}

#[wasm_bindgen_test]
fn browser_host_snapshot_uses_actual_viewports_and_preserves_unknown_capabilities() {
    use meari::domain::host::{HostTarget, Support};
    let observed = meari::infrastructure::web_host::snapshot().unwrap();
    observed.validate().unwrap();
    let window = web_sys::window().unwrap();
    assert_eq!(observed.target, HostTarget::Web);
    assert_eq!(observed.device_scale, window.device_pixel_ratio());
    assert_eq!(
        observed.visual.width,
        window.visual_viewport().unwrap().width()
    );
    assert_eq!(
        observed.layout.width,
        window.inner_width().unwrap().as_f64().unwrap().round() as u32
    );
    assert!(observed.safe_area.is_none());
    assert_eq!(observed.capabilities.client_resize, Support::Unsupported);
    assert_eq!(observed.capabilities.preview_resize, Support::Unknown);
}

#[wasm_bindgen_test]
fn browser_host_observer_coalesces_events_and_detaches_on_close() {
    use meari::infrastructure::web_host::observer::BrowserHostObserver;
    let mut observer = BrowserHostObserver::new().unwrap();
    observer.take_pending().unwrap().unwrap();
    assert!(observer.take_pending().is_none());
    let window = web_sys::window().unwrap();
    for _ in 0..3 {
        window
            .dispatch_event(&web_sys::Event::new("resize").unwrap())
            .unwrap();
    }
    let observed = observer.take_pending().unwrap().unwrap();
    assert_eq!(observed.device_scale, window.device_pixel_ratio());
    assert!(observer.take_pending().is_none());
    window
        .visual_viewport()
        .unwrap()
        .dispatch_event(&web_sys::Event::new("scroll").unwrap())
        .unwrap();
    observer.take_pending().unwrap().unwrap();
    observer.close();
    observer.close();
    window
        .dispatch_event(&web_sys::Event::new("resize").unwrap())
        .unwrap();
    assert!(observer.take_pending().is_none());
}

#[wasm_bindgen_test]
fn browser_media_uses_shared_subscription_lifecycle() {
    use meari::{
        application::media::{MediaQueryEvaluator, MediaSubscriptions},
        infrastructure::web_media::BrowserMedia,
    };
    let mut browser = BrowserMedia;
    assert!(browser.matches_media("(min-width:0px)").unwrap());
    assert!(!browser.matches_media("(min-width:999999px)").unwrap());
    assert!(!browser.matches_media("(invalid-feature:yes)").unwrap());
    let mut subscriptions = MediaSubscriptions::default();
    let initial = subscriptions
        .subscribe(&mut browser, "(min-width:0px)")
        .unwrap();
    assert!(initial.matches);
    assert!(subscriptions.refresh(&mut browser).unwrap().is_empty());
    assert!(subscriptions.unsubscribe(initial.subscription));
    assert!(subscriptions.refresh(&mut browser).unwrap().is_empty());
}

#[wasm_bindgen_test]
fn browser_native_plan_uses_the_default_design_contract() {
    use meari::domain::design::utilities::COLORS;
    let colors: serde_json::Map<String, serde_json::Value> = COLORS
        .iter()
        .map(|name| ((*name).into(), serde_json::json!([10, 20, 30, 255])))
        .collect();
    let request = serde_json::json!({
        "source": "type Props = { label: string; }; export function Demo(props: Props) { return <Button variant=\"outline\" className=\"md:px-6\">{props.label}</Button>; }",
        "language": "tsx", "props": {"label": {"String":"한글🦀"}},
        "theme": {"light":colors, "dark":colors},
        "environment": {"viewport_width":800.0, "root_font_px":16.0, "dark":false}
    });
    let plan: serde_json::Value =
        serde_json::from_str(&prepare_native_plan(&request.to_string()).unwrap()).unwrap();
    assert_eq!(plan["profile"], "tailwind-shadcn-v1");
    assert_eq!(plan["root"]["style"]["PaddingLeft"]["Px"], 24.0);
    assert_eq!(plan["root"]["children"][0]["text"], "한글🦀");
}

#[wasm_bindgen_test]
fn browser_roundtrip_through_public_wasm_api() {
    let source = include_str!("../fixtures/card.rs");
    let tsx: serde_json::Value =
        serde_json::from_str(&convert_source(source, "rust", "tsx").unwrap()).unwrap();
    assert_eq!(tsx["schema_version"], 2);
    assert_eq!(tsx["design_system"]["profile"], "tailwind-shadcn-v1");
    assert_eq!(tsx["mappings"].as_array().unwrap().len(), 4);
    let rust: serde_json::Value = serde_json::from_str(
        &convert_source(tsx["code"].as_str().unwrap(), "tsx", "rust").unwrap(),
    )
    .unwrap();
    let original = meari::parse(source, Language::Rust).unwrap();
    let roundtrip = meari::parse(rust["code"].as_str().unwrap(), Language::Rust).unwrap();
    assert!(original.semantic_eq(&roundtrip));
}

#[wasm_bindgen_test]
fn browser_unsupported_and_syntax_errors_are_structured() {
    for (source, from, to, code) in [
        ("@@@", "tsx", "rust", "syntax"),
        ("", "unknown", "rust", "input"),
        (
            "type Props = {}; export function Card(props: Props) { useEffect(() => {}); return <div />; }",
            "tsx",
            "rust",
            "unsupported",
        ),
    ] {
        let diagnostic: serde_json::Value = serde_json::from_str(
            &convert_source(source, from, to)
                .unwrap_err()
                .as_string()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(diagnostic["code"], code);
        assert!(diagnostic["span"].is_object());
    }
}

#[wasm_bindgen_test]
fn browser_unicode_mapping_uses_utf8_bytes() {
    let source = include_str!("../fixtures/card.rs").replace(
        "text(props.label.clone())",
        "text(\"한글🦀\"), text(props.label.clone())",
    );
    let output: serde_json::Value =
        serde_json::from_str(&convert_source(&source, "rust", "tsx").unwrap()).unwrap();
    assert!(output["code"].as_str().unwrap().contains("한글🦀"));
    assert!(
        output["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mapping| {
                let start = mapping["source"]["start"].as_u64().unwrap() as usize;
                let end = mapping["source"]["end"].as_u64().unwrap() as usize;
                source.get(start..end) == Some("text(props.label.clone())")
            })
    );
}

#[wasm_bindgen_test]
fn browser_mixed_hook_order_survives_both_frontends_and_dispatch() {
    let component =
        meari::parse(include_str!("../fixtures/mixed-hooks.tsx"), Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert!(component.semantic_eq(&meari::parse(&rust.code, Language::Rust).unwrap()));
    let mut program = meari::application::state_program::StateProgram::mount(
        component,
        "browser-mixed".into(),
        1,
    )
    .unwrap();
    let initial = program.project().unwrap();
    let changed = program.dispatch(&initial.events["0/4"]).unwrap();
    assert_eq!(
        meari::domain::hooks::ids(&initial.component.root),
        meari::domain::hooks::ids(&changed.component.root)
    );
}
