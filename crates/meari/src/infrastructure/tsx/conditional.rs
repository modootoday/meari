//! JSX `&&` conditional rendering lowered to a `hidden` predicate.
use super::{bool_expression, element, err};
use crate::domain::{Attribute, Diagnostic, Node, NodeKind, Value};
use oxc_ast::ast::*;

/// `{cond && <El/>}` lowers to `<El hidden={!cond}/>`: the node stays mounted and the
/// UA/preflight `[hidden]` rule removes it from layout in both renderers.
pub(super) fn node(expression: Option<&Expression<'_>>) -> Result<Option<Node>, Diagnostic> {
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
    let mut node = element(jsx)?;
    let NodeKind::Element { attributes, .. } = &mut node.kind else {
        unreachable!()
    };
    if attributes
        .iter()
        .any(|attribute| attribute.name == "hidden")
    {
        return Err(err(right, "conditional element cannot also set hidden"));
    }
    let span = condition.span;
    attributes.push(Attribute {
        name: "hidden".into(),
        value: Value::Predicate(Box::new(crate::domain::bool_expression::BoolExpression {
            kind: crate::domain::bool_expression::BoolExpressionKind::Not(Box::new(condition)),
            span,
        })),
    });
    Ok(Some(node))
}
