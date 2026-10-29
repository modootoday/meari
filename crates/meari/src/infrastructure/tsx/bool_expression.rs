//! Prepared lowering for typed boolean props; unrelated JavaScript remains diagnostic.
use crate::domain::{
    Diagnostic,
    bool_expression::{
        BoolExpression, BoolExpressionKind as Kind, ScalarExpression,
        ScalarExpressionKind as Scalar,
    },
};
use oxc_ast::ast::Expression;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

pub(super) fn parse(expression: &Expression<'_>) -> Result<BoolExpression, Diagnostic> {
    fn visit(
        expression: &Expression<'_>,
        depth: usize,
        steps: &mut usize,
    ) -> Result<BoolExpression, Diagnostic> {
        *steps += if matches!(expression, Expression::BinaryExpression(_)) {
            3
        } else {
            1
        };
        if depth > 64 || *steps > 8192 {
            return Err(super::err(
                expression,
                "boolean expression exceeds depth/node budget",
            ));
        }
        let kind = match expression {
            Expression::BooleanLiteral(value) => Kind::Literal(value.value),
            Expression::Identifier(value) => Kind::State(value.name.to_string()),
            Expression::StaticMemberExpression(member)
                if !member.optional
                    && matches!(&member.object, Expression::Identifier(name) if name.name == "props") =>
            {
                Kind::Prop(member.property.name.to_string())
            }
            Expression::ParenthesizedExpression(value) => {
                return visit(&value.expression, depth + 1, steps);
            }
            Expression::StaticMemberExpression(_) => match super::value(expression)? {
                crate::domain::Value::Member(member) => Kind::Member(member),
                _ => return Err(super::err(expression, "record predicate member required")),
            },
            Expression::UnaryExpression(value) if value.operator == UnaryOperator::LogicalNot => {
                Kind::Not(Box::new(visit(&value.argument, depth + 1, steps)?))
            }
            Expression::LogicalExpression(value) => {
                let left = Box::new(visit(&value.left, depth + 1, steps)?);
                let right = Box::new(visit(&value.right, depth + 1, steps)?);
                match value.operator {
                    LogicalOperator::And => Kind::And(left, right),
                    LogicalOperator::Or => Kind::Or(left, right),
                    _ => {
                        return Err(super::err(
                            expression,
                            "nullish predicate requires an explicit nullable type",
                        ));
                    }
                }
            }
            Expression::BinaryExpression(value) => {
                if depth >= 64 {
                    return Err(super::err(
                        expression,
                        "boolean expression exceeds depth/node budget",
                    ));
                }
                let left = scalar(&value.left)?;
                let right = scalar(&value.right)?;
                match value.operator {
                    BinaryOperator::StrictEquality => Kind::Equal(left, right),
                    BinaryOperator::StrictInequality => Kind::NotEqual(left, right),
                    _ => {
                        return Err(super::err(
                            expression,
                            "typed scalar comparison requires === or !== without coercion",
                        ));
                    }
                }
            }
            _ => {
                return Err(super::err(
                    expression,
                    "predicate supports boolean literals, props members, !, && and ||",
                ));
            }
        };
        Ok(BoolExpression {
            kind,
            span: super::location(expression),
        })
    }
    visit(expression, 0, &mut 0)
}

fn scalar(expression: &Expression<'_>) -> Result<ScalarExpression, Diagnostic> {
    let kind = match expression {
        Expression::StringLiteral(value) => Scalar::String(value.value.to_string()),
        Expression::BooleanLiteral(value) => Scalar::Bool(value.value),
        Expression::StaticMemberExpression(member)
            if !member.optional
                && matches!(&member.object, Expression::Identifier(name) if name.name == "props") =>
        {
            Scalar::Prop(member.property.name.to_string())
        }
        Expression::StaticMemberExpression(_) => match super::value(expression)? {
            crate::domain::Value::Member(member) => Scalar::Member(member),
            _ => return Err(super::err(expression, "record comparison member required")),
        },
        _ => {
            return Err(super::err(
                expression,
                "comparison operand requires a string/boolean literal or plain props member",
            ));
        }
    };
    Ok(ScalarExpression {
        kind,
        span: super::location(expression),
    })
}
