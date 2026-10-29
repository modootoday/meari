//! Predicate operands share the existing bounded immutable-record contract.
use crate::domain::{Diagnostic, Prop, PropType, Span, Value, member_value::MemberValue};
use std::collections::BTreeMap;

pub(super) fn kind(
    member: &MemberValue,
    props: &[Prop],
    span: Span,
) -> Result<PropType, Diagnostic> {
    member
        .kind(props)
        .map_err(|error| Diagnostic::unsupported(span, error))
}

pub(super) fn bound(
    member: &MemberValue,
    props: &BTreeMap<String, Value>,
    span: Span,
) -> Result<Value, Diagnostic> {
    let root = match member.root.as_ref() {
        Value::Prop(name) => props.get(name).ok_or_else(|| {
            Diagnostic::unsupported(span, "predicate record prop binding is missing")
        })?,
        Value::Record(_) => member.root.as_ref(),
        _ => {
            return Err(Diagnostic::unsupported(
                span,
                "predicate lexical scope is unresolved",
            ));
        }
    };
    let resolved = MemberValue {
        root: Box::new(root.clone()),
        path: member.path.clone(),
    };
    resolved
        .bound()
        .map_err(|error| Diagnostic::unsupported(span, error))?
        .ok_or_else(|| Diagnostic::unsupported(span, "predicate requires a bound record"))
}

pub(super) fn substitute(member: &mut MemberValue, name: &str, replacement: &Value) {
    if matches!(member.root.as_ref(), Value::Local(binding) if binding == name) {
        *member.root = replacement.clone();
    }
}

impl super::BoolExpression {
    pub(crate) fn contains_member(&self) -> bool {
        use super::{BoolExpressionKind as Kind, ScalarExpressionKind as Scalar};
        match &self.kind {
            Kind::Member(_) => true,
            Kind::Not(value) => value.contains_member(),
            Kind::And(left, right) | Kind::Or(left, right) => {
                left.contains_member() || right.contains_member()
            }
            Kind::Equal(left, right) | Kind::NotEqual(left, right) => {
                matches!(left.kind, Scalar::Member(_)) || matches!(right.kind, Scalar::Member(_))
            }
            _ => false,
        }
    }

    pub(crate) fn contains_local(&self) -> bool {
        use super::{BoolExpressionKind as Kind, ScalarExpressionKind as Scalar};
        let local = |member: &MemberValue| matches!(member.root.as_ref(), Value::Local(_));
        match &self.kind {
            Kind::Member(member) => local(member),
            Kind::Not(value) => value.contains_local(),
            Kind::And(left, right) | Kind::Or(left, right) => {
                left.contains_local() || right.contains_local()
            }
            Kind::Equal(left, right) | Kind::NotEqual(left, right) => [left, right]
                .into_iter()
                .any(|operand| matches!(&operand.kind, Scalar::Member(member) if local(member))),
            _ => false,
        }
    }

    pub(crate) fn substitute_local(&mut self, name: &str, replacement: &Value) {
        use super::{BoolExpressionKind as Kind, ScalarExpressionKind as Scalar};
        match &mut self.kind {
            Kind::Member(member) => substitute(member, name, replacement),
            Kind::Not(value) => value.substitute_local(name, replacement),
            Kind::And(left, right) | Kind::Or(left, right) => {
                left.substitute_local(name, replacement);
                right.substitute_local(name, replacement);
            }
            Kind::Equal(left, right) | Kind::NotEqual(left, right) => {
                for operand in [left, right] {
                    if let Scalar::Member(member) = &mut operand.kind {
                        substitute(member, name, replacement);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::record_value::RecordValue;

    fn record(name: &str, value: Value, kind: PropType) -> Value {
        Value::Record(
            RecordValue::new(
                vec![Prop {
                    name: name.into(),
                    kind,
                }],
                BTreeMap::from([(name.into(), value)]),
            )
            .unwrap(),
        )
    }

    fn nested() -> (Value, MemberValue) {
        let flags = record("editable", Value::Bool(false), PropType::Bool);
        let Value::Record(flags_record) = &flags else {
            unreachable!()
        };
        let row = record("flags", flags.clone(), flags_record.kind());
        let member = MemberValue {
            root: Box::new(Value::Prop("row".into())),
            path: vec!["flags".into(), "editable".into()],
        };
        (row, member)
    }

    #[test]
    fn nested_schema_and_runtime_read_agree() {
        let (row, member) = nested();
        let Value::Record(value) = &row else {
            unreachable!()
        };
        let props = vec![Prop {
            name: "row".into(),
            kind: value.kind(),
        }];
        assert_eq!(
            kind(&member, &props, Span::default()).unwrap(),
            PropType::Bool
        );
        assert_eq!(
            bound(
                &member,
                &BTreeMap::from([("row".into(), row)]),
                Span::default()
            )
            .unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn lexical_read_requires_matching_substitution() {
        let (row, mut member) = nested();
        *member.root = Value::Local("item".into());
        let span = Span { start: 5, end: 19 };
        assert_eq!(
            bound(&member, &BTreeMap::new(), span).unwrap_err().span,
            span
        );
        substitute(&mut member, "other", &row);
        assert!(bound(&member, &BTreeMap::new(), span).is_err());
        substitute(&mut member, "item", &row);
        assert_eq!(
            bound(&member, &BTreeMap::new(), span).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn missing_fields_nonrecords_and_overbudget_paths_keep_source_span() {
        let (row, mut member) = nested();
        let span = Span { start: 21, end: 42 };
        let props = BTreeMap::from([("row".into(), row)]);
        member.path.push("unknown".into());
        assert_eq!(bound(&member, &props, span).unwrap_err().span, span);
        member.path = vec!["flags".into(); 33];
        assert!(bound(&member, &props, span).is_err());
        *member.root = Value::Bool(true);
        assert!(bound(&member, &props, span).is_err());
        *member.root = Value::Prop("missing".into());
        assert_eq!(bound(&member, &props, span).unwrap_err().span, span);
    }
}
