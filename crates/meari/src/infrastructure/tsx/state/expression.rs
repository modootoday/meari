use super::super::err;
use crate::domain::{
    Diagnostic,
    state_expression::{Binary, StateExpression as E, Unary},
};
use oxc_ast::ast::Expression;
use oxc_syntax::operator::{BinaryOperator as B, LogicalOperator as L, UnaryOperator as U};

pub(super) fn parse(expression: &Expression<'_>, previous: Option<&str>) -> Result<E, Diagnostic> {
    parse_at(expression, previous, 0)
}

fn parse_at(
    expression: &Expression<'_>,
    previous: Option<&str>,
    depth: u32,
) -> Result<E, Diagnostic> {
    if depth > 64 {
        return Err(err(expression, "state expression depth exceeds 64"));
    }
    let parse = |value: &Expression<'_>| parse_at(value, previous, depth + 1).map(Box::new);
    if let Ok(value) = super::literal(expression) {
        return Ok(E::Literal { value });
    }
    Ok(match expression {
        Expression::Identifier(name) if previous == Some(name.name.as_str()) => E::Previous,
        Expression::Identifier(name) => E::State {
            name: name.name.to_string(),
        },
        Expression::ParenthesizedExpression(value) => {
            return parse_at(&value.expression, previous, depth + 1);
        }
        Expression::UnaryExpression(value) => E::Unary {
            operator: match value.operator {
                U::LogicalNot => Unary::Not,
                U::UnaryNegation => Unary::Negate,
                _ => return Err(err(expression, "unsupported state unary operator")),
            },
            argument: parse(&value.argument)?,
        },
        Expression::BinaryExpression(value) => E::Binary {
            operator: match value.operator {
                B::Addition => Binary::Add,
                B::Subtraction => Binary::Subtract,
                B::Multiplication => Binary::Multiply,
                B::Division => Binary::Divide,
                B::Remainder => Binary::Remainder,
                B::StrictEquality => Binary::Equal,
                B::StrictInequality => Binary::NotEqual,
                B::LessThan => Binary::Less,
                B::LessEqualThan => Binary::LessEqual,
                B::GreaterThan => Binary::Greater,
                B::GreaterEqualThan => Binary::GreaterEqual,
                _ => return Err(err(expression, "unsupported state binary operator")),
            },
            left: parse(&value.left)?,
            right: parse(&value.right)?,
        },
        Expression::LogicalExpression(value) => E::Binary {
            operator: match value.operator {
                L::And => Binary::And,
                L::Or => Binary::Or,
                _ => {
                    return Err(err(
                        expression,
                        "nullable state expression requires union/member IR",
                    ));
                }
            },
            left: parse(&value.left)?,
            right: parse(&value.right)?,
        },
        Expression::ConditionalExpression(value) => E::Conditional {
            test: parse(&value.test)?,
            consequent: parse(&value.consequent)?,
            alternate: parse(&value.alternate)?,
        },
        _ => {
            return Err(err(
                expression,
                "state expression call/member/collection is not implemented",
            ));
        }
    })
}
