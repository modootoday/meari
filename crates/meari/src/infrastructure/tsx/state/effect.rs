//! Mount-only effects: `useEffect(() => { setX(literal); }, [])`.
//!
//! React runs such an effect once after the first commit and re-renders with the updated
//! state. Meari renders committed frames, so the effect's literal setter updates are folded
//! into the state's mounted value in source order. Anything that could observe time, props,
//! the DOM or cleanup (dependencies, returns, expressions, async) is rejected rather than
//! approximated.
use super::super::err;
use crate::domain::{
    Diagnostic,
    component_state::StateValue,
    state_program::{StateBinding, StateUpdate},
};
use oxc_ast::ast::*;

pub(in super::super) fn is_effect(statement: &Statement<'_>) -> bool {
    matches!(statement, Statement::ExpressionStatement(expression)
        if matches!(&expression.expression, Expression::CallExpression(call)
            if matches!(&call.callee, Expression::Identifier(name) if name.name == "useEffect")))
}

pub(in super::super) fn apply(
    statement: &Statement<'_>,
    states: &mut [StateBinding],
) -> Result<(), Diagnostic> {
    let Statement::ExpressionStatement(expression) = statement else {
        return Err(err(statement, "useEffect statement required"));
    };
    let Expression::CallExpression(call) = &expression.expression else {
        return Err(err(statement, "useEffect call required"));
    };
    let [effect, dependencies] = call.arguments.as_slice() else {
        return Err(err(call.as_ref(), "useEffect requires a callback and []"));
    };
    let Some(Expression::ArrayExpression(dependencies)) = dependencies.as_expression() else {
        return Err(err(
            call.as_ref(),
            "only mount-only useEffect(..., []) supported",
        ));
    };
    if !dependencies.elements.is_empty() || call.optional || call.type_arguments.is_some() {
        return Err(err(
            call.as_ref(),
            "only mount-only useEffect(..., []) supported",
        ));
    }
    let Some(Expression::ArrowFunctionExpression(arrow)) = effect.as_expression() else {
        return Err(err(
            call.as_ref(),
            "useEffect callback must be an arrow function",
        ));
    };
    if arrow.r#async
        || arrow.type_parameters.is_some()
        || arrow.return_type.is_some()
        || arrow.params.rest.is_some()
        || !arrow.params.items.is_empty()
    {
        return Err(err(
            arrow.as_ref(),
            "plain synchronous zero-argument effect required",
        ));
    }
    let ArrowFunctionBody::FunctionBody(body) = &arrow.body else {
        return Err(err(
            arrow.as_ref(),
            "effect requires a block body of setter calls",
        ));
    };
    if !body.directives.is_empty() || body.statements.is_empty() {
        return Err(err(
            arrow.as_ref(),
            "effect requires a non-empty block of setter calls",
        ));
    }
    for statement in &body.statements {
        let Statement::ExpressionStatement(expression) = statement else {
            return Err(err(
                statement,
                "effect body supports only state setter calls",
            ));
        };
        let Expression::CallExpression(setter) = &expression.expression else {
            return Err(err(
                statement,
                "effect body supports only state setter calls",
            ));
        };
        let event = super::setter_call(setter)?;
        let binding = states
            .iter_mut()
            .find(|binding| binding.setter == event.setter)
            .ok_or_else(|| err(statement, "effect calls an unknown state setter"))?;
        binding.initial = match (&binding.initial, event.update) {
            (current, StateUpdate::Set(value)) if value.kind() == current.kind() => value,
            (StateValue::Bool(value), StateUpdate::Toggle) => StateValue::Bool(!value),
            (StateValue::Number(value), StateUpdate::Add(delta)) => {
                let sum = value.as_f64().unwrap_or(f64::NAN) + delta.as_f64().unwrap_or(f64::NAN);
                StateValue::Number(
                    serde_json::Number::from_f64(sum)
                        .filter(|_| sum.is_finite())
                        .ok_or_else(|| err(statement, "non-finite effect state number"))?,
                )
            }
            _ => {
                return Err(err(
                    statement,
                    "effect setter requires a literal, toggle or numeric add of the same type",
                ));
            }
        };
    }
    Ok(())
}
