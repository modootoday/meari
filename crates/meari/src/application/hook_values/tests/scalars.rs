//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

#[test]
fn scalar_same_value_preserves_nan_signed_zero_and_type() {
    let mut owner = Owner::new(Budget::default());
    let mut tx = owner.begin().unwrap();
    let nan = tx.number(f64::NAN).unwrap();
    let other_nan = tx.number(f64::from_bits(0x7ff8_0000_0000_0001)).unwrap();
    let zero = tx.number(0.0).unwrap();
    let negative = tx.number(-0.0).unwrap();
    let infinity = tx.number(f64::INFINITY).unwrap();
    let string = tx.string("0".into()).unwrap();
    let same_string = tx.string("0".into()).unwrap();
    let boolean = tx.boolean(false).unwrap();
    let null = tx.null().unwrap();
    let undefined = tx.undefined().unwrap();
    owner.commit(tx).unwrap();
    assert!(owner.same_value(&nan, &other_nan).unwrap());
    assert!(!owner.same_value(&zero, &negative).unwrap());
    assert!(!owner.same_value(&zero, &string).unwrap());
    assert!(owner.same_value(&string, &same_string).unwrap());
    assert!(!owner.same_value(&null, &undefined).unwrap());
    assert!(!owner.same_value(&boolean, &zero).unwrap());
    assert!(!owner.boolean(&boolean).unwrap());
    assert_eq!(owner.text(&string).unwrap(), "0");
    assert!(owner.number(&infinity).unwrap().is_infinite());
    assert!(owner.number(&negative).unwrap().is_sign_negative());
}

#[test]
fn empty_arrays_are_objects_not_structurally_interned_values() {
    let mut owner = Owner::new(Budget::default());
    let mut tx = owner.begin().unwrap();
    let first = tx.array(vec![]).unwrap();
    let second = tx.array(vec![]).unwrap();
    owner.commit(tx).unwrap();
    assert!(!owner.same_value(&first, &second).unwrap());
    assert!(owner.element(&first, 0).unwrap().is_none());
    fails(owner.field(&first, "x"), Error::WrongKind);
}

#[test]
fn rollback_releases_budget_without_reusing_an_escaped_identity() {
    let mut owner = Owner::new(Budget::new(1, 1, 4, 1).unwrap());
    let mut abandoned = owner.begin().unwrap();
    let escaped = abandoned.string("old".into()).unwrap();
    drop(abandoned);
    assert_eq!(owner.usage(), Usage::default());
    let mut next = owner.begin().unwrap();
    let current = next.string("new".into()).unwrap();
    assert_ne!(escaped.id, current.id);
    owner.commit(next).unwrap();
    assert_eq!(owner.kind(&escaped), Err(Error::StaleValue));
    assert_eq!(owner.text(&current).unwrap(), "new");
}

#[test]
fn stale_candidate_cannot_overwrite_the_committed_winner() {
    let mut owner = Owner::new(Budget::default());
    let mut first = owner.begin().unwrap();
    let mut second = owner.begin().unwrap();
    let winner = first.record(vec![]).unwrap();
    let loser = second.record(vec![]).unwrap();
    assert_ne!(winner.id, loser.id);
    owner.commit(first).unwrap();
    fails(second.number(2.0), Error::StaleTransaction);
    assert_eq!(owner.commit(second), Err(Error::StaleTransaction));
    assert_eq!(owner.kind(&winner).unwrap(), ValueKind::Record);
    assert_eq!(owner.kind(&loser), Err(Error::StaleValue));
    assert_eq!(owner.usage().nodes, 1);
    assert_eq!(owner.identity.reserved.get(), Usage::default());
}

#[test]
fn dropping_a_candidate_preserves_committed_aliases() {
    let mut owner = Owner::new(Budget::default());
    let mut original = owner.begin().unwrap();
    let committed = original.array(vec![]).unwrap();
    owner.commit(original).unwrap();
    let baseline = owner.usage();
    let mut candidate = owner.begin().unwrap();
    candidate.array(vec![committed.clone()]).unwrap();
    drop(candidate);
    assert_eq!(owner.usage(), baseline);
    assert!(owner.same_value(&committed, &committed.clone()).unwrap());
}

#[test]
fn foreign_handles_and_foreign_transactions_do_not_cross_runtime_scopes() {
    let mut first = Owner::new(Budget::default());
    let mut second = Owner::new(Budget::default());
    let mut tx = first.begin().unwrap();
    let foreign = tx.null().unwrap();
    first.commit(tx).unwrap();
    let mut second_tx = second.begin().unwrap();
    fails(second_tx.array(vec![foreign.clone()]), Error::ForeignOwner);
    assert_eq!(second.kind(&foreign), Err(Error::ForeignOwner));
    assert_eq!(first.commit(second_tx), Err(Error::ForeignOwner));
    let mut fresh = second.begin().unwrap();
    let local = fresh.null().unwrap();
    second.commit(fresh).unwrap();
    assert_eq!(local.id, foreign.id);
    assert_eq!(second.kind(&local).unwrap(), ValueKind::Null);
}

#[test]
fn close_and_drop_revoke_handles_and_pending_candidates() {
    let mut owner = Owner::new(Budget::default());
    let mut tx = owner.begin().unwrap();
    let handle = tx.null().unwrap();
    owner.commit(tx).unwrap();
    let mut pending = owner.begin().unwrap();
    pending.array(vec![handle.clone()]).unwrap();
    owner.close();
    assert_eq!(owner.kind(&handle), Err(Error::Closed));
    assert_eq!(owner.commit(pending), Err(Error::Closed));
    assert_eq!(owner.usage(), Usage::default());
    assert!(matches!(owner.begin(), Err(Error::Closed)));
    let mut doomed = {
        let temporary = Owner::new(Budget::default());
        temporary.begin().unwrap()
    };
    fails(doomed.null(), Error::Closed);
}

#[test]
fn aggregate_pending_budget_and_candidate_count_are_bounded() {
    let owner = Owner::new(Budget::new(1, 1, 1, 1).unwrap());
    let mut first = owner.begin().unwrap();
    first.null().unwrap();
    let mut second = owner.begin().unwrap();
    fails(second.null(), Error::BudgetExceeded);
    drop(first);
    fails(second.null(), Error::BudgetExceeded);
    drop(second);
    owner.begin().unwrap().null().unwrap();
    let candidates: Vec<_> = (0..64).map(|_| owner.begin().unwrap()).collect();
    assert!(matches!(owner.begin(), Err(Error::BudgetExceeded)));
    drop(candidates);
    assert!(owner.begin().is_ok());
}

#[test]
fn depth_edges_bytes_and_duplicate_fields_fail_before_publication() {
    let mut owner = Owner::new(Budget::new(16, 2, 4, 1).unwrap());
    let mut tx = owner.begin().unwrap();
    let item = tx.string("x".into()).unwrap();
    owner.commit(tx).unwrap();
    let baseline = owner.usage();
    let mut tx = owner.begin().unwrap();
    fails(tx.string("1234".into()), Error::BudgetExceeded);
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    let mut tx = owner.begin().unwrap();
    fails(tx.array(vec![item.clone(); 3]), Error::BudgetExceeded);
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    let mut tx = owner.begin().unwrap();
    fails(
        tx.record(vec![("a".into(), item.clone()), ("a".into(), item.clone())]),
        Error::DuplicateField,
    );
    assert_eq!(owner.commit(tx), Err(Error::DuplicateField));
    let mut tx = owner.begin().unwrap();
    let array = tx.array(vec![item]).unwrap();
    fails(tx.array(vec![array]), Error::BudgetExceeded);
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    assert_eq!(owner.usage(), baseline);
}

#[test]
fn failed_allocation_poison_prevents_a_partial_commit() {
    let mut owner = Owner::new(Budget::new(1, 1, 4, 1).unwrap());
    let mut tx = owner.begin().unwrap();
    let staged = tx.null().unwrap();
    fails(tx.null(), Error::BudgetExceeded);
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    assert_eq!(owner.kind(&staged), Err(Error::StaleValue));
    assert_eq!(owner.usage(), Usage::default());
    assert_eq!(owner.identity.reserved.get(), Usage::default());
}

#[test]
fn overcapacity_string_is_charged_by_retained_buffer_not_short_payload() {
    let mut owner = Owner::new(Budget::new(4, 4, 8, 4).unwrap());
    let mut oversized = String::with_capacity(1024);
    oversized.push('x');
    assert_eq!(oversized.len(), 1);
    assert!(oversized.capacity() > 8);
    let mut tx = owner.begin().unwrap();
    fails(tx.string(oversized), Error::BudgetExceeded);
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    assert_eq!(owner.usage(), Usage::default());
    assert_eq!(owner.identity.reserved.get(), Usage::default());

    let text = "ok".to_owned();
    let capacity = text.capacity();
    let mut tx = owner.begin().unwrap();
    let value = tx.string(text).unwrap();
    owner.commit(tx).unwrap();
    assert_eq!(owner.text(&value).unwrap(), "ok");
    assert_eq!(owner.usage().bytes, capacity);
}

#[test]
fn overcapacity_record_key_cannot_bypass_budget_or_mutate_original_state() {
    let mut owner = Owner::new(Budget::new(4, 4, 8, 4).unwrap());
    let mut tx = owner.begin().unwrap();
    let original = tx.null().unwrap();
    owner.commit(tx).unwrap();
    let baseline = owner.usage();
    let mut key = String::with_capacity(1024);
    key.push('x');
    assert_eq!(key.len(), 1);
    assert!(key.capacity() > 8);
    let mut tx = owner.begin().unwrap();
    fails(
        tx.record(vec![(key, original.clone())]),
        Error::BudgetExceeded,
    );
    assert_eq!(owner.commit(tx), Err(Error::BudgetExceeded));
    assert_eq!(owner.usage(), baseline);
    assert_eq!(owner.kind(&original).unwrap(), ValueKind::Null);
    assert_eq!(owner.identity.reserved.get(), Usage::default());

    let key = "ok".to_owned();
    let capacity = key.capacity();
    let mut tx = owner.begin().unwrap();
    let record = tx.record(vec![(key, original.clone())]).unwrap();
    owner.commit(tx).unwrap();
    assert_eq!(owner.usage().bytes, capacity);
    assert!(
        owner
            .same_value(&original, &owner.field(&record, "ok").unwrap().unwrap())
            .unwrap()
    );
}

#[test]
fn checked_identity_revision_and_usage_overflow_leave_original_state_unchanged() {
    let mut owner = Owner::new(Budget::default());
    owner.identity.sequence.set(u64::MAX);
    fails(owner.begin().unwrap().null(), Error::Overflow);
    assert_eq!(owner.usage(), Usage::default());
    owner.identity.sequence.set(0);
    owner.identity.revision.set(u64::MAX);
    let mut tx = owner.begin().unwrap();
    let unpublished = tx.null().unwrap();
    assert_eq!(owner.commit(tx), Err(Error::Overflow));
    assert_eq!(owner.kind(&unpublished), Err(Error::StaleValue));
    assert_eq!(owner.usage(), Usage::default());
    assert_eq!(
        Usage {
            nodes: usize::MAX,
            edges: 0,
            bytes: 0
        }
        .add(Usage {
            nodes: 1,
            edges: 0,
            bytes: 0
        }),
        Err(Error::Overflow)
    );
    assert!(matches!(
        Budget::new(0, 1, 1, 1),
        Err(Error::BudgetExceeded)
    ));
}
