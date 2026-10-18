#![cfg(all(target_arch = "wasm32", feature = "wasm"))]
use meari::{
    application::Language,
    wasm::{convert_source, prepare_native_plan},
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

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
