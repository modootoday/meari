//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use super::{hook_expression, state_expression};
use crate::application::{Language, Mapping};
use std::fmt::Write;

pub(super) fn value(binding: &Value, language: Language, props: &[Prop]) -> String {
    match binding {
        Value::NodeSlot(node_slots::NodeSlot::Empty) => match language {
            Language::Rust => "empty_node_slot()".into(),
            Language::Tsx => "null".into(),
        },
        Value::NodeSlot(node_slots::NodeSlot::Content(nodes)) => {
            let mut code = String::new();
            code.push_str(if matches!(language, Language::Rust) {
                "node_slot(vec!["
            } else {
                "<>"
            });
            for child in nodes {
                node(child, language, props, &mut code, &mut Vec::new());
                if matches!(language, Language::Rust) {
                    code.push(',');
                }
            }
            code.push_str(if matches!(language, Language::Rust) {
                "])"
            } else {
                "</>"
            });
            code
        }
        Value::NodeSlot(_) => {
            unreachable!("runtime node slots cannot be emitted as authoring source")
        }
        Value::Local(name) => match language {
            Language::Rust => format!("local({name:?})"),
            Language::Tsx => name.clone(),
        },
        Value::Member(member) => match language {
            Language::Rust => format!(
                "member({}, vec![{}])",
                value(&member.root, language, props),
                member
                    .path
                    .iter()
                    .map(|field| format!("{field:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Language::Tsx => format!(
                "{}.{}",
                value(&member.root, language, props),
                member.path.join(".")
            ),
        },
        Value::Repeat(_) => unreachable!("repeat is emitted as a source node"),
        Value::String(s) => match language {
            Language::Rust => format!("{s:?}"),
            Language::Tsx => serde_json::to_string(s).expect("string serialization is infallible"),
        },
        Value::Bool(b) => b.to_string(),
        Value::NotProp(name) => format!("!props.{name}"),
        Value::Predicate(expression) => predicate(expression, language),
        Value::Callback(handler) => match language {
            Language::Rust
                if handler.source
                    == crate::domain::host_callbacks::CallbackSource::CapturedText =>
            {
                format!(
                    "host_text_capture(&props.{}, &props.{})",
                    handler.prop,
                    handler
                        .capture_prop
                        .as_deref()
                        .expect("validated capture prop")
                )
            }
            Language::Rust => format!(
                "{}(&props.{})",
                match handler.source {
                    crate::domain::host_callbacks::CallbackSource::Action => "host_action",
                    crate::domain::host_callbacks::CallbackSource::CurrentTargetText => "host_text",
                    crate::domain::host_callbacks::CallbackSource::BooleanValue => "host_boolean",
                    crate::domain::host_callbacks::CallbackSource::SelectionValue =>
                        "host_selection",
                    crate::domain::host_callbacks::CallbackSource::CapturedText =>
                        unreachable!("capture emitted above"),
                },
                handler.prop
            ),
            Language::Tsx => match handler.source {
                crate::domain::host_callbacks::CallbackSource::CapturedText => format!(
                    "() => props.{}(props.{})",
                    handler.prop,
                    handler
                        .capture_prop
                        .as_deref()
                        .expect("validated capture prop")
                ),
                crate::domain::host_callbacks::CallbackSource::Action => {
                    format!("() => props.{}()", handler.prop)
                }
                crate::domain::host_callbacks::CallbackSource::CurrentTargetText => {
                    format!("event => props.{}(event.currentTarget.value)", handler.prop)
                }
                crate::domain::host_callbacks::CallbackSource::BooleanValue => {
                    format!("value => props.{}(value)", handler.prop)
                }
                crate::domain::host_callbacks::CallbackSource::SelectionValue => {
                    format!("value => props.{}(value)", handler.prop)
                }
            },
        },
        Value::CallbackReference(_)
        | Value::BoundCallback(_)
        | Value::Array(_)
        | Value::Record(_) => {
            unreachable!("runtime binding values are rejected by authoring emit")
        }
        Value::State(name) => match language {
            Language::Rust => format!("state_read(state, {name:?})?"),
            Language::Tsx => name.clone(),
        },
        Value::Event(event) => match language {
            Language::Rust => {
                if matches!(
                    event.update,
                    crate::domain::state_program::StateUpdate::Capture(_)
                        | crate::domain::state_program::StateUpdate::Reduce(_)
                        | crate::domain::state_program::StateUpdate::HookCapture(_)
                        | crate::domain::state_program::StateUpdate::HookReduce(_)
                ) {
                    format!(
                        "state_captured_event(state, {:?})?",
                        serde_json::to_string(event).unwrap()
                    )
                } else {
                    format!("state_event({:?})?", serde_json::to_string(event).unwrap())
                }
            }
            Language::Tsx => {
                use crate::domain::state_program::StateUpdate;
                let update = match &event.update {
                    StateUpdate::Set(value) => state_literal(value),
                    StateUpdate::Toggle => "previous => !previous".into(),
                    StateUpdate::Add(delta) => format!("previous => previous + {delta}"),
                    StateUpdate::Capture(expression) => state_expression::emit(expression),
                    StateUpdate::Reduce(expression) => state_expression::updater(expression),
                    StateUpdate::HookCapture(expression) => hook_expression::emit(expression),
                    StateUpdate::HookReduce(expression) => hook_expression::updater(expression),
                    StateUpdate::Append { item, .. } => {
                        format!(
                            "previous => [...previous, {}]",
                            serde_json::to_string(item).unwrap()
                        )
                    }
                    StateUpdate::Remove { item, .. } => {
                        format!(
                            "previous => previous.filter(item => item !== {})",
                            serde_json::to_string(item).unwrap()
                        )
                    }
                };
                format!("() => {}({update})", event.setter)
            }
        },
        Value::Input(binding) => match language {
            Language::Rust => format!(
                "input_event({:?})?",
                serde_json::to_string(binding).unwrap()
            ),
            Language::Tsx => format!(
                "event => {}(event.currentTarget.{})",
                binding.setter,
                match binding.kind {
                    crate::domain::input_event::InputKind::Text => "value",
                    crate::domain::input_event::InputKind::Checked => "checked",
                }
            ),
        },
        Value::Id(name) => match language {
            Language::Tsx => name.clone(),
            Language::Rust => format!("use_id({name:?})"),
        },
        Value::Prop(p) => match language {
            Language::Rust
                if props
                    .iter()
                    .any(|prop| prop.name == *p && prop.kind != PropType::Bool) =>
            {
                format!("props.{p}.clone()")
            }
            _ => format!("props.{p}"),
        },
    }
}

pub(super) fn state_literal(value: &crate::domain::component_state::StateValue) -> String {
    use crate::domain::component_state::StateValue;
    match value {
        StateValue::Bool(value) => value.to_string(),
        StateValue::String(value) => serde_json::to_string(value).unwrap(),
        StateValue::Number(value) => value.to_string(),
        StateValue::Json(value) => value.to_string(),
        StateValue::Structured(value) => hook_expression::emit(value),
    }
}

pub(super) fn node(
    n: &Node,
    language: Language,
    props: &[Prop],
    code: &mut String,
    mappings: &mut Vec<Mapping>,
) {
    let start = code.len();
    match (&n.kind, language) {
        (NodeKind::Text(Value::Repeat(repeat)), language) => match language {
            Language::Rust => {
                if repeat.key_path.is_empty() {
                    write!(
                        code,
                        "repeat({}, {:?}, {:?}, ",
                        value(&repeat.source, language, props),
                        repeat.item,
                        repeat.key
                    )
                    .unwrap();
                } else {
                    write!(
                        code,
                        "repeat_member({}, {:?}, vec![{}], ",
                        value(&repeat.source, language, props),
                        repeat.item,
                        repeat
                            .key_path
                            .iter()
                            .map(|field| format!("{field:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                    .unwrap();
                }
                node(&repeat.template, language, props, code, mappings);
                code.push(')');
            }
            Language::Tsx => {
                write!(
                    code,
                    "{{{}.map({} => ",
                    value(&repeat.source, language, props),
                    repeat.item
                )
                .unwrap();
                let mut template = repeat.template.clone();
                if let NodeKind::Element { attributes, .. } = &mut template.kind {
                    attributes.push(Attribute {
                        name: "key".into(),
                        value: if repeat.key_path.is_empty() {
                            Value::Local(repeat.key.clone())
                        } else {
                            Value::Member(member_value::MemberValue {
                                root: Box::new(Value::Local(repeat.key.clone())),
                                path: repeat.key_path.clone(),
                            })
                        },
                    });
                }
                node(&template, language, props, code, mappings);
                code.push_str(")}");
            }
        },
        (NodeKind::Text(v), Language::Rust) => {
            write!(code, "text({})", value(v, language, props)).unwrap();
        }
        (NodeKind::Text(v), Language::Tsx) => {
            write!(code, "{{{}}}", value(v, language, props)).unwrap();
        }
        (
            NodeKind::Element {
                tag,
                attributes,
                children,
            },
            Language::Rust,
        ) => {
            write!(code, "element({tag:?}, vec![").unwrap();
            for a in attributes {
                write!(
                    code,
                    "attribute({:?}, {}),",
                    a.name,
                    value(&a.value, language, props)
                )
                .unwrap();
            }
            code.push_str("], vec![");
            for child in children {
                node(child, language, props, code, mappings);
                code.push(',');
            }
            code.push_str("])");
        }
        (
            NodeKind::Element {
                tag,
                attributes,
                children,
            },
            Language::Tsx,
        ) => {
            write!(code, "<{tag}").unwrap();
            for a in attributes {
                let emitted = if a.name == "onValueCommit" {
                    if let Value::Callback(handler) = &a.value {
                        format!("value => props.{}(value)", handler.prop)
                    } else {
                        value(&a.value, language, props)
                    }
                } else {
                    value(&a.value, language, props)
                };
                write!(code, " {}={{{}}}", a.name, emitted).unwrap();
            }
            code.push('>');
            for child in children {
                node(child, language, props, code, mappings);
            }
            write!(code, "</{tag}>").unwrap();
        }
    }
    mappings.push(Mapping {
        source: n.span,
        generated: Span {
            start,
            end: code.len(),
        },
    });
}
