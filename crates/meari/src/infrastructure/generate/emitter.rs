//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::application::{Language, Mapping, Output};
use std::fmt::Write;

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

pub(super) fn has_repeat(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Text(Value::Repeat(_)) => true,
        NodeKind::Text(_) => false,
        NodeKind::Element { children, .. } => children.iter().any(has_repeat),
    }
}

pub(super) fn has_callback_capture(node: &Node) -> bool {
    has_callback_source(
        node,
        crate::domain::host_callbacks::CallbackSource::CapturedText,
    )
}

pub(super) fn has_callback_source(
    node: &Node,
    source: crate::domain::host_callbacks::CallbackSource,
) -> bool {
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

pub(super) fn has_boolean_callback(kind: &PropType) -> bool {
    match kind {
        PropType::Callback(crate::domain::host_callbacks::CallbackSignature::Boolean) => true,
        PropType::Array(element) => has_boolean_callback(element),
        PropType::Record(fields) => fields.iter().any(|field| has_boolean_callback(&field.kind)),
        _ => false,
    }
}

pub(super) fn has_predicate_matching(
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

pub(super) fn predicate(
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

pub(super) fn scalar_operand(
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
