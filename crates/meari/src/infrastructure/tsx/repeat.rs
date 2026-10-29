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
    parse_with_source(expression, value(&member.object)?)
}

fn parse_with_source(expression: &Expression<'_>, source: Value) -> Result<Repeat, Diagnostic> {
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
        source,
        item,
        key,
        key_path,
        template,
        source_span: location(&member.object),
        key_span,
    })
}

/// `["a", "b"].map(item => <li key={item}>{item}</li>)`: a literal string array is
/// expanded at compile time, so both renderers receive plain sibling nodes.
fn static_items(expression: &Expression<'_>) -> Option<Vec<String>> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::ArrayExpression(array) = &member.object else {
        return None;
    };
    array
        .elements
        .iter()
        .map(|item| match item.as_expression() {
            Some(Expression::StringLiteral(value)) => Some(value.value.to_string()),
            _ => None,
        })
        .collect()
}

fn expand(expression: &Expression<'_>, items: Vec<String>) -> Result<Vec<Node>, Diagnostic> {
    let Expression::CallExpression(call) = expression else {
        unreachable!()
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        unreachable!()
    };
    if items.len() > 1024 {
        return Err(err(expression, "static map exceeds 1024 items"));
    }
    // Reuse the keyed-template validation with a placeholder source.
    let repeat = parse_with_source(expression, Value::String(String::new()))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        if !repeat.key_path.is_empty() || !seen.insert(item.clone()) {
            return Err(err(
                &member.object,
                "static map requires unique string keys",
            ));
        }
        let mut node = repeat.template.clone();
        crate::domain::repeat::map_values(&mut node, &mut |value| {
            if matches!(value, Value::Local(name) if *name == repeat.item) {
                *value = Value::String(item.clone());
            }
        });
        nodes.push(node);
    }
    Ok(nodes)
}

pub(super) fn nodes(expression: &Expression<'_>) -> Result<Vec<Node>, Diagnostic> {
    if let Some(items) = static_items(expression) {
        return expand(expression, items);
    }
    Ok(vec![Node {
        kind: NodeKind::Text(Value::Repeat(Box::new(parse(expression)?))),
        span: location(expression),
    }])
}
