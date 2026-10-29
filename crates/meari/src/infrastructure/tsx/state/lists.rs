//! State-driven keyed lists: `const [items, setItems] = useState(["a", "b"])` rendered by
//! `items.map(item => <li key={item}>{item}</li>)`.
//!
//! A state read only as a `.map` source with a dense string-array literal is held as a JSON
//! string array, which the scalar state program can store, replace with `Set` literals, and
//! expand per projection. Any other update shape stays structured and is rejected by
//! validation instead of being approximated.
use crate::domain::{
    Node, NodeKind, Value,
    component_state::StateValue,
    node_slots::NodeSlot,
    state_program::{StateBinding, StateUpdate, string_items},
};
use oxc_ast::ast::{ArrayExpressionElement, BindingPattern, Expression};
use oxc_syntax::operator::BinaryOperator;

fn sources(node: &Node, out: &mut Vec<String>) {
    match &node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => {
            if let Value::State(name) = &repeat.source {
                out.push(name.clone());
            }
            sources(&repeat.template, out);
        }
        NodeKind::Text(Value::NodeSlot(NodeSlot::Content(nodes))) => {
            nodes.iter().for_each(|child| sources(child, out))
        }
        NodeKind::Text(_) => {}
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                if let Value::NodeSlot(NodeSlot::Content(nodes)) = &attribute.value {
                    nodes.iter().for_each(|child| sources(child, out));
                }
            }
            children.iter().for_each(|child| sources(child, out));
        }
    }
}

fn json(value: &StateValue) -> Option<StateValue> {
    let items = string_items(value)?;
    (!items.is_empty()).then(|| {
        StateValue::Json(serde_json::Value::Array(
            items.into_iter().map(serde_json::Value::String).collect(),
        ))
    })
}

/// `[...items, "x"]` / `items.filter(item => item !== "x")`, or the same over an updater's
/// `previous`; the spread source is kept until `admit` confirms it is the setter's own state.
pub(super) fn update(expression: &Expression<'_>, previous: Option<&str>) -> Option<StateUpdate> {
    let source = |expression: &Expression<'_>| match expression {
        Expression::Identifier(name) if previous == Some(name.name.as_str()) => Some(None),
        Expression::Identifier(name) if previous.is_none() => Some(Some(name.name.to_string())),
        _ => None,
    };
    let text = |expression: &Expression<'_>| match expression {
        Expression::StringLiteral(text) => Some(text.value.to_string()),
        _ => None,
    };
    match expression {
        Expression::ArrayExpression(array) => match array.elements.as_slice() {
            [ArrayExpressionElement::SpreadElement(spread), item] => Some(StateUpdate::Append {
                source: source(&spread.argument)?,
                item: text(item.as_expression()?)?,
            }),
            _ => None,
        },
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return None;
            };
            let [argument] = call.arguments.as_slice() else {
                return None;
            };
            let Some(Expression::ArrowFunctionExpression(arrow)) = argument.as_expression() else {
                return None;
            };
            let [parameter] = arrow.params.items.as_slice() else {
                return None;
            };
            let BindingPattern::BindingIdentifier(parameter) = &parameter.pattern else {
                return None;
            };
            let Some(Expression::BinaryExpression(test)) = arrow.body.as_expression() else {
                return None;
            };
            let is_parameter = |expression: &Expression<'_>| matches!(expression, Expression::Identifier(name) if name.name == parameter.name);
            let item = if is_parameter(&test.left) {
                text(&test.right)
            } else if is_parameter(&test.right) {
                text(&test.left)
            } else {
                None
            }?;
            (member.property.name == "filter"
                && !member.optional
                && !call.optional
                && call.type_arguments.is_none()
                && !arrow.r#async
                && arrow.params.rest.is_none()
                && matches!(
                    test.operator,
                    BinaryOperator::StrictInequality | BinaryOperator::Inequality
                ))
            .then_some(())?;
            Some(StateUpdate::Remove {
                source: source(&member.object)?,
                item,
            })
        }
        _ => None,
    }
}

fn events(node: &mut Node, setters: &[(String, String)]) {
    let convert = |value: &mut Value| {
        let Value::Event(event) = value else {
            return;
        };
        let Some((_, name)) = setters.iter().find(|(setter, _)| *setter == event.setter) else {
            return;
        };
        match &mut event.update {
            StateUpdate::Set(update) => {
                if let Some(update) = json(update) {
                    event.update = StateUpdate::Set(update);
                }
            }
            StateUpdate::Append { source, .. } | StateUpdate::Remove { source, .. }
                if source.as_ref() == Some(name) =>
            {
                *source = None;
            }
            _ => {}
        }
    };
    match &mut node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => events(&mut repeat.template, setters),
        NodeKind::Text(Value::NodeSlot(NodeSlot::Content(nodes))) => {
            nodes.iter_mut().for_each(|child| events(child, setters))
        }
        NodeKind::Text(value) => convert(value),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                convert(&mut attribute.value);
                if let Value::NodeSlot(NodeSlot::Content(nodes)) = &mut attribute.value {
                    nodes.iter_mut().for_each(|child| events(child, setters));
                }
            }
            children.iter_mut().for_each(|child| events(child, setters));
        }
    }
}

pub(in super::super) fn admit(root: &mut Node, states: &mut [StateBinding]) {
    let mut names = Vec::new();
    sources(root, &mut names);
    let mut setters = Vec::new();
    for state in states
        .iter_mut()
        .filter(|state| names.contains(&state.name))
    {
        if let Some(initial) = json(&state.initial) {
            state.initial = initial;
            setters.push((state.setter.clone(), state.name.clone()));
        }
    }
    events(root, &setters);
}
