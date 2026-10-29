//! JSX `&&` and `? :` conditional rendering lowered to `hidden` predicates.
use super::{bool_expression, element, err};
use crate::domain::{Attribute, Diagnostic, Node, NodeKind, Value};
use oxc_ast::ast::*;

/// `{cond && <El/>}` lowers to `<El hidden={!cond}/>`: the node stays mounted and the
/// UA/preflight `[hidden]` rule removes it from layout in both renderers.
/// `{cond ? <A/> : <B/>}` lowers to `<A hidden={!cond}/><B hidden={!!cond}/>`.
pub(super) fn nodes(expression: Option<&Expression<'_>>) -> Result<Option<Vec<Node>>, Diagnostic> {
    match expression {
        Some(Expression::ConditionalExpression(ternary)) => {
            let branch = |value: &'_ Expression<'_>| match unwrap(value) {
                Expression::JSXElement(_) => Ok(()),
                other => Err(err(
                    other,
                    "ternary rendering supports `cond ? <A/> : <B/>` only",
                )),
            };
            branch(&ternary.consequent)?;
            branch(&ternary.alternate)?;
            let condition = bool_expression::parse(&ternary.test)?;
            let span = condition.span;
            let negated = crate::domain::bool_expression::BoolExpression {
                kind: crate::domain::bool_expression::BoolExpressionKind::Not(Box::new(
                    condition.clone(),
                )),
                span,
            };
            let Expression::JSXElement(when) = unwrap(&ternary.consequent) else {
                unreachable!()
            };
            let Expression::JSXElement(otherwise) = unwrap(&ternary.alternate) else {
                unreachable!()
            };
            Ok(Some(vec![
                hide(element(when)?, condition, unwrap(&ternary.consequent))?,
                hide(element(otherwise)?, negated, unwrap(&ternary.alternate))?,
            ]))
        }
        other => Ok(node(other)?.map(|node| vec![node])),
    }
}

fn unwrap<'a>(value: &'a Expression<'a>) -> &'a Expression<'a> {
    match value {
        Expression::ParenthesizedExpression(inner) => &inner.expression,
        other => other,
    }
}

/// Adds `hidden={!shown}` to an element.
fn hide(
    mut node: Node,
    shown: crate::domain::bool_expression::BoolExpression,
    source: &Expression<'_>,
) -> Result<Node, Diagnostic> {
    let NodeKind::Element { attributes, .. } = &mut node.kind else {
        unreachable!()
    };
    if attributes
        .iter()
        .any(|attribute| attribute.name == "hidden")
    {
        return Err(err(source, "conditional element cannot also set hidden"));
    }
    let span = shown.span;
    attributes.push(Attribute {
        name: "hidden".into(),
        value: Value::Predicate(Box::new(crate::domain::bool_expression::BoolExpression {
            kind: crate::domain::bool_expression::BoolExpressionKind::Not(Box::new(shown)),
            span,
        })),
    });
    Ok(node)
}

fn node(expression: Option<&Expression<'_>>) -> Result<Option<Node>, Diagnostic> {
    let Some(Expression::LogicalExpression(logical)) = expression else {
        return Ok(None);
    };
    let right = match &logical.right {
        Expression::ParenthesizedExpression(inner) => &inner.expression,
        other => other,
    };
    let Expression::JSXElement(jsx) = right else {
        return Ok(None);
    };
    if logical.operator != oxc_syntax::operator::LogicalOperator::And {
        return Err(err(
            right,
            "conditional rendering supports `cond && <Element/>` only",
        ));
    }
    let condition = bool_expression::parse(&logical.left)?;
    Ok(Some(hide(element(jsx)?, condition, right)?))
}
