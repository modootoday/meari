//! Lower a callback handler expression without evaluating JavaScript.
use crate::domain::{
    Diagnostic, Span,
    host_callbacks::{CallbackHandler, CallbackSource},
};
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};

pub fn parse_handler(source: &str) -> Result<CallbackHandler, Diagnostic> {
    if source.len() > 1024 * 1024 {
        return Err(Diagnostic::unsupported(
            Span {
                start: 0,
                end: source.len(),
            },
            "callback source exceeds byte budget",
        ));
    }
    let allocator = Allocator::default();
    let expression = Parser::new(&allocator, source, SourceType::tsx())
        .parse_expression()
        .map_err(|errors| {
            Diagnostic::unsupported(
                Span {
                    start: 0,
                    end: source.len(),
                },
                format!("callback expression syntax: {errors:?}"),
            )
        })?;
    lower(&expression)
}

pub(crate) fn lower(expression: &Expression<'_>) -> Result<CallbackHandler, Diagnostic> {
    let error = |message| {
        Diagnostic::unsupported(
            Span {
                start: expression.span().start as usize,
                end: expression.span().end as usize,
            },
            message,
        )
    };
    let Expression::ArrowFunctionExpression(arrow) = expression else {
        return Err(error("callback requires an explicit arrow invocation"));
    };
    if arrow.r#async
        || arrow.type_parameters.is_some()
        || arrow.return_type.is_some()
        || arrow.params.rest.is_some()
        || arrow.params.items.len() > 1
    {
        return Err(error("callback requires a synchronous plain handler"));
    }
    let Some(Expression::CallExpression(call)) = arrow.body.as_expression() else {
        return Err(error(
            "callback handler requires exactly one call expression",
        ));
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return Err(error("callback must invoke its declared props member"));
    };
    if call.optional
        || call.type_arguments.is_some()
        || member.optional
        || !matches!(&member.object, Expression::Identifier(name) if name.name == "props")
    {
        return Err(error(
            "callback callee must be props.name without optional execution",
        ));
    }
    let mut capture_prop = None;
    let event_source = if arrow.params.items.is_empty() && call.arguments.is_empty() {
        CallbackSource::Action
    } else if arrow.params.items.is_empty() && call.arguments.len() == 1 {
        let Some(Expression::StaticMemberExpression(argument)) = call.arguments[0].as_expression()
        else {
            return Err(error(
                "callback capture requires one explicit props string member",
            ));
        };
        if argument.optional
            || !matches!(&argument.object,Expression::Identifier(name) if name.name == "props")
        {
            return Err(error(
                "callback capture must reference props.name without optional execution",
            ));
        }
        capture_prop = Some(argument.property.name.to_string());
        CallbackSource::CapturedText
    } else {
        let [parameter] = arrow.params.items.as_slice() else {
            return Err(error("text callback requires one event parameter"));
        };
        let BindingPattern::BindingIdentifier(event) = &parameter.pattern else {
            return Err(error("callback event parameter must be a plain identifier"));
        };
        if event.name == "props"
            || parameter.type_annotation.is_some()
            || parameter.initializer.is_some()
            || parameter.optional
            || !parameter.decorators.is_empty()
            || call.arguments.len() != 1
        {
            return Err(error(
                "callback requires one unshadowed event parameter and argument",
            ));
        }
        if matches!(call.arguments[0].as_expression(), Some(Expression::Identifier(name)) if name.name == event.name)
        {
            CallbackSource::BooleanValue
        } else {
            let Some(Expression::StaticMemberExpression(field)) = call.arguments[0].as_expression()
            else {
                return Err(error(
                    "callback argument requires its own boolean value or event.currentTarget.value",
                ));
            };
            let Expression::StaticMemberExpression(target) = &field.object else {
                return Err(error("text callback requires its own currentTarget"));
            };
            if field.optional
                || target.optional
                || field.property.name != "value"
                || target.property.name != "currentTarget"
                || !matches!(&target.object, Expression::Identifier(name) if name.name == event.name)
            {
                return Err(error(
                    "text callback argument must be its own event.currentTarget.value",
                ));
            }
            CallbackSource::CurrentTargetText
        }
    };
    Ok(CallbackHandler {
        prop: member.property.name.to_string(),
        source: event_source,
        span: Span {
            start: expression.span().start as usize,
            end: expression.span().end as usize,
        },
        capture_prop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lowers_original_action_and_controlled_text_without_evaluation() {
        let action = parse_handler("() => props.onSave()").unwrap();
        assert_eq!(action.prop, "onSave");
        assert!(action.invoke(None).is_ok());
        assert!(action.invoke(Some("unexpected")).is_err());
        let text =
            parse_handler("event => props.onDraftChange(event.currentTarget.value)").unwrap();
        assert_eq!(text.prop, "onDraftChange");
        assert_eq!(text.source, CallbackSource::CurrentTargetText);
        assert!(text.invoke(Some("edited draft")).is_ok());
        for source in [
            "async () => props.onSave()",
            "props => props.onDraftChange(props.currentTarget.value)",
            "event => props.onDraftChange(other.currentTarget.value)",
            "() => global.onSave()",
            "() => { props.onSave(); props.onAccept(); }",
        ] {
            assert!(parse_handler(source).is_err(), "{source}");
        }
    }
}
