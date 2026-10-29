use crate::application::{Language, Mapping, Output};
use crate::domain::*;
use std::fmt::Write;
mod hook_expression;
pub(crate) mod records;
mod state_expression;

fn value(binding: &Value, language: Language, props: &[Prop]) -> String {
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

fn state_literal(value: &crate::domain::component_state::StateValue) -> String {
    use crate::domain::component_state::StateValue;
    match value {
        StateValue::Bool(value) => value.to_string(),
        StateValue::String(value) => serde_json::to_string(value).unwrap(),
        StateValue::Number(value) => value.to_string(),
        StateValue::Json(value) => value.to_string(),
        StateValue::Structured(value) => hook_expression::emit(value),
    }
}

fn node(
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

pub fn emit(component: &Component, language: Language) -> Output {
    let mut code = String::new();
    let mut mappings = Vec::new();
    let hooks = if component.hook_order.is_empty() {
        crate::domain::hooks::ids(&component.root)
            .into_iter()
            .map(crate::domain::hooks::HookSlot::Id)
            .chain(
                component
                    .states
                    .iter()
                    .map(|state| crate::domain::hooks::HookSlot::State(state.name.clone())),
            )
            .collect::<Vec<_>>()
    } else {
        component.hook_order.clone()
    };
    if component.client {
        code.push_str(match language {
            Language::Tsx => "\"use client\";\n",
            Language::Rust => "pub const BRIDGE_USE_CLIENT: bool = true;\n",
        });
    }
    if matches!(language, Language::Tsx) {
        if !component.states.is_empty() {
            code.push_str("import { useState } from \"react\";\n");
        }
        if !crate::domain::hooks::ids(&component.root).is_empty() {
            code.push_str("import { useId } from \"react\";\n");
        }
        fn imports(
            node: &Node,
            names: &mut std::collections::BTreeMap<
                &'static str,
                std::collections::BTreeSet<String>,
            >,
        ) {
            if let NodeKind::Text(Value::Repeat(repeat)) = &node.kind {
                imports(&repeat.template, names);
            }
            if let NodeKind::Element { tag, children, .. } = &node.kind {
                if let Some(module) = crate::domain::design::components::module(tag) {
                    names.entry(module).or_default().insert(tag.clone());
                }
                for child in children {
                    imports(child, names);
                }
            }
        }
        let mut names = std::collections::BTreeMap::new();
        imports(&component.root, &mut names);
        for (module, names) in names {
            writeln!(
                code,
                "import {{ {} }} from \"@/components/ui/{module}\";",
                names.into_iter().collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }
    }
    match language {
        Language::Rust => {
            code.push_str("use meari::runtime::*;\n\n");
            records::emit(&component.props, &mut code);
            // React prop spelling is part of the source contract, including camelCase.
            if component
                .props
                .iter()
                .any(|prop| prop.name.chars().any(|ch| ch.is_ascii_uppercase()))
            {
                code.push_str("#[allow(non_snake_case)]\n");
            }
            code.push_str("pub struct Props {\n");
            for p in &component.props {
                writeln!(
                    code,
                    "    pub {}: {},",
                    p.name,
                    prop_type(&p.kind, language)
                )
                .unwrap();
            }
            let stateful = !component.states.is_empty();
            writeln!(
                code,
                "}}\n\n#[allow({})]\npub fn {}(props: Props{}) -> {} {{",
                if stateful {
                    "non_snake_case, unused_variables"
                } else {
                    "non_snake_case"
                },
                component.name,
                if stateful {
                    ", state: &StateContext"
                } else {
                    ""
                },
                if stateful { "RenderResult" } else { "Node" }
            )
            .unwrap();
            for hook in &hooks {
                let crate::domain::hooks::HookSlot::State(name) = hook else {
                    if !component.hook_order.is_empty() {
                        let crate::domain::hooks::HookSlot::Id(name) = hook else {
                            unreachable!()
                        };
                        writeln!(code, "    use_id({name:?});").unwrap();
                    }
                    continue;
                };
                let binding = component
                    .states
                    .iter()
                    .find(|binding| &binding.name == name)
                    .expect("validated hook slot");
                let start = code.len();
                writeln!(
                    code,
                    "    state_declaration(state, {:?}, {:?}, {:?})?;",
                    binding.name,
                    binding.setter,
                    serde_json::to_string(&binding.initial).unwrap()
                )
                .unwrap();
                mappings.push(Mapping {
                    source: binding.span,
                    generated: Span {
                        start,
                        end: code.len(),
                    },
                });
            }
            code.push_str("    ");
            if stateful {
                code.push_str("Ok(");
            }
        }
        Language::Tsx => {
            if component
                .props
                .iter()
                .any(|prop| prop.kind == PropType::Node)
            {
                code.push_str("import type { ReactNode } from \"react\";\n");
            }
            code.push_str("type Props = {\n");
            for p in &component.props {
                writeln!(code, "  {}: {};", p.name, prop_type(&p.kind, language)).unwrap();
            }
            writeln!(
                code,
                "}};\n\nexport function {}(props: Props) {{",
                component.name
            )
            .unwrap();
            for hook in &hooks {
                let crate::domain::hooks::HookSlot::State(name) = hook else {
                    let crate::domain::hooks::HookSlot::Id(name) = hook else {
                        unreachable!()
                    };
                    writeln!(code, "  const {name} = useId();").unwrap();
                    continue;
                };
                let binding = component
                    .states
                    .iter()
                    .find(|binding| &binding.name == name)
                    .expect("validated hook slot");
                let start = code.len();
                writeln!(
                    code,
                    "  const [{}, {}] = useState({});",
                    binding.name,
                    binding.setter,
                    state_literal(&binding.initial)
                )
                .unwrap();
                mappings.push(Mapping {
                    source: binding.span,
                    generated: Span {
                        start,
                        end: code.len(),
                    },
                });
            }
            code.push_str("  return (");
        }
    }
    node(
        &component.root,
        language,
        &component.props,
        &mut code,
        &mut mappings,
    );
    if matches!(language, Language::Rust) && !component.states.is_empty() {
        code.push(')');
    }
    code.push_str(match language {
        Language::Rust => "\n}\n",
        Language::Tsx => ");\n}\n",
    });
    Output {
        schema_version: if has_predicate_matching(
            &component.root,
            crate::domain::bool_expression::BoolExpression::contains_member,
        ) {
            17
        } else if has_callback_source(
            &component.root,
            crate::domain::host_callbacks::CallbackSource::SelectionValue,
        ) {
            16
        } else if component
            .props
            .iter()
            .any(|prop| prop.kind.contains_record())
        {
            15
        } else if has_predicate_matching(
            &component.root,
            crate::domain::bool_expression::BoolExpression::contains_state,
        ) {
            14
        } else if has_predicate_matching(
            &component.root,
            crate::domain::bool_expression::BoolExpression::contains_comparison,
        ) {
            13
        } else if has_callback_capture(&component.root) {
            12
        } else if component
            .props
            .iter()
            .any(|prop| has_boolean_callback(&prop.kind))
        {
            11
        } else if component
            .props
            .iter()
            .any(|prop| prop.kind == PropType::Node)
            || has_node_slots(&component.root)
        {
            10
        } else if has_repeat(&component.root) {
            9
        } else if component
            .props
            .iter()
            .any(|prop| matches!(prop.kind, PropType::Array(_)))
        {
            8
        } else if has_predicate(&component.root) {
            7
        } else if has_extended_values(&component.root)
            || component
                .props
                .iter()
                .any(|prop| matches!(prop.kind, PropType::Callback(_)))
        {
            6
        } else if has_input_events(&component.root) {
            5
        } else if !component.hook_order.is_empty() {
            4
        } else if component.states.is_empty() {
            2
        } else {
            3
        },
        code,
        mappings,
        design_system: crate::domain::design::analyze(component),
    }
}

fn has_repeat(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Text(Value::Repeat(_)) => true,
        NodeKind::Text(_) => false,
        NodeKind::Element { children, .. } => children.iter().any(has_repeat),
    }
}

fn has_callback_capture(node: &Node) -> bool {
    has_callback_source(
        node,
        crate::domain::host_callbacks::CallbackSource::CapturedText,
    )
}

fn has_callback_source(node: &Node, source: crate::domain::host_callbacks::CallbackSource) -> bool {
    let captures = |value: &Value| match value {
        Value::Callback(handler) => handler.source == source,
        Value::Repeat(repeat) => has_callback_source(&repeat.template, source),
        Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) => {
            nodes.iter().any(|node| has_callback_source(node, source))
        }
        _ => false,
    };
    match &node.kind {
        NodeKind::Text(value) => captures(value),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| captures(&attribute.value))
                || children
                    .iter()
                    .any(|node| has_callback_source(node, source))
        }
    }
}

fn has_boolean_callback(kind: &PropType) -> bool {
    match kind {
        PropType::Callback(crate::domain::host_callbacks::CallbackSignature::Boolean) => true,
        PropType::Array(element) => has_boolean_callback(element),
        PropType::Record(fields) => fields.iter().any(|field| has_boolean_callback(&field.kind)),
        _ => false,
    }
}

fn has_predicate_matching(
    node: &Node,
    predicate: fn(&crate::domain::bool_expression::BoolExpression) -> bool,
) -> bool {
    let compares = |value: &Value| match value {
        Value::Predicate(expression) => predicate(expression),
        Value::Repeat(repeat) => has_predicate_matching(&repeat.template, predicate),
        Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) => nodes
            .iter()
            .any(|node| has_predicate_matching(node, predicate)),
        _ => false,
    };
    match &node.kind {
        NodeKind::Text(value) => compares(value),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| compares(&attribute.value))
                || children
                    .iter()
                    .any(|node| has_predicate_matching(node, predicate))
        }
    }
}

fn predicate(
    expression: &crate::domain::bool_expression::BoolExpression,
    language: Language,
) -> String {
    use crate::domain::bool_expression::BoolExpressionKind as Kind;
    let operand = |value: &crate::domain::bool_expression::BoolExpression| {
        let text = predicate(value, language);
        if matches!(
            value.kind,
            Kind::And(..) | Kind::Or(..) | Kind::Equal(..) | Kind::NotEqual(..)
        ) {
            format!("({text})")
        } else {
            text
        }
    };
    match &expression.kind {
        Kind::Literal(value) => value.to_string(),
        Kind::Prop(name) => format!("props.{name}"),
        Kind::Member(member) => value(&Value::Member(member.clone()), language, &[]),
        Kind::State(name) => match language {
            Language::Tsx => name.clone(),
            Language::Rust => format!("state_bool(state, {name:?})?"),
        },
        Kind::Not(argument) => format!("!{}", operand(argument)),
        Kind::And(left, right) => format!("{} && {}", operand(left), operand(right)),
        Kind::Or(left, right) => format!("{} || {}", operand(left), operand(right)),
        Kind::Equal(left, right) | Kind::NotEqual(left, right) => {
            let unequal = matches!(expression.kind, Kind::NotEqual(..));
            let operator = match (language, unequal) {
                (Language::Rust, false) => "==",
                (Language::Rust, true) => "!=",
                (Language::Tsx, false) => "===",
                (Language::Tsx, true) => "!==",
            };
            format!(
                "{} {operator} {}",
                scalar_operand(left, language),
                scalar_operand(right, language)
            )
        }
    }
}

fn scalar_operand(
    expression: &crate::domain::bool_expression::ScalarExpression,
    language: Language,
) -> String {
    use crate::domain::bool_expression::ScalarExpressionKind;
    match &expression.kind {
        ScalarExpressionKind::String(value) => match language {
            Language::Rust => format!("{value:?}"),
            Language::Tsx => {
                serde_json::to_string(value).expect("string serialization is infallible")
            }
        },
        ScalarExpressionKind::Bool(value) => value.to_string(),
        ScalarExpressionKind::Prop(name) => format!("props.{name}"),
        ScalarExpressionKind::Member(member) => {
            value(&Value::Member(member.clone()), language, &[])
        }
    }
}

fn prop_type(kind: &PropType, language: Language) -> String {
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
fn has_node_slots(node: &Node) -> bool {
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

fn has_predicate(node: &Node) -> bool {
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

fn has_extended_values(node: &Node) -> bool {
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

fn has_input_events(node: &Node) -> bool {
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
