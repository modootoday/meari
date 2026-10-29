use super::{err, location};
use crate::domain::{
    Diagnostic, Node, NodeKind, Value,
    component_state::StateValue,
    state_program::{StateBinding, StateEvent, StateUpdate},
};
use oxc_ast::ast::*;
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};
pub(super) mod effect;
mod expression;
mod input;
mod structured;

fn captured(
    expression: &Expression<'_>,
    previous: Option<&str>,
) -> Result<StateUpdate, Diagnostic> {
    match self::expression::parse(expression, previous) {
        Ok(value) if previous.is_some() => Ok(StateUpdate::Reduce(value)),
        Ok(value) => Ok(StateUpdate::Capture(value)),
        Err(_) => {
            let value = structured::contextual(expression, previous)?;
            Ok(if previous.is_some() {
                StateUpdate::HookReduce(value)
            } else {
                StateUpdate::HookCapture(value)
            })
        }
    }
}

pub(super) fn literal(mut expr: &Expression<'_>) -> Result<StateValue, Diagnostic> {
    let mut wrappers = 0;
    while let Expression::ParenthesizedExpression(value) = expr {
        wrappers += 1;
        if wrappers > 64 {
            return Err(err(expr, "state literal wrapper budget exceeded"));
        }
        expr = &value.expression;
    }
    match expr {
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => {
            Ok(StateValue::Structured(Box::new(structured::parse(expr)?)))
        }
        Expression::StringLiteral(value) => Ok(StateValue::String(value.value.to_string())),
        Expression::BooleanLiteral(value) => Ok(StateValue::Bool(value.value)),
        Expression::NumericLiteral(value) => numeric(value.value, expr),
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryNegation => {
            if let Expression::NumericLiteral(value) = &unary.argument {
                numeric(-value.value, expr)
            } else {
                Err(err(expr, "numeric literal required after unary minus"))
            }
        }
        _ => Err(err(
            expr,
            "state initializer/update requires a string, boolean or finite numeric literal",
        )),
    }
}

fn numeric(value: f64, expr: &Expression<'_>) -> Result<StateValue, Diagnostic> {
    if !value.is_finite() {
        return Err(err(expr, "non-finite source state number"));
    }
    Ok(StateValue::Number(
        serde_json::Number::from_f64(value).ok_or_else(|| err(expr, "invalid numeric state"))?,
    ))
}

pub(super) fn declaration(statement: &Statement<'_>) -> Result<StateBinding, Diagnostic> {
    let Statement::VariableDeclaration(declaration) = statement else {
        return Err(err(statement, "state declaration required"));
    };
    if declaration.kind != VariableDeclarationKind::Const
        || declaration.declare
        || declaration.declarations.len() != 1
    {
        return Err(err(
            statement,
            "plain const [value, setter] = useState(initial) required",
        ));
    }
    let binding = &declaration.declarations[0];
    let BindingPattern::ArrayPattern(pattern) = &binding.id else {
        return Err(err(statement, "useState requires array destructuring"));
    };
    let [
        Some(BindingPattern::BindingIdentifier(name)),
        Some(BindingPattern::BindingIdentifier(setter)),
    ] = pattern.elements.as_slice()
    else {
        return Err(err(
            statement,
            "useState requires exactly value and setter identifiers",
        ));
    };
    let Some(Expression::CallExpression(call)) = &binding.init else {
        return Err(err(statement, "useState initializer call required"));
    };
    if pattern.rest.is_some()
        || binding.type_annotation.is_some()
        || binding.definite
        || call.optional
        || call.type_arguments.is_some()
        || !matches!(&call.callee, Expression::Identifier(name) if name.name == "useState")
        || call.arguments.len() != 1
    {
        return Err(err(
            statement,
            "plain useState with one literal initializer required",
        ));
    }
    let initial = literal(
        call.arguments[0]
            .as_expression()
            .ok_or_else(|| err(statement, "spread state initializer unsupported"))?,
    )?;
    Ok(StateBinding {
        name: name.name.to_string(),
        setter: setter.name.to_string(),
        initial,
        span: location(statement),
    })
}

pub(super) fn event(arrow: &ArrowFunctionExpression<'_>) -> Result<Value, Diagnostic> {
    if !arrow.params.items.is_empty() {
        return input::event(arrow);
    }
    if arrow.r#async
        || arrow.type_parameters.is_some()
        || arrow.return_type.is_some()
        || arrow.params.rest.is_some()
        || !arrow.params.items.is_empty()
    {
        return Err(err(
            arrow,
            "state onClick requires a synchronous zero-argument arrow",
        ));
    }
    let Some(Expression::CallExpression(call)) = arrow.body.as_expression() else {
        return Err(err(arrow, "state onClick requires one setter expression"));
    };
    Ok(Value::Event(setter_call(call)?))
}

pub(super) fn setter_call(call: &CallExpression<'_>) -> Result<StateEvent, Diagnostic> {
    let Expression::Identifier(setter) = &call.callee else {
        return Err(err(call, "named state setter required"));
    };
    if call.optional || call.type_arguments.is_some() || call.arguments.len() != 1 {
        return Err(err(call, "setter requires one update"));
    }
    let mut argument = call.arguments[0]
        .as_expression()
        .ok_or_else(|| err(call, "spread setter update unsupported"))?;
    while let Expression::ParenthesizedExpression(value) = argument {
        argument = &value.expression;
    }
    let update = if let Expression::ArrowFunctionExpression(updater) = argument {
        if updater.r#async
            || updater.type_parameters.is_some()
            || updater.return_type.is_some()
            || updater.params.rest.is_some()
            || updater.params.items.len() != 1
        {
            return Err(err(
                updater.as_ref(),
                "one plain updater parameter required",
            ));
        }
        let parameter = &updater.params.items[0];
        let BindingPattern::BindingIdentifier(previous) = &parameter.pattern else {
            return Err(err(parameter, "updater identifier required"));
        };
        if parameter.type_annotation.is_some()
            || parameter.initializer.is_some()
            || parameter.optional
            || !parameter.decorators.is_empty()
        {
            return Err(err(parameter, "plain updater identifier required"));
        }
        match updater.body.as_expression() {
            Some(Expression::UnaryExpression(value))
                if value.operator == UnaryOperator::LogicalNot
                    && matches!(&value.argument, Expression::Identifier(name) if name.name == previous.name) =>
            {
                StateUpdate::Toggle
            }
            Some(Expression::BinaryExpression(value))
                if value.operator == BinaryOperator::Addition
                    && matches!(&value.left, Expression::Identifier(name) if name.name == previous.name) =>
            {
                if let Ok(StateValue::Number(number)) = literal(&value.right) {
                    StateUpdate::Add(number)
                } else {
                    captured(
                        updater
                            .body
                            .as_expression()
                            .ok_or_else(|| err(updater.as_ref(), "updater expression required"))?,
                        Some(previous.name.as_str()),
                    )?
                }
            }
            _ => captured(
                updater
                    .body
                    .as_expression()
                    .ok_or_else(|| err(updater.as_ref(), "updater expression required"))?,
                Some(previous.name.as_str()),
            )?,
        }
    } else if matches!(
        argument,
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
    ) {
        let value = structured::contextual(argument, None)?;
        if value.validate_literal().is_ok() {
            StateUpdate::Set(StateValue::Structured(Box::new(value)))
        } else {
            StateUpdate::HookCapture(value)
        }
    } else {
        match literal(argument) {
            Ok(value) => StateUpdate::Set(value),
            Err(_) => captured(argument, None)?,
        }
    };
    Ok(StateEvent {
        setter: setter.name.to_string(),
        update,
    })
}

pub(super) fn resolve(root: &mut Node, states: &[StateBinding]) {
    let resolve = |value: &mut Value| {
        if let Value::Id(name) = value
            && states.iter().any(|binding| binding.name == *name)
        {
            *value = Value::State(name.clone());
        }
    };
    match &mut root.kind {
        NodeKind::Text(Value::Repeat(repeat)) => self::resolve(&mut repeat.template, states),
        NodeKind::Text(value) => {
            resolve(value);
            if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) = value {
                for child in nodes {
                    self::resolve(child, states);
                }
            }
        }
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                resolve(&mut attribute.value);
                if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) =
                    &mut attribute.value
                {
                    for child in nodes {
                        self::resolve(child, states);
                    }
                }
            }
            for child in children {
                self::resolve(child, states);
            }
        }
    }
}
