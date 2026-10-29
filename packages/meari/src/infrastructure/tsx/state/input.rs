use super::super::err;
use crate::domain::{
    Diagnostic, Value,
    input_event::{InputKind, InputStateBinding},
};
use oxc_ast::ast::{ArrowFunctionExpression, BindingPattern, Expression};

pub(super) fn event(arrow: &ArrowFunctionExpression<'_>) -> Result<Value, Diagnostic> {
    if arrow.r#async
        || arrow.type_parameters.is_some()
        || arrow.return_type.is_some()
        || arrow.params.rest.is_some()
        || arrow.params.items.len() != 1
    {
        return Err(err(
            arrow,
            "input handler requires one synchronous plain parameter",
        ));
    }
    let parameter = &arrow.params.items[0];
    let BindingPattern::BindingIdentifier(name) = &parameter.pattern else {
        return Err(err(
            parameter,
            "input event parameter must be an identifier",
        ));
    };
    if parameter.type_annotation.is_some()
        || parameter.initializer.is_some()
        || parameter.optional
        || !parameter.decorators.is_empty()
    {
        return Err(err(
            parameter,
            "input event parameter modifiers require an explicit type contract",
        ));
    }
    let Some(Expression::CallExpression(call)) = arrow.body.as_expression() else {
        return Err(err(
            arrow,
            "input handler requires one direct setter expression",
        ));
    };
    let Expression::Identifier(setter) = &call.callee else {
        return Err(err(call.as_ref(), "named input state setter required"));
    };
    if setter.name == name.name
        || call.optional
        || call.type_arguments.is_some()
        || call.arguments.len() != 1
    {
        return Err(err(
            call.as_ref(),
            "input setter must be unshadowed and take one payload",
        ));
    }
    let Some(Expression::StaticMemberExpression(field)) = call.arguments[0].as_expression() else {
        return Err(err(
            call.as_ref(),
            "input setter requires event.currentTarget.value or checked",
        ));
    };
    let Expression::StaticMemberExpression(target) = &field.object else {
        return Err(err(
            field.as_ref(),
            "input payload requires an explicit event target",
        ));
    };
    if field.optional
        || target.optional
        || !["target", "currentTarget"].contains(&target.property.name.as_str())
        || !matches!(&target.object, Expression::Identifier(event) if event.name == name.name)
    {
        return Err(err(
            field.as_ref(),
            "input payload must come from its own event target",
        ));
    }
    let kind = match field.property.name.as_str() {
        "value" => InputKind::Text,
        "checked" => InputKind::Checked,
        _ => return Err(err(field.as_ref(), "unsupported typed input payload field")),
    };
    Ok(Value::Input(InputStateBinding {
        setter: setter.name.to_string(),
        kind,
    }))
}
