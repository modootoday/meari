use meari::{
    application::{Language, state_program::StateProgram},
    domain::{component_state::StateValue, input_event::InputPayload},
};

fn source() -> &'static str {
    include_str!("../../fixtures/input-state.tsx")
}

#[test]
fn typed_input_sources_roundtrip_without_becoming_clicks_or_raw_expressions() {
    let component = meari::parse(source(), Language::Tsx).unwrap();
    let rust = meari::emit(&component, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 5);
    assert!(rust.code.contains("input_event("));
    let parsed = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(component.semantic_eq(&parsed));
    let tsx = meari::emit(&parsed, Language::Tsx).unwrap();
    assert_eq!(tsx.schema_version, 5);
    assert!(tsx.code.contains("event.currentTarget.value"));
    assert!(tsx.code.contains("event.currentTarget.checked"));
    assert!(component.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    assert!(meari::application::dom::validate_source(&component).is_err());
}

#[test]
fn source_projection_exposes_typed_input_bindings_to_the_shared_dispatch_sdk() {
    let component = meari::parse(source(), Language::Tsx).unwrap();
    let mut program = StateProgram::mount(component, "input-source".into(), 1).unwrap();
    let projection = program.project().unwrap();
    assert!(projection.events.is_empty());
    assert_eq!(projection.inputs.len(), 2);
    let text = program
        .input_handler(projection.inputs["0/0"].clone())
        .unwrap();
    let checked = program
        .input_handler(projection.inputs["0/1"].clone())
        .unwrap();
    program
        .dispatch_input(&text, &InputPayload::Text("한글 input".into()))
        .unwrap();
    program
        .dispatch_input(&checked, &InputPayload::Checked(true))
        .unwrap();
    let state = program.snapshot().unwrap();
    assert_eq!(
        state.values["text"],
        StateValue::String("한글 input".into())
    );
    assert_eq!(state.values["checked"], StateValue::Bool(true));
}

#[test]
fn input_source_rejects_shadowed_foreign_unsupported_and_mistyped_event_payloads() {
    for invalid in [
        source().replace(
            "event => setText(event.currentTarget.value)",
            "setText => setText(setText.currentTarget.value)",
        ),
        source().replace("event.currentTarget.value", "other.currentTarget.value"),
        source().replace("event.currentTarget.value", "event.currentTarget.files"),
        source().replace("event.currentTarget.value", "event.currentTarget.checked"),
        source().replace("onChange=", "onClick="),
        source().replace("type=\"checkbox\"", "type=\"text\""),
    ] {
        let error = meari::parse(&invalid, Language::Tsx).unwrap_err();
        assert!(error.span.start < error.span.end);
        assert!(error.span.end <= invalid.len());
    }
}
