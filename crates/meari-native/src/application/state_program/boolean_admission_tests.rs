#[test]
fn unsupported_boolean_source_and_bound_handlers_are_explicit() {
    use meari::{
        application::{Language, binding},
        domain::{
            Value,
            host_callbacks::{CallbackReference, CallbackSignature},
        },
    };
    use std::collections::BTreeMap;
    let source = "type Props={onCheckedChange:(value:boolean)=>void};export function Panel(props:Props){return <Switch onCheckedChange={value=>props.onCheckedChange(value)}/>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    let bound = binding::bind(
        &component,
        &BTreeMap::from([(
            "onCheckedChange".into(),
            Value::CallbackReference(CallbackReference {
                binding: "checked".into(),
                signature: CallbackSignature::Boolean,
            }),
        )]),
    )
    .unwrap();
    for definition in [&component, &bound] {
        assert!(
            super::validate_callbacks(definition, true)
                .unwrap_err()
                .contains("native Boolean callbacks require")
        );
    }
}

#[test]
fn controlled_boolean_profiles_require_a_scoped_dispatcher_for_source_and_bound_values() {
    use meari::{
        application::{Language, binding},
        domain::{
            Value,
            host_callbacks::{CallbackReference, CallbackSignature},
        },
    };
    use std::collections::BTreeMap;
    for (tag, property, event) in [
        ("Switch", "checked", "onCheckedChange"),
        ("Checkbox", "checked", "onCheckedChange"),
        ("Dialog", "open", "onOpenChange"),
        ("AlertDialog", "open", "onOpenChange"),
    ] {
        let source = format!(
            "type Props={{value:boolean;change:(value:boolean)=>void}};export function Panel(props:Props){{return <{tag} {property}={{props.value}} {event}={{props.change}}/>;}}"
        );
        let component = meari::parse(&source, Language::Tsx).unwrap();
        let bound = binding::bind(
            &component,
            &BTreeMap::from([
                ("value".into(), Value::Bool(false)),
                (
                    "change".into(),
                    Value::CallbackReference(CallbackReference {
                        binding: "changed".into(),
                        signature: CallbackSignature::Boolean,
                    }),
                ),
            ]),
        )
        .unwrap();
        for definition in [&component, &bound] {
            super::validate_callbacks(definition, true).unwrap();
            assert!(
                super::validate_callbacks(definition, false)
                    .unwrap_err()
                    .contains("scoped document dispatcher")
            );
        }
    }
}

#[test]
fn profile_admission_does_not_accept_uncontrolled_or_tristate_or_unknown_widgets() {
    use meari::domain::{Attribute, Value};
    for (tag, event, attributes) in [
        (
            "Switch",
            "onCheckedChange",
            vec![Attribute {
                name: "defaultChecked".into(),
                value: Value::Bool(false),
            }],
        ),
        (
            "Checkbox",
            "onCheckedChange",
            vec![Attribute {
                name: "checked".into(),
                value: Value::String("indeterminate".into()),
            }],
        ),
        (
            "Dialog",
            "onOpenChange",
            vec![Attribute {
                name: "defaultOpen".into(),
                value: Value::Bool(false),
            }],
        ),
        (
            "Popover",
            "onOpenChange",
            vec![Attribute {
                name: "open".into(),
                value: Value::Bool(false),
            }],
        ),
    ] {
        assert!(crate::application::boolean_profile::validate(tag, event, &attributes).is_err());
    }
}
