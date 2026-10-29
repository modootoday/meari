//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::application::Language;

pub(super) fn prop_type(kind: &PropType, language: Language) -> String {
    use crate::domain::host_callbacks::CallbackSignature;
    match (kind, language) {
        (PropType::Node, Language::Rust) => "NodeSlot".into(),
        (PropType::Node, Language::Tsx) => "ReactNode".into(),
        (PropType::String, Language::Rust) => "String".into(),
        (PropType::String, Language::Tsx) => "string".into(),
        (PropType::Bool, Language::Rust) => "bool".into(),
        (PropType::Bool, Language::Tsx) => "boolean".into(),
        (PropType::Callback(CallbackSignature::Action), Language::Rust) => "ActionCallback".into(),
        (PropType::Callback(CallbackSignature::Text), Language::Rust) => "TextCallback".into(),
        (PropType::Callback(CallbackSignature::Boolean), Language::Rust) => {
            "BooleanCallback".into()
        }
        (PropType::Callback(CallbackSignature::Action), Language::Tsx) => "() => void".into(),
        (PropType::Callback(CallbackSignature::Text), Language::Tsx) => {
            "(value: string) => void".into()
        }
        (PropType::Callback(CallbackSignature::Boolean), Language::Tsx) => {
            "(value: boolean) => void".into()
        }
        (PropType::Array(element), Language::Rust) => {
            format!("Vec<{}>", prop_type(element, language))
        }
        (PropType::Array(element), Language::Tsx) => {
            format!("Array<{}>", prop_type(element, language))
        }
        (PropType::Record(fields), Language::Rust) => records::name(fields),
        (PropType::Record(fields), Language::Tsx) => {
            format!(
                "{{ {} }}",
                fields
                    .iter()
                    .map(|field| format!("{}: {};", field.name, prop_type(&field.kind, language)))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        }
    }
}

pub(super) fn has_node_slots(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => has_node_slots(&repeat.template),
        NodeKind::Text(value) => matches!(value, Value::NodeSlot(_)),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| matches!(attribute.value, Value::NodeSlot(_)))
                || children.iter().any(has_node_slots)
        }
    }
}

pub(super) fn has_predicate(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Text(value) => matches!(value, Value::Predicate(_)),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| matches!(attribute.value, Value::Predicate(_)))
                || children.iter().any(has_predicate)
        }
    }
}

pub(super) fn has_extended_values(node: &Node) -> bool {
    let extended = |value: &Value| {
        matches!(
            value,
            Value::NotProp(_)
                | Value::Callback(_)
                | Value::CallbackReference(_)
                | Value::BoundCallback(_)
        )
    };
    match &node.kind {
        NodeKind::Text(value) => extended(value),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| extended(&attribute.value))
                || children.iter().any(has_extended_values)
        }
    }
}

pub(super) fn has_input_events(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| matches!(attribute.value, Value::Input(_)))
                || children.iter().any(has_input_events)
        }
        NodeKind::Text(_) => false,
    }
}
