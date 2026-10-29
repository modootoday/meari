use super::super::{err, location};
use crate::domain::{
    Diagnostic,
    hook_expression::{Entry, Expression as Ir, Kind},
};
use oxc_ast::ast::*;
use oxc_syntax::operator::{BinaryOperator, UnaryOperator};

pub(super) fn parse(expression: &Expression<'_>) -> Result<Ir, Diagnostic> {
    let result = contextual(expression, None)?;
    result.validate_literal()?;
    Ok(result)
}

pub(super) fn contextual(
    expression: &Expression<'_>,
    previous: Option<&str>,
) -> Result<Ir, Diagnostic> {
    fn visit(
        expression: &Expression<'_>,
        previous: Option<&str>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<Ir, Diagnostic> {
        *nodes += 1;
        if depth > 64 || *nodes > 16_384 {
            return Err(err(
                expression,
                "structured source expression node/depth budget exceeded",
            ));
        }
        let kind = match expression {
            Expression::Identifier(value) if previous == Some(value.name.as_str()) => {
                Kind::Previous
            }
            Expression::Identifier(value) => Kind::State(value.name.to_string()),
            Expression::NullLiteral(_) => Kind::Null,
            Expression::BooleanLiteral(value) => Kind::Boolean(value.value),
            Expression::NumericLiteral(value) => Kind::Number(value.value.to_bits()),
            Expression::StringLiteral(value) => {
                if value.lone_surrogates {
                    return Err(err(
                        value.as_ref(),
                        "lone UTF-16 surrogates require an explicit string adapter",
                    ));
                }
                Kind::Text(value.value.to_string())
            }
            Expression::ParenthesizedExpression(value) => {
                return visit(&value.expression, previous, depth + 1, nodes);
            }
            Expression::StaticMemberExpression(value) if !value.optional => Kind::Field {
                target: Box::new(visit(&value.object, previous, depth + 1, nodes)?),
                key: value.property.name.to_string(),
            },
            Expression::ComputedMemberExpression(value) if !value.optional => {
                let target = Box::new(visit(&value.object, previous, depth + 1, nodes)?);
                match &value.expression {
                    Expression::StringLiteral(key) if !key.lone_surrogates => Kind::Field {
                        target,
                        key: key.value.to_string(),
                    },
                    Expression::NumericLiteral(index)
                        if index.value.is_finite()
                            && index.value >= 0.0
                            && index.value.fract() == 0.0
                            && index.value <= 4_294_967_294.0 =>
                    {
                        Kind::Element {
                            target,
                            index: index.value as usize,
                        }
                    }
                    _ => {
                        return Err(err(
                            value.as_ref(),
                            "hook member requires a literal string key or portable array index",
                        ));
                    }
                }
            }
            Expression::UnaryExpression(value) if value.operator == UnaryOperator::LogicalNot => {
                Kind::Not(Box::new(visit(
                    &value.argument,
                    previous,
                    depth + 1,
                    nodes,
                )?))
            }
            Expression::BinaryExpression(value) if value.operator == BinaryOperator::Addition => {
                Kind::Add {
                    left: Box::new(visit(&value.left, previous, depth + 1, nodes)?),
                    right: Box::new(visit(&value.right, previous, depth + 1, nodes)?),
                }
            }
            Expression::UnaryExpression(value)
                if value.operator == UnaryOperator::UnaryNegation =>
            {
                let Expression::NumericLiteral(number) = &value.argument else {
                    return Err(err(
                        expression,
                        "negative structured number literal required",
                    ));
                };
                Kind::Number((-number.value).to_bits())
            }
            Expression::ArrayExpression(value) => {
                let mut items = Vec::new();
                for item in &value.elements {
                    let item = item.as_expression().ok_or_else(|| {
                        err(
                            item,
                            "sparse/spread arrays require an explicit source adapter",
                        )
                    })?;
                    items.push(visit(item, previous, depth + 1, nodes)?);
                }
                Kind::Array(items)
            }
            Expression::ObjectExpression(value) => {
                let mut entries = Vec::new();
                for property in &value.properties {
                    let ObjectPropertyKind::ObjectProperty(property) = property else {
                        let ObjectPropertyKind::SpreadProperty(spread) = property else {
                            unreachable!()
                        };
                        entries.push(Entry::Spread {
                            value: visit(&spread.argument, previous, depth + 1, nodes)?,
                        });
                        continue;
                    };
                    if property.kind != PropertyKind::Init
                        || property.method
                        || property.computed
                        || property.shorthand
                    {
                        return Err(err(
                            property.as_ref(),
                            "plain structured data property required",
                        ));
                    }
                    let key = match &property.key {
                        PropertyKey::StaticIdentifier(value) => value.name.to_string(),
                        PropertyKey::StringLiteral(value) => {
                            if value.lone_surrogates {
                                return Err(err(
                                    value.as_ref(),
                                    "lone UTF-16 surrogates require an explicit string adapter",
                                ));
                            }
                            value.value.to_string()
                        }
                        _ => {
                            return Err(err(
                                property.as_ref(),
                                "literal structured property key required",
                            ));
                        }
                    };
                    entries.push(Entry::Field {
                        key,
                        value: visit(&property.value, previous, depth + 1, nodes)?,
                    });
                }
                Kind::Record(entries)
            }
            _ => {
                return Err(err(
                    expression,
                    "structured literal requires plain data, not executable values",
                ));
            }
        };
        Ok(Ir {
            span: location(expression),
            kind,
        })
    }
    let result = visit(expression, previous, 0, &mut 0)?;
    result.validate()?;
    Ok(result)
}
