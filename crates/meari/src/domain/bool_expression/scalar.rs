//! Pure scalar operands for typed source comparisons, without JavaScript coercion.
use crate::domain::{Diagnostic, Prop, PropType, Span, Value};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalarExpression {
    pub kind: ScalarExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalarExpressionKind {
    String(String),
    Bool(bool),
    Prop(String),
    Member(crate::domain::member_value::MemberValue),
}

#[derive(PartialEq, Eq)]
pub(super) enum ScalarValue<'a> {
    String(Cow<'a, str>),
    Bool(bool),
}

impl ScalarExpression {
    pub(super) fn validate(&self, props: &[Prop]) -> Result<PropType, Diagnostic> {
        match &self.kind {
            ScalarExpressionKind::String(value) if value.len() <= 1024 * 1024 => {
                Ok(PropType::String)
            }
            ScalarExpressionKind::Bool(_) => Ok(PropType::Bool),
            ScalarExpressionKind::Member(member) => {
                let kind = super::member::kind(member, props, self.span)?;
                if matches!(kind, PropType::Bool | PropType::String) {
                    Ok(kind)
                } else {
                    Err(Diagnostic::unsupported(
                        self.span,
                        "comparison requires a string or boolean record field",
                    ))
                }
            }
            ScalarExpressionKind::Prop(name) => props
                .iter()
                .find(|prop| {
                    prop.name == *name && matches!(prop.kind, PropType::String | PropType::Bool)
                })
                .map(|prop| prop.kind.clone())
                .ok_or_else(|| {
                    Diagnostic::unsupported(
                        self.span,
                        "comparison requires a declared string or boolean prop",
                    )
                }),
            _ => Err(Diagnostic::unsupported(
                self.span,
                "comparison string exceeds byte budget",
            )),
        }
    }

    pub(super) fn evaluate<'a>(
        &'a self,
        props: &'a BTreeMap<String, Value>,
    ) -> Result<ScalarValue<'a>, Diagnostic> {
        let value = match &self.kind {
            ScalarExpressionKind::String(value) => ScalarValue::String(Cow::Borrowed(value)),
            ScalarExpressionKind::Bool(value) => ScalarValue::Bool(*value),
            ScalarExpressionKind::Prop(name) => match props.get(name) {
                Some(Value::String(value)) => ScalarValue::String(Cow::Borrowed(value)),
                Some(Value::Bool(value)) => ScalarValue::Bool(*value),
                _ => {
                    return Err(Diagnostic::unsupported(
                        self.span,
                        "scalar prop binding is missing or has a different type",
                    ));
                }
            },
            ScalarExpressionKind::Member(member) => {
                match super::member::bound(member, props, self.span)? {
                    Value::String(value) => ScalarValue::String(Cow::Owned(value)),
                    Value::Bool(value) => ScalarValue::Bool(value),
                    _ => {
                        return Err(Diagnostic::unsupported(
                            self.span,
                            "comparison requires a string or boolean record field",
                        ));
                    }
                }
            }
        };
        if matches!(&value, ScalarValue::String(value) if value.len() > 1024 * 1024) {
            return Err(Diagnostic::unsupported(
                self.span,
                "comparison string exceeds byte budget",
            ));
        }
        Ok(value)
    }
}
