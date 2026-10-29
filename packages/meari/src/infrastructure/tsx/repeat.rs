use super::{element, err, location, value};
use crate::domain::{Diagnostic, Node, NodeKind, Value, repeat::Repeat};
use oxc_ast::ast::*;

pub(super) fn parse(expression: &Expression<'_>) -> Result<Repeat, Diagnostic> {
    let Expression::CallExpression(call) = expression else {
        return Err(err(expression, "repeat requires Array.map"));
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return Err(err(expression, "repeat requires Array.map"));
    };
    if call.optional
        || member.optional
        || member.property.name != "map"
        || call.arguments.len() != 1
        || call.type_arguments.is_some()
    {
        return Err(err(expression, "repeat requires one plain map callback"));
    }
    let Some(Expression::ArrowFunctionExpression(arrow)) = call.arguments[0].as_expression() else {
        return Err(err(expression, "map requires a plain arrow template"));
    };
    if arrow.r#async
        || arrow.type_parameters.is_some()
        || arrow.params.rest.is_some()
        || arrow.return_type.is_some()
    {
        return Err(err(expression, "map callback cannot be async or generic"));
    }
    let [parameter] = arrow.params.items.as_slice() else {
        return Err(err(expression, "map requires one lexical item binding"));
    };
    let BindingPattern::BindingIdentifier(binding) = &parameter.pattern else {
        return Err(err(expression, "map item must be an identifier"));
    };
    if parameter.type_annotation.is_some()
        || parameter.initializer.is_some()
        || parameter.optional
        || !parameter.decorators.is_empty()
    {
        return Err(err(expression, "plain map item binding required"));
    }
    let body = arrow
        .body
        .as_expression()
        .ok_or_else(|| err(expression, "map requires an expression template"))?;
    let body = match body {
        Expression::ParenthesizedExpression(inner) => &inner.expression,
        other => other,
    };
    let Expression::JSXElement(jsx) = body else {
        return Err(err(body, "map template requires one keyed element"));
    };
    let mut template = element(jsx)?;
    let NodeKind::Element { attributes, .. } = &mut template.kind else {
        unreachable!()
    };
    let position = attributes
        .iter()
        .position(|attribute| attribute.name == "key")
        .ok_or_else(|| err(jsx.as_ref(), "map template requires an explicit item key"))?;
    let key_attribute = attributes.remove(position);
    let (key, key_path) = match key_attribute.value {
        Value::Id(key) => (key, Vec::new()),
        Value::Member(member) => {
            let Value::Local(key) = *member.root else {
                return Err(err(jsx.as_ref(), "map key must belong to its lexical item"));
            };
            (key, member.path)
        }
        _ => {
            return Err(err(
                jsx.as_ref(),
                "map key requires its lexical item or record field",
            ));
        }
    };
    let item = binding.name.to_string();
    crate::domain::repeat::map_values(&mut template, &mut |value| {
        if matches!(value, Value::Id(name) if *name == item) {
            *value = Value::Local(item.clone());
        }
    });
    // Keep source/key diagnostics in the original TSX, before removing React's special key.
    let key_span = jsx.opening_element.attributes.iter().find_map(|attribute| match attribute {
        JSXAttributeItem::Attribute(attribute) if matches!(&attribute.name, JSXAttributeName::Identifier(name) if name.name == "key") => Some(location(attribute.as_ref())),
        _ => None,
    }).unwrap_or_else(|| location(jsx.as_ref()));
    Ok(Repeat {
        source: value(&member.object)?,
        item,
        key,
        key_path,
        template,
        source_span: location(&member.object),
        key_span,
    })
}

pub(super) fn node(expression: &Expression<'_>) -> Result<Node, Diagnostic> {
    Ok(Node {
        kind: NodeKind::Text(Value::Repeat(Box::new(parse(expression)?))),
        span: location(expression),
    })
}
