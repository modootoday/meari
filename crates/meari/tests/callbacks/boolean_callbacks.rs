use meari::{
    application::Language,
    domain::{
        Span,
        host_callbacks::{CallbackArguments, CallbackHandler, CallbackSource},
    },
};

#[test]
fn boolean_payload_is_typed_and_legacy_text_cannot_coerce_it() {
    let handler = CallbackHandler {
        prop: "onOpenChange".into(),
        source: CallbackSource::BooleanValue,
        capture_prop: None,
        span: Span { start: 8, end: 32 },
    };
    for value in [false, true] {
        let invocation = handler
            .invoke_arguments(CallbackArguments::Boolean(value))
            .unwrap();
        assert_eq!(invocation.arguments, CallbackArguments::Boolean(value));
        assert_eq!(invocation.span, handler.span);
        assert_eq!(
            serde_json::to_string(&invocation.arguments).unwrap(),
            format!(r#"{{"kind":"boolean","value":{value}}}"#)
        );
    }
    assert!(handler.invoke(None).is_err());
    assert!(handler.invoke(Some("true")).is_err());
    assert!(
        serde_json::from_str::<CallbackArguments>(r#"{"kind":"boolean","value":"true"}"#).is_err()
    );
}

#[test]
fn boolean_callback_roundtrip_preserves_exact_direct_value_profile() {
    let source = "type Props={open:boolean;onOpenChange:(value:boolean)=>void};export function Panel(props:Props){return <Dialog open={props.open} onOpenChange={value=>props.onOpenChange(value)}/>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    let direct = meari::parse(
        &source.replace("value=>props.onOpenChange(value)", "props.onOpenChange"),
        Language::Tsx,
    )
    .unwrap();
    assert!(component.semantic_eq(&direct));
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 11);
    assert!(rust.code.contains("BooleanCallback"));
    assert!(rust.code.contains("host_boolean"));
    let reparsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&reparsed));
    let tsx = meari::emit(&reparsed, Language::Tsx).unwrap();
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    for rejected in [
        source.replace("props.onOpenChange(value)", "props.onOpenChange(!value)"),
        source.replace("props.onOpenChange(value)", "props.onOpenChange(other)"),
        source.replace("(value:boolean)", "(value:string)"),
        source.replace("onOpenChange={", "onClick={"),
    ] {
        assert!(
            meari::parse(&rejected, Language::Tsx).is_err(),
            "{rejected}"
        );
    }
}

#[test]
fn nested_boolean_callback_types_require_schema_eleven() {
    for kind in [
        "Array<(value:boolean)=>void>",
        "Array<Array<(value:boolean)=>void>>",
    ] {
        let source = format!(
            "type Props={{callbacks:{kind}}};export function Panel(props:Props){{return <div/>;}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let rust = meari::emit(&component, Language::Rust).unwrap();
        assert_eq!(rust.schema_version, 11);
        let reparsed = meari::parse(&rust.code, Language::Rust).unwrap();
        assert!(component.semantic_eq(&reparsed));
    }
    let legacy = meari::parse("type Props={callbacks:Array<(value:string)=>void>};export function Panel(props:Props){return <div/>;}", Language::Tsx).unwrap();
    assert_eq!(
        meari::emit(&legacy, Language::Rust).unwrap().schema_version,
        8
    );
}

#[test]
fn original_inspector_dock_preserves_slots_boolean_callback_and_overlay_props() {
    use meari::domain::{PropType, host_callbacks::CallbackSignature};
    let source = include_str!("../../fixtures/node-slots/studio/inspector-dock.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    assert!(
        component
            .props
            .iter()
            .any(|prop| prop.name == "onOpenChange"
                && prop.kind == PropType::Callback(CallbackSignature::Boolean))
    );
    assert!(
        component
            .props
            .iter()
            .any(|prop| prop.name == "children" && prop.kind == PropType::Node)
    );
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 11);
    let reparsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&reparsed));
    let tsx = meari::emit(&reparsed, Language::Tsx).unwrap();
    for name in ["onOpenChange", "modal", "forceMount", "asChild", "hidden"] {
        assert!(tsx.code.contains(name), "{name}");
    }
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
}
