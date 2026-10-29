//! Prepared typed props predicates, retaining source locations and short-circuit semantics.
use super::{Diagnostic, Prop, PropType, Span, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod member;
mod scalar;
mod state;
pub use scalar::{ScalarExpression, ScalarExpressionKind};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoolExpression {
    pub kind: BoolExpressionKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolExpressionKind {
    Literal(bool),
    Prop(String),
    Member(super::member_value::MemberValue),
    State(String),
    Not(Box<BoolExpression>),
    And(Box<BoolExpression>, Box<BoolExpression>),
    Or(Box<BoolExpression>, Box<BoolExpression>),
    Equal(ScalarExpression, ScalarExpression),
    NotEqual(ScalarExpression, ScalarExpression),
}
impl BoolExpression {
    pub(crate) fn erase_spans(&mut self) {
        self.span = Span::default();
        match &mut self.kind {
            BoolExpressionKind::Not(argument) => argument.erase_spans(),
            BoolExpressionKind::And(left, right) | BoolExpressionKind::Or(left, right) => {
                left.erase_spans();
                right.erase_spans();
            }
            BoolExpressionKind::Equal(left, right) | BoolExpressionKind::NotEqual(left, right) => {
                left.span = Span::default();
                right.span = Span::default();
            }
            _ => {}
        }
    }
    pub(crate) fn contains_comparison(&self) -> bool {
        match &self.kind {
            BoolExpressionKind::Equal(..) | BoolExpressionKind::NotEqual(..) => true,
            BoolExpressionKind::Not(value) => value.contains_comparison(),
            BoolExpressionKind::And(left, right) | BoolExpressionKind::Or(left, right) => {
                left.contains_comparison() || right.contains_comparison()
            }
            _ => false,
        }
    }
    fn budget(&self, depth: usize, steps: &mut usize) -> Result<(), Diagnostic> {
        let comparison = matches!(
            self.kind,
            BoolExpressionKind::Equal(..) | BoolExpressionKind::NotEqual(..)
        );
        *steps += if comparison { 3 } else { 1 };
        let path_depth = match &self.kind {
            BoolExpressionKind::Member(member) => {
                *steps += member.path.len();
                member.path.len()
            }
            BoolExpressionKind::Equal(left, right) | BoolExpressionKind::NotEqual(left, right) => {
                let mut longest = 0;
                for operand in [left, right] {
                    if let ScalarExpressionKind::Member(member) = &operand.kind {
                        *steps += member.path.len();
                        longest = longest.max(member.path.len() + 1);
                    }
                }
                longest
            }
            _ => 0,
        };
        if depth.saturating_add(path_depth) > 64 || (comparison && depth >= 64) || *steps > 8192 {
            return Err(Diagnostic::unsupported(
                self.span,
                "boolean expression exceeds depth/node budget",
            ));
        }
        Ok(())
    }
    pub fn validate(&self, props: &[Prop]) -> Result<(), Diagnostic> {
        self.validate_with_states(props, &[])
    }
    pub fn validate_with_states(
        &self,
        props: &[Prop],
        states: &[super::state_program::StateBinding],
    ) -> Result<(), Diagnostic> {
        fn visit(
            expression: &BoolExpression,
            props: &[Prop],
            states: &[super::state_program::StateBinding],
            depth: usize,
            steps: &mut usize,
        ) -> Result<(), Diagnostic> {
            expression.budget(depth, steps)?;
            match &expression.kind {
                BoolExpressionKind::Literal(_) => Ok(()),
                BoolExpressionKind::Member(member) => {
                    if self::member::kind(member, props, expression.span)? == PropType::Bool {
                        Ok(())
                    } else {
                        Err(Diagnostic::unsupported(
                            expression.span,
                            "predicate requires a boolean record field",
                        ))
                    }
                }
                BoolExpressionKind::State(name) => {
                    if states.iter().any(|binding| {
                        binding.name == *name
                            && matches!(
                                binding.initial,
                                super::component_state::StateValue::Bool(_)
                            )
                    }) {
                        Ok(())
                    } else {
                        Err(Diagnostic::unsupported(
                            expression.span,
                            "predicate requires a declared boolean state",
                        ))
                    }
                }
                BoolExpressionKind::Prop(name)
                    if props
                        .iter()
                        .any(|prop| prop.name == *name && prop.kind == PropType::Bool) =>
                {
                    Ok(())
                }
                BoolExpressionKind::Prop(_) => Err(Diagnostic::unsupported(
                    expression.span,
                    "predicate requires a declared boolean prop",
                )),
                BoolExpressionKind::Not(argument) => {
                    visit(argument, props, states, depth + 1, steps)
                }
                BoolExpressionKind::And(left, right) | BoolExpressionKind::Or(left, right) => {
                    visit(left, props, states, depth + 1, steps)?;
                    visit(right, props, states, depth + 1, steps)
                }
                BoolExpressionKind::Equal(left, right)
                | BoolExpressionKind::NotEqual(left, right) => {
                    if left.validate(props)? != right.validate(props)? {
                        return Err(Diagnostic::unsupported(
                            expression.span,
                            "comparison operands must have the same scalar type",
                        ));
                    }
                    Ok(())
                }
            }
        }
        visit(self, props, states, 0, &mut 0)
    }
    pub fn evaluate(&self, props: &BTreeMap<String, Value>) -> Result<bool, Diagnostic> {
        self.evaluate_with_states(props, &BTreeMap::new())
    }
    pub fn evaluate_with_states(
        &self,
        props: &BTreeMap<String, Value>,
        states: &BTreeMap<String, super::component_state::StateValue>,
    ) -> Result<bool, Diagnostic> {
        fn visit(
            expression: &BoolExpression,
            props: &BTreeMap<String, Value>,
            states: &BTreeMap<String, super::component_state::StateValue>,
            depth: usize,
            steps: &mut usize,
        ) -> Result<bool, Diagnostic> {
            expression.budget(depth, steps)?;
            Ok(match &expression.kind {
                BoolExpressionKind::Literal(value) => *value,
                BoolExpressionKind::Member(member) => {
                    match self::member::bound(member, props, expression.span)? {
                        Value::Bool(value) => value,
                        _ => {
                            return Err(Diagnostic::unsupported(
                                expression.span,
                                "predicate requires a boolean record field",
                            ));
                        }
                    }
                }
                BoolExpressionKind::State(name) => match states.get(name) {
                    Some(super::component_state::StateValue::Bool(value)) => *value,
                    _ => {
                        return Err(Diagnostic::unsupported(
                            expression.span,
                            "boolean state binding is missing or has a different type",
                        ));
                    }
                },
                BoolExpressionKind::Prop(name) => match props.get(name) {
                    Some(Value::Bool(value)) => *value,
                    _ => {
                        return Err(Diagnostic::unsupported(
                            expression.span,
                            "boolean prop binding is missing or has a different type",
                        ));
                    }
                },
                BoolExpressionKind::Not(argument) => {
                    !visit(argument, props, states, depth + 1, steps)?
                }
                BoolExpressionKind::And(left, right) => {
                    visit(left, props, states, depth + 1, steps)?
                        && visit(right, props, states, depth + 1, steps)?
                }
                BoolExpressionKind::Or(left, right) => {
                    visit(left, props, states, depth + 1, steps)?
                        || visit(right, props, states, depth + 1, steps)?
                }
                BoolExpressionKind::Equal(left, right)
                | BoolExpressionKind::NotEqual(left, right) => {
                    let left = left.evaluate(props)?;
                    let right = right.evaluate(props)?;
                    if std::mem::discriminant(&left) != std::mem::discriminant(&right) {
                        return Err(Diagnostic::unsupported(
                            expression.span,
                            "comparison bindings must have the same scalar type",
                        ));
                    }
                    (left == right) != matches!(expression.kind, BoolExpressionKind::NotEqual(..))
                }
            })
        }
        visit(self, props, states, 0, &mut 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn expression(kind: BoolExpressionKind) -> BoolExpression {
        BoolExpression {
            kind,
            span: Span {
                start: 377,
                end: 430,
            },
        }
    }
    fn prop(name: &str) -> BoolExpression {
        expression(BoolExpressionKind::Prop(name.into()))
    }
    #[test]
    fn typed_predicates_preserve_or_and_not_short_circuit_behavior() {
        let source = expression(BoolExpressionKind::Or(
            Box::new(expression(BoolExpressionKind::Not(Box::new(prop(
                "canEdit",
            ))))),
            Box::new(prop("busy")),
        ));
        source
            .validate(&[
                Prop {
                    name: "canEdit".into(),
                    kind: PropType::Bool,
                },
                Prop {
                    name: "busy".into(),
                    kind: PropType::Bool,
                },
            ])
            .unwrap();
        assert!(
            source
                .evaluate(&BTreeMap::from([("canEdit".into(), Value::Bool(false))]))
                .unwrap()
        );
        assert!(
            !source
                .evaluate(&BTreeMap::from([
                    ("canEdit".into(), Value::Bool(true)),
                    ("busy".into(), Value::Bool(false))
                ]))
                .unwrap()
        );
        let and = expression(BoolExpressionKind::And(
            Box::new(expression(BoolExpressionKind::Literal(false))),
            Box::new(prop("missing")),
        ));
        assert!(!and.evaluate(&BTreeMap::new()).unwrap());
    }
    #[test]
    fn wrong_type_and_oversized_predicates_report_original_spans() {
        let predicate = prop("busy");
        let diagnostic = predicate
            .validate(&[Prop {
                name: "busy".into(),
                kind: PropType::String,
            }])
            .unwrap_err();
        assert_eq!(
            diagnostic.span,
            Span {
                start: 377,
                end: 430
            }
        );
        let mut nested = predicate;
        for _ in 0..65 {
            nested = expression(BoolExpressionKind::Not(Box::new(nested)));
        }
        assert!(
            nested
                .validate(&[Prop {
                    name: "busy".into(),
                    kind: PropType::Bool
                }])
                .is_err()
        );
    }
}
