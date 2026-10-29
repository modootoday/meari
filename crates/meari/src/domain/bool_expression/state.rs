//! Bind accepted props without freezing per-instance state reads.
use super::{BoolExpression, BoolExpressionKind as Kind};
use crate::domain::{Diagnostic, Value};
use std::collections::BTreeMap;

impl BoolExpression {
    pub(crate) fn state_reference_span(&self, name: &str) -> Option<crate::domain::Span> {
        match &self.kind {
            Kind::State(candidate) if candidate == name => Some(self.span),
            Kind::Not(value) => value.state_reference_span(name),
            Kind::And(left, right) | Kind::Or(left, right) => left
                .state_reference_span(name)
                .or_else(|| right.state_reference_span(name)),
            _ => None,
        }
    }

    pub fn contains_state(&self) -> bool {
        match &self.kind {
            Kind::State(_) => true,
            Kind::Not(value) => value.contains_state(),
            Kind::And(left, right) | Kind::Or(left, right) => {
                left.contains_state() || right.contains_state()
            }
            _ => false,
        }
    }

    pub fn bind_props(&self, props: &BTreeMap<String, Value>) -> Result<Self, Diagnostic> {
        fn visit(
            expression: &BoolExpression,
            props: &BTreeMap<String, Value>,
            depth: usize,
            steps: &mut usize,
        ) -> Result<BoolExpression, Diagnostic> {
            expression.budget(depth, steps)?;
            let kind = match &expression.kind {
                Kind::Member(member) if matches!(member.root.as_ref(), Value::Local(_)) => {
                    Kind::Member(member.clone())
                }
                Kind::Member(_) => Kind::Literal(expression.evaluate(props)?),
                Kind::Prop(name) => match props.get(name) {
                    Some(Value::Bool(value)) => Kind::Literal(*value),
                    _ => {
                        return Err(Diagnostic::unsupported(
                            expression.span,
                            "boolean prop binding is missing or has a different type",
                        ));
                    }
                },
                Kind::Not(value) => Kind::Not(Box::new(visit(value, props, depth + 1, steps)?)),
                Kind::And(left, right) => Kind::And(
                    Box::new(visit(left, props, depth + 1, steps)?),
                    Box::new(visit(right, props, depth + 1, steps)?),
                ),
                Kind::Or(left, right) => Kind::Or(
                    Box::new(visit(left, props, depth + 1, steps)?),
                    Box::new(visit(right, props, depth + 1, steps)?),
                ),
                Kind::Equal(left, right) | Kind::NotEqual(left, right)
                    if expression.contains_local() =>
                {
                    let bind = |operand: &super::ScalarExpression| -> Result<super::ScalarExpression, Diagnostic> {
                        use super::ScalarExpressionKind as Scalar;
                        let kind = match &operand.kind {
                            Scalar::Member(member) if matches!(member.root.as_ref(), Value::Local(_)) => operand.kind.clone(),
                            _ => match operand.evaluate(props)? {
                                super::scalar::ScalarValue::String(value) => Scalar::String(value.into_owned()),
                                super::scalar::ScalarValue::Bool(value) => Scalar::Bool(value),
                            },
                        };
                        Ok(super::ScalarExpression { kind, span: operand.span })
                    };
                    if matches!(expression.kind, Kind::Equal(..)) {
                        Kind::Equal(bind(left)?, bind(right)?)
                    } else {
                        Kind::NotEqual(bind(left)?, bind(right)?)
                    }
                }
                Kind::Equal(..) | Kind::NotEqual(..) => Kind::Literal(expression.evaluate(props)?),
                kind => kind.clone(),
            };
            Ok(BoolExpression {
                kind,
                span: expression.span,
            })
        }
        visit(self, props, 0, &mut 0)
    }
}
