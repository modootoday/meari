use meari::{
    application::Language,
    domain::{
        Span,
        host_callbacks::{
            BoundCallback, CallbackArguments, CallbackReference, CallbackSignature, CallbackSource,
        },
    },
};

const SOURCE: &str = "type Props={workspaceId:string;onNavigate:(value:string)=>void};export function Navigation(props:Props){return <Button onClick={() => props.onNavigate(props.workspaceId)}/>;}";

#[test]
fn captured_prop_roundtrip_preserves_callback_and_argument_identity() {
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 12);
    assert!(
        rust.code
            .contains("host_text_capture(&props.onNavigate, &props.workspaceId)")
    );
    let reparsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&reparsed));
    let tsx = meari::emit(&reparsed, Language::Tsx).unwrap();
    assert!(
        tsx.code
            .contains("() => props.onNavigate(props.workspaceId)")
    );
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
}

#[test]
fn original_navigation_fixture_parses_without_route_or_source_replacement() {
    let source = include_str!("../../fixtures/node-slots/studio/navigation-item.tsx");
    let component = meari::parse(source, Language::Tsx).unwrap();
    let meari::domain::NodeKind::Element { attributes, .. } = &component.root.kind else {
        panic!("original button root required")
    };
    let meari::domain::Value::Callback(handler) = &attributes
        .iter()
        .find(|attribute| attribute.name == "onClick")
        .unwrap()
        .value
    else {
        panic!("source callback required")
    };
    assert_eq!(
        handler.span,
        Span {
            start: 349,
            end: 390
        }
    );
    assert_eq!(handler.prop, "onNavigate");
    assert_eq!(handler.capture_prop.as_deref(), Some("workspaceId"));
    assert_eq!(
        meari::emit(&component, Language::Rust)
            .unwrap()
            .schema_version,
        12
    );
}

#[test]
fn capture_rejects_untyped_and_arbitrary_expressions() {
    for rejected in [
        SOURCE.replace("workspaceId:string", "workspaceId:boolean"),
        SOURCE.replace("props.workspaceId)", "props.missing)"),
        SOURCE.replace("props.workspaceId)", "external)"),
        SOURCE.replace("props.workspaceId)", "\"route\")"),
        SOURCE.replace(
            "props.workspaceId)",
            "props.workspaceId, props.workspaceId)",
        ),
        SOURCE.replace("() =>", "async () =>"),
    ] {
        assert!(
            meari::parse(&rejected, Language::Tsx).is_err(),
            "{rejected}"
        );
    }
}

#[test]
fn bound_capture_accepts_only_activation_and_preserves_empty_unicode_payloads() {
    for value in ["", "작업공간/🌊"] {
        let callback = BoundCallback {
            reference: CallbackReference {
                binding: "navigate".into(),
                signature: CallbackSignature::Text,
            },
            source: CallbackSource::CapturedText,
            span: Span {
                start: 349,
                end: 390,
            },
            captured_text: Some(value.into()),
        };
        let invocation = callback
            .invoke_arguments(CallbackArguments::Action)
            .unwrap();
        assert_eq!(invocation.arguments, CallbackArguments::Text(value.into()));
        assert_eq!(invocation.span, callback.span);
        assert!(
            callback
                .invoke_arguments(CallbackArguments::Text("injected".into()))
                .is_err()
        );
        assert!(
            callback
                .invoke_arguments(CallbackArguments::Boolean(true))
                .is_err()
        );
    }
}

#[test]
fn accepted_prop_binding_captures_each_render_without_mutating_previous_snapshot() {
    use meari::domain::{NodeKind, Value};
    use std::collections::BTreeMap;
    let component = meari::parse(SOURCE, Language::Tsx).unwrap();
    let mut props = BTreeMap::from([
        ("workspaceId".into(), Value::String("first".into())),
        (
            "onNavigate".into(),
            Value::CallbackReference(CallbackReference {
                binding: "navigate".into(),
                signature: CallbackSignature::Text,
            }),
        ),
    ]);
    let first = meari::application::binding::bind(&component, &props).unwrap();
    props.insert("workspaceId".into(), Value::String("두 번째".into()));
    let second = meari::application::binding::bind(&component, &props).unwrap();
    for (bound, expected) in [(&first, "first"), (&second, "두 번째")] {
        let NodeKind::Element { attributes, .. } = &bound.root.kind else {
            panic!("button root required")
        };
        let Value::BoundCallback(callback) = &attributes
            .iter()
            .find(|attribute| attribute.name == "onClick")
            .unwrap()
            .value
        else {
            panic!("bound callback required")
        };
        assert_eq!(
            callback.invoke(None).unwrap().arguments,
            CallbackArguments::Text(expected.into())
        );
    }
    props.insert("workspaceId".into(), Value::Bool(false));
    assert!(meari::application::binding::bind(&component, &props).is_err());
}

#[test]
fn malformed_bound_capture_metadata_is_rejected() {
    let callback = BoundCallback {
        reference: CallbackReference {
            binding: "navigate".into(),
            signature: CallbackSignature::Text,
        },
        source: CallbackSource::CapturedText,
        span: Span::default(),
        captured_text: None,
    };
    assert!(callback.validate().is_err());
    let mut oversized = callback.clone();
    oversized.captured_text = Some("x".repeat(1024 * 1024 + 1));
    assert!(oversized.validate().is_err());
    let mut wrong_source = callback.clone();
    wrong_source.source = CallbackSource::CurrentTargetText;
    wrong_source.captured_text = Some("unexpected".into());
    assert!(wrong_source.validate().is_err());
    let mut wrong_signature = callback;
    wrong_signature.reference.signature = CallbackSignature::Action;
    wrong_signature.captured_text = Some("value".into());
    assert!(wrong_signature.validate().is_err());
}

#[test]
fn existing_callback_json_defaults_capture_to_none_and_capture_json_roundtrips() {
    use meari::domain::host_callbacks::CallbackHandler;
    let old_handler: CallbackHandler =
        serde_json::from_str(r#"{"prop":"onSave","source":"action","span":{"start":0,"end":1}}"#)
            .unwrap();
    assert!(old_handler.capture_prop.is_none());
    assert!(
        !serde_json::to_string(&old_handler)
            .unwrap()
            .contains("capture_prop")
    );
    for (source, signature) in [
        ("action", "action"),
        ("current_target_text", "text"),
        ("boolean_value", "boolean"),
    ] {
        let json = format!(
            r#"{{"reference":{{"binding":"callback","signature":"{signature}"}},"source":"{source}","span":{{"start":0,"end":1}}}}"#
        );
        let bound: BoundCallback = serde_json::from_str(&json).unwrap();
        assert!(bound.captured_text.is_none());
        bound.validate().unwrap();
        assert!(
            !serde_json::to_string(&bound)
                .unwrap()
                .contains("captured_text")
        );
    }
    let capture = BoundCallback {
        reference: CallbackReference {
            binding: "navigate".into(),
            signature: CallbackSignature::Text,
        },
        source: CallbackSource::CapturedText,
        span: Span {
            start: 349,
            end: 390,
        },
        captured_text: Some("작업/🌊".into()),
    };
    let decoded: BoundCallback =
        serde_json::from_str(&serde_json::to_string(&capture).unwrap()).unwrap();
    assert_eq!(decoded, capture);
    decoded.validate().unwrap();
}
