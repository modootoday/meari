use super::*;
use crate::application::hook_values::{Budget, Owner};

#[test]
fn publication_barrier_is_transient_and_does_not_poison_expression_candidate() {
    let mut owner = Owner::new(Budget::default());
    let peer = owner.begin().unwrap();
    let selected = owner.begin().unwrap();
    let guard = owner.prepare_publish(selected).unwrap();
    let failure = evaluate(
        peer,
        &number(2.0),
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.cause, Some(Error::Busy));
    drop(guard);
    let result = evaluate(
        failure.candidate,
        &number(2.0),
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .unwrap();
    let value = result.root;
    owner.prepare_publish(result.candidate).unwrap().publish();
    assert_eq!(owner.number(&value), Ok(2.0));
}

fn e(kind: Kind) -> Expression {
    Expression {
        span: Span { start: 3, end: 9 },
        kind,
    }
}
fn number(value: f64) -> Expression {
    e(Kind::Number(value.to_bits()))
}
fn field(target: Expression, key: &str) -> Expression {
    e(Kind::Field {
        target: Box::new(target),
        key: key.into(),
    })
}
fn initial() -> (Owner, Value, Value) {
    let mut owner = Owner::new(Budget::default());
    let mut tx = owner.begin().unwrap();
    let count = tx.number(0.0).unwrap();
    let label = tx.string("shared".into()).unwrap();
    let root = tx
        .record(vec![
            ("count".into(), count),
            ("label".into(), label.clone()),
        ])
        .unwrap();
    owner.commit(tx).unwrap();
    (owner, root, label)
}

#[test]
fn immutable_updater_uses_staged_read_and_prepare_publish_without_losing_aliases() {
    let (mut owner, old, label) = initial();
    let expr = e(Kind::Record(vec![
        Entry::Spread {
            value: e(Kind::Previous),
        },
        Entry::Field {
            key: "count".into(),
            value: e(Kind::Add {
                left: Box::new(field(e(Kind::Previous), "count")),
                right: Box::new(number(1.0)),
            }),
        },
    ]));
    let result = evaluate(
        owner.begin().unwrap(),
        &expr,
        Bindings {
            states: &BTreeMap::new(),
            previous: Some(&old),
        },
    )
    .unwrap();
    let next_count = result
        .candidate
        .field(&result.root, "count")
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate.read_number(&next_count), Ok(1.0));
    assert_eq!(owner.kind(&result.root), Err(Error::StaleValue));
    let old_count = owner.field(&old, "count").unwrap().unwrap();
    assert_eq!(owner.number(&old_count), Ok(0.0));
    let next = result.root;
    let receipt = owner.prepare_publish(result.candidate).unwrap().publish();
    assert!(receipt.changed);
    assert_eq!(
        owner.number(&owner.field(&next, "count").unwrap().unwrap()),
        Ok(1.0)
    );
    assert!(
        owner
            .same_value(&label, &owner.field(&next, "label").unwrap().unwrap())
            .unwrap()
    );
    assert_eq!(owner.number(&old_count), Ok(0.0));
}

#[test]
fn same_root_is_allocation_free_but_new_deep_equal_record_has_distinct_identity() {
    let (mut owner, old, _) = initial();
    let states = BTreeMap::from([("state".into(), old.clone())]);
    let result = evaluate(
        owner.begin().unwrap(),
        &e(Kind::State("state".into())),
        Bindings {
            states: &states,
            previous: None,
        },
    )
    .unwrap();
    assert!(
        !owner
            .prepare_publish(result.candidate)
            .unwrap()
            .publish()
            .changed
    );
    assert!(owner.same_value(&old, &result.root).unwrap());
    let expr = e(Kind::Record(vec![Entry::Spread {
        value: e(Kind::State("state".into())),
    }]));
    let result = evaluate(
        owner.begin().unwrap(),
        &expr,
        Bindings {
            states: &states,
            previous: None,
        },
    )
    .unwrap();
    assert!(!result.candidate.same_value(&old, &result.root).unwrap());
    let root = result.root;
    owner.prepare_publish(result.candidate).unwrap().publish();
    assert!(!owner.same_value(&old, &root).unwrap());
}

#[test]
fn dense_array_preserves_repeated_root_and_missing_property_is_undefined() {
    let (mut owner, old, _) = initial();
    let states = BTreeMap::from([("state".into(), old.clone())]);
    let expr = e(Kind::Array(vec![
        e(Kind::State("state".into())),
        e(Kind::State("state".into())),
        field(e(Kind::State("state".into())), "missing"),
    ]));
    let result = evaluate(
        owner.begin().unwrap(),
        &expr,
        Bindings {
            states: &states,
            previous: None,
        },
    )
    .unwrap();
    let root = result.root;
    owner.prepare_publish(result.candidate).unwrap().publish();
    let a = owner.element(&root, 0).unwrap().unwrap();
    let b = owner.element(&root, 1).unwrap().unwrap();
    assert!(owner.same_value(&a, &b).unwrap());
    assert_eq!(
        owner.kind(&owner.element(&root, 2).unwrap().unwrap()),
        Ok(ValueKind::Undefined)
    );
}

#[test]
fn partial_expression_failure_retains_diagnostic_and_cannot_commit_earlier_allocations() {
    let (mut owner, old, _) = initial();
    let baseline = owner.usage();
    let expr = e(Kind::Array(vec![
        number(1.0),
        e(Kind::State("missing".into())),
    ]));
    let failure = evaluate(
        owner.begin().unwrap(),
        &expr,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.diagnostic.span, expr.span);
    assert_eq!(failure.diagnostic.message, "unbound structured state");
    assert_eq!(
        owner.commit(failure.candidate),
        Err(Error::InvalidExpression)
    );
    assert_eq!(owner.usage(), baseline);
    assert_eq!(owner.kind(&old), Ok(ValueKind::Record));
}

#[test]
fn foreign_root_and_wrong_spread_kind_are_not_silently_copied_or_coerced() {
    let (mut owner, old, _) = initial();
    let other = Owner::new(Budget::default());
    let bindings = BTreeMap::from([("foreign".into(), old)]);
    let failure = evaluate(
        other.begin().unwrap(),
        &e(Kind::State("foreign".into())),
        Bindings {
            states: &bindings,
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.cause, Some(Error::ForeignOwner));
    let expr = e(Kind::Record(vec![Entry::Spread { value: number(1.0) }]));
    let failure = evaluate(
        owner.begin().unwrap(),
        &expr,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.cause, Some(Error::WrongKind));
    assert_eq!(
        owner.commit(failure.candidate),
        Err(Error::InvalidExpression)
    );
}

#[test]
fn static_ir_roundtrip_preserves_signed_zero_and_rejects_nonfinite_or_prototype_literals() {
    let minus = number(-0.0);
    let plus = number(0.0);
    assert_ne!(minus, plus);
    let json = serde_json::to_string(&minus).unwrap();
    let restored: Expression = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, minus);
    assert!(
        number(f64::NAN)
            .validate()
            .unwrap_err()
            .message
            .contains("finite")
    );
    let bad = e(Kind::Record(vec![Entry::Field {
        key: "__proto__".into(),
        value: e(Kind::Null),
    }]));
    assert!(bad.validate().unwrap_err().message.contains("prototype"));
}

#[test]
fn source_budget_and_mixed_addition_fail_closed_with_original_span() {
    let owner = Owner::new(Budget::default());
    let mut deep = e(Kind::Null);
    for _ in 0..65 {
        deep = e(Kind::Not(Box::new(deep)));
    }
    let failure = evaluate(
        owner.begin().unwrap(),
        &deep,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert!(failure.diagnostic.message.contains("budget"));
    let mixed = e(Kind::Add {
        left: Box::new(number(1.0)),
        right: Box::new(e(Kind::Text("x".into()))),
    });
    let failure = evaluate(
        owner.begin().unwrap(),
        &mixed,
        Bindings {
            states: &BTreeMap::new(),
            previous: None,
        },
    )
    .err()
    .unwrap();
    assert!(failure.diagnostic.message.contains("coercion"));
    assert_eq!(failure.diagnostic.span, mixed.span);
}
