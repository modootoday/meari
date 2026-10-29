use super::{StateEvent, StateUpdate};
use crate::domain::{
    Diagnostic, Span,
    component_state::StateValue,
    hook_expression::{Expression, Kind},
    state_expression::{Binary, StateExpression, Unary},
};

pub(crate) fn literal(value: &StateValue, span: Span) -> Result<Expression, Diagnostic> {
    let kind = match value {
        StateValue::Structured(expression) => {
            expression.validate_literal()?;
            return Ok(expression.as_ref().clone());
        }
        StateValue::Bool(value) => Kind::Boolean(*value),
        StateValue::String(value) => Kind::Text(value.clone()),
        StateValue::Number(value) => {
            super::render_value(&StateValue::Number(value.clone()))
                .map_err(|message| Diagnostic::unsupported(span, message))?;
            Kind::Number(
                value
                    .as_f64()
                    .ok_or_else(|| Diagnostic::unsupported(span, "numeric hook value unavailable"))?
                    .to_bits(),
            )
        }
        StateValue::Json(_) => {
            return Err(Diagnostic::unsupported(
                span,
                "JSON snapshots are not live structured hook roots",
            ));
        }
    };
    Ok(Expression { span, kind })
}

fn legacy(value: &StateExpression, span: Span, depth: usize) -> Result<Expression, Diagnostic> {
    if depth > 64 {
        return Err(Diagnostic::unsupported(
            span,
            "hook updater depth budget exceeded",
        ));
    }
    let child = |value| legacy(value, span, depth + 1).map(Box::new);
    Ok(Expression {
        span,
        kind: match value {
            StateExpression::Literal { value } => return literal(value, span),
            StateExpression::State { name } => Kind::State(name.clone()),
            StateExpression::Previous => Kind::Previous,
            StateExpression::Unary {
                operator: Unary::Not,
                argument,
            } => Kind::Not(child(argument)?),
            StateExpression::Binary {
                operator: Binary::Add,
                left,
                right,
            } => Kind::Add {
                left: child(left)?,
                right: child(right)?,
            },
            _ => {
                return Err(Diagnostic::unsupported(
                    span,
                    "hook operator requires an explicit expression adapter",
                ));
            }
        },
    })
}

impl StateEvent {
    /// Normalize authoring updates without copying any live value or evaluating a closure.
    pub(crate) fn hook_expression(&self, span: Span) -> Result<Expression, Diagnostic> {
        let previous = || Expression {
            span,
            kind: Kind::Previous,
        };
        Ok(match &self.update {
            StateUpdate::Set(value) => literal(value, span)?,
            StateUpdate::Toggle => Expression {
                span,
                kind: Kind::Not(Box::new(previous())),
            },
            StateUpdate::Add(value) => Expression {
                span,
                kind: Kind::Add {
                    left: Box::new(previous()),
                    right: Box::new(literal(&StateValue::Number(value.clone()), span)?),
                },
            },
            StateUpdate::Capture(value) | StateUpdate::Reduce(value) => legacy(value, span, 0)?,
            StateUpdate::HookCapture(value) | StateUpdate::HookReduce(value) => value.clone(),
        })
    }
}
