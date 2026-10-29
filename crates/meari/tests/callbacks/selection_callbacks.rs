use meari::{
    application::{Language, binding},
    domain::{
        NodeKind, Value,
        host_callbacks::{CallbackArguments, CallbackReference, CallbackSignature, CallbackSource},
    },
};
use std::collections::BTreeMap;

const SOURCE: &str = "import {RadioGroup,RadioGroupItem} from '@/components/ui/radio-group';type Props={selected:string;change:(key:string)=>void};export function Choice(props:Props){return <RadioGroup value={props.selected} onValueChange={props.change}><RadioGroupItem value=\"a\" aria-label=\"A\"/><RadioGroupItem value=\"b\" aria-label=\"B\"/></RadioGroup>;}";

#[test]
fn selection_is_a_distinct_typed_producer_and_roundtrips_bare_and_arrow_sources() {
    let direct = meari::parse(SOURCE, Language::Tsx).unwrap();
    let arrow = meari::parse(
        &SOURCE.replace("{props.change}", "{key=>props.change(key)}"),
        Language::Tsx,
    )
    .unwrap();
    assert!(direct.semantic_eq(&arrow));
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&direct, language).unwrap();
        assert_eq!(output.schema_version, 16);
        assert!(direct.semantic_eq(&meari::parse(&output.code, language).unwrap()));
    }
    let bound = binding::bind(
        &direct,
        &BTreeMap::from([
            ("selected".into(), Value::String(String::new())),
            (
                "change".into(),
                Value::CallbackReference(CallbackReference {
                    binding: "host_select".into(),
                    signature: CallbackSignature::Text,
                }),
            ),
        ]),
    )
    .unwrap();
    let NodeKind::Element { attributes, .. } = &bound.root.kind else {
        panic!()
    };
    let Value::BoundCallback(handler) = &attributes
        .iter()
        .find(|attribute| attribute.name == "onValueChange")
        .unwrap()
        .value
    else {
        panic!()
    };
    assert_eq!(handler.source, CallbackSource::SelectionValue);
    assert_eq!(
        handler
            .invoke_arguments(CallbackArguments::Text("b".into()))
            .unwrap()
            .prop,
        "host_select"
    );
    assert!(handler.invoke_arguments(CallbackArguments::Action).is_err());
    assert!(
        handler
            .invoke_arguments(CallbackArguments::Boolean(true))
            .is_err()
    );
    assert!(meari::emit(&bound, Language::Tsx).is_err());
}

#[test]
fn wrong_context_signature_and_dom_input_payload_are_not_selection_handlers() {
    for source in [
        SOURCE.replace("key:string", "key:boolean"),
        SOURCE.replace("onValueChange={props.change}", "onClick={props.change}"),
        SOURCE.replace(
            "onValueChange={props.change}",
            "onValueChange={event=>props.change(event.currentTarget.value)}",
        ),
        SOURCE.replace(
            "onValueChange={props.change}",
            "onValueChange={()=>props.change(props.selected)}",
        ),
    ] {
        assert!(meari::parse(&source, Language::Tsx).is_err(), "{source}");
    }
}
