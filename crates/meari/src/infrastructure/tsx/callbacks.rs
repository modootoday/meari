use super::err;
use crate::domain::{Diagnostic, host_callbacks::CallbackSignature};
use oxc_ast::ast::{BindingPattern, TSFunctionType, TSType};

pub(super) fn signature(function: &TSFunctionType<'_>) -> Result<CallbackSignature, Diagnostic> {
    if function.type_parameters.is_some()
        || function.this_param.is_some()
        || function.params.rest.is_some()
        || !matches!(
            function.return_type.type_annotation,
            TSType::TSVoidKeyword(_)
        )
    {
        return Err(err(
            function,
            "callback requires a plain void-returning signature",
        ));
    }
    if function.params.items.is_empty() {
        return Ok(CallbackSignature::Action);
    }
    let [parameter] = function.params.items.as_slice() else {
        return Err(err(
            function,
            "callback supports zero arguments or one string/boolean argument",
        ));
    };
    if !matches!(&parameter.pattern, BindingPattern::BindingIdentifier(_))
        || parameter.optional
        || parameter.accessibility.is_some()
        || parameter.readonly
        || parameter.r#override
        || parameter.initializer.is_some()
        || !parameter.decorators.is_empty()
        || !parameter
            .type_annotation
            .as_ref()
            .is_some_and(|annotation| {
                matches!(
                    annotation.type_annotation,
                    TSType::TSStringKeyword(_) | TSType::TSBooleanKeyword(_)
                )
            })
    {
        return Err(err(
            parameter,
            "callback requires one required plain string/boolean parameter",
        ));
    }
    Ok(
        if parameter
            .type_annotation
            .as_ref()
            .is_some_and(|annotation| {
                matches!(annotation.type_annotation, TSType::TSBooleanKeyword(_))
            })
        {
            CallbackSignature::Boolean
        } else {
            CallbackSignature::Text
        },
    )
}
