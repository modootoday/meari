//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

#[test]
fn prepared_structured_publication_preserves_nested_aliases_and_root_identity() {
    let mut owner = Owner::new(Budget::default());
    let mut initial = owner.begin().unwrap();
    let leaf = initial.string("shared".into()).unwrap();
    let old_root = initial.record(vec![("leaf".into(), leaf.clone())]).unwrap();
    owner.commit(initial).unwrap();
    let old_alias = old_root.clone();
    let mut candidate = owner.begin().unwrap();
    let new_root = candidate
        .record(vec![("leaf".into(), leaf.clone())])
        .unwrap();
    let rows = candidate
        .array(vec![new_root.clone(), new_root.clone(), old_root.clone()])
        .unwrap();
    assert!(!candidate.same_value(&old_root, &new_root).unwrap());
    assert!(candidate.same_value(&old_root, &old_alias).unwrap());
    let staged_usage = candidate.reserved;
    let baseline = owner.usage();
    let receipt = owner.prepare_publish(candidate).unwrap().publish();
    assert_eq!(receipt.usage, baseline.add(staged_usage).unwrap());
    let first = owner.element(&rows, 0).unwrap().unwrap();
    let second = owner.element(&rows, 1).unwrap().unwrap();
    let previous = owner.element(&rows, 2).unwrap().unwrap();
    assert!(owner.same_value(&first, &second).unwrap());
    assert!(owner.same_value(&previous, &old_alias).unwrap());
    assert!(!owner.same_value(&first, &previous).unwrap());
    let retained = owner.field(&first, "leaf").unwrap().unwrap();
    assert!(owner.same_value(&leaf, &retained).unwrap());
    assert_eq!(owner.text(&retained), Ok("shared"));
    assert_eq!(owner.identity.reserved.get(), Usage::default());
}

#[test]
fn changed_prepared_publication_invalidates_older_peer_after_barrier_is_released() {
    let mut owner = Owner::new(Budget::default());
    let mut peer = owner.begin().unwrap();
    let old_candidate = peer.number(1.0).unwrap();
    let mut selected = owner.begin().unwrap();
    let current = selected.number(2.0).unwrap();
    let identity = owner.identity.clone();
    owner.prepare_publish(selected).unwrap().publish();
    assert!(!identity.publishing.get());
    assert_eq!(
        peer.read_number(&old_candidate),
        Err(Error::StaleTransaction)
    );
    assert_eq!(peer.null().err(), Some(Error::StaleTransaction));
    assert_eq!(owner.number(&current), Ok(2.0));
    assert_eq!(owner.number(&old_candidate), Err(Error::StaleValue));
    drop(peer);
    assert_eq!(identity.reserved.get(), Usage::default());
    let fresh = owner.begin().unwrap();
    assert_eq!(fresh.read_number(&current), Ok(2.0));
}

#[test]
fn prepared_publish_blocks_escaped_peer_without_poisoning_or_charging_it() {
    let mut owner = Owner::new(Budget::default());
    let mut peer = owner.begin().unwrap();
    let peer_value = peer.number(3.0).unwrap();
    let mut candidate = owner.begin().unwrap();
    candidate.number(7.0).unwrap();
    let identity = owner.identity.clone();
    let reserved = identity.reserved.get();
    let guard = owner.prepare_publish(candidate).unwrap();
    assert_eq!(guard.owner.begin().err(), Some(Error::Busy));
    assert_eq!(peer.null().err(), Some(Error::Busy));
    assert_eq!(peer.read_number(&peer_value), Err(Error::Busy));
    assert_eq!(peer.failure, None);
    assert_eq!(identity.reserved.get(), reserved);
    let candidate = guard.abort();
    assert_eq!(peer.read_number(&peer_value), Ok(3.0));
    assert!(peer.null().is_ok());
    drop(candidate);
    owner.commit(peer).unwrap();
    assert_eq!(owner.number(&peer_value), Ok(3.0));
}

#[test]
fn peer_drop_during_preparation_cannot_remove_guard_reservation_or_publish_partial_heap() {
    let mut owner = Owner::new(Budget::default());
    let mut peer = owner.begin().unwrap();
    let abandoned = peer.string("peer".into()).unwrap();
    let mut candidate = owner.begin().unwrap();
    let selected = candidate.string("selected".into()).unwrap();
    let own_reservation = candidate.reserved;
    let identity = owner.identity.clone();
    let guard = owner.prepare_publish(candidate).unwrap();
    drop(peer);
    assert_eq!(identity.reserved.get(), own_reservation);
    assert_eq!(identity.candidates.get(), 1);
    let receipt = guard.publish();
    assert!(receipt.changed);
    assert_eq!(receipt.revision, 1);
    assert_eq!(owner.usage(), own_reservation);
    assert_eq!(owner.text(&selected), Ok("selected"));
    assert_eq!(owner.text(&abandoned), Err(Error::StaleValue));
    assert_eq!(identity.reserved.get(), Usage::default());
    assert_eq!(identity.candidates.get(), 0);
    assert!(!identity.publishing.get());
}

#[test]
fn prepared_abort_returns_original_candidate_and_drop_preserves_committed_baseline() {
    let mut owner = Owner::new(Budget::default());
    let mut initial = owner.begin().unwrap();
    let old = initial.number(1.0).unwrap();
    owner.commit(initial).unwrap();
    let baseline = owner.usage();
    let mut candidate = owner.begin().unwrap();
    let next = candidate.number(2.0).unwrap();
    let returned = owner.prepare_publish(candidate).unwrap().abort();
    assert_eq!(owner.number(&old), Ok(1.0));
    assert_eq!(owner.number(&next), Err(Error::StaleValue));
    assert_eq!(returned.read_number(&next), Ok(2.0));
    owner.prepare_publish(returned).unwrap().publish();
    assert_eq!(owner.number(&next), Ok(2.0));
    let published_usage = owner.usage();
    assert!(published_usage.nodes > baseline.nodes);
    let mut discarded = owner.begin().unwrap();
    let gone = discarded.null().unwrap();
    drop(owner.prepare_publish(discarded).unwrap());
    assert_eq!(owner.usage(), published_usage);
    assert_eq!(owner.kind(&gone), Err(Error::StaleValue));
    assert_eq!(owner.number(&next), Ok(2.0));
    assert!(owner.begin().is_ok());
}

#[test]
fn prepare_rejection_returns_foreign_stale_and_poisoned_original_candidates() {
    let mut owner = Owner::new(Budget::default());
    let mut foreign = Owner::new(Budget::default());
    let mut candidate = foreign.begin().unwrap();
    let foreign_value = candidate.number(9.0).unwrap();
    let failure = owner.prepare_publish(candidate).err().unwrap();
    assert_eq!(failure.error, Error::ForeignOwner);
    assert_eq!(failure.candidate.read_number(&foreign_value), Ok(9.0));
    foreign.commit(failure.candidate).unwrap();
    let stale = owner.begin().unwrap();
    let mut winner = owner.begin().unwrap();
    winner.null().unwrap();
    owner.commit(winner).unwrap();
    let failure = owner.prepare_publish(stale).err().unwrap();
    assert_eq!(failure.error, Error::StaleTransaction);
    assert_eq!(failure.candidate.current(), Err(Error::StaleTransaction));
    let mut poisoned = owner.begin().unwrap();
    assert_eq!(
        poisoned.array(vec![foreign_value]).err(),
        Some(Error::ForeignOwner)
    );
    let failure = owner.prepare_publish(poisoned).err().unwrap();
    assert_eq!(failure.error, Error::ForeignOwner);
    assert_eq!(failure.candidate.failure, Some(Error::ForeignOwner));
    assert!(!owner.identity.publishing.get());
}

#[test]
fn preparation_overflow_and_closed_owner_fail_before_any_publication() {
    let mut owner = Owner::new(Budget::default());
    let mut candidate = owner.begin().unwrap();
    let staged = candidate.number(4.0).unwrap();
    owner.identity.revision.set(u64::MAX);
    candidate.baseline = u64::MAX;
    let failure = owner.prepare_publish(candidate).err().unwrap();
    assert_eq!(failure.error, Error::Overflow);
    assert_eq!(failure.candidate.read_number(&staged), Ok(4.0));
    assert_eq!(owner.usage(), Usage::default());
    assert_eq!(owner.number(&staged), Err(Error::StaleValue));
    assert!(!owner.identity.publishing.get());
    owner.close();
    let failure = owner.prepare_publish(failure.candidate).err().unwrap();
    assert_eq!(failure.error, Error::Closed);
    assert_eq!(failure.candidate.current(), Err(Error::Closed));
}

#[test]
fn empty_prepared_publication_preserves_revision_and_does_not_invalidate_peer() {
    let mut owner = Owner::new(Budget::default());
    let peer = owner.begin().unwrap();
    let empty = owner.begin().unwrap();
    let receipt = owner.prepare_publish(empty).unwrap().publish();
    assert!(!receipt.changed);
    assert_eq!(receipt.revision, 0);
    assert_eq!(receipt.usage, Usage::default());
    assert_eq!(peer.current(), Ok(()));
    owner.commit(peer).unwrap();
    assert_eq!(owner.identity.candidates.get(), 0);
}

#[test]
fn heap_guard_unwind_releases_barrier_without_claiming_renderer_recovery() {
    let mut owner = Owner::new(Budget::default());
    let mut candidate = owner.begin().unwrap();
    let staged = candidate.boolean(true).unwrap();
    let identity = owner.identity.clone();
    let mut external_side_effect = false;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = owner.prepare_publish(candidate).unwrap();
        external_side_effect = true;
        panic!("model renderer unwound after a side effect");
    }));
    assert!(result.is_err());
    assert!(
        external_side_effect,
        "heap guard must not claim external rollback"
    );
    assert_eq!(owner.usage(), Usage::default());
    assert_eq!(owner.kind(&staged), Err(Error::StaleValue));
    assert_eq!(identity.reserved.get(), Usage::default());
    assert_eq!(identity.candidates.get(), 0);
    assert!(!identity.publishing.get());
    assert!(owner.begin().is_ok());
}

#[test]
fn staged_projection_reads_nested_aliases_without_publishing_or_allocating() {
    let mut owner = Owner::new(Budget::default());
    let mut original = owner.begin().unwrap();
    let base = original.string("base".into()).unwrap();
    owner.commit(original).unwrap();
    let baseline = owner.usage();
    let mut candidate = owner.begin().unwrap();
    let number = candidate.number(42.0).unwrap();
    let boolean = candidate.boolean(true).unwrap();
    let text = candidate.string("staged".into()).unwrap();
    let null = candidate.null().unwrap();
    let undefined = candidate.undefined().unwrap();
    let array = candidate
        .array(vec![text.clone(), text.clone(), base.clone()])
        .unwrap();
    let root = candidate
        .record(vec![("items".into(), array.clone())])
        .unwrap();
    let reserved = candidate.reserved;
    let sequence = candidate.identity.sequence.get();
    assert_eq!(candidate.kind(&null).unwrap(), ValueKind::Null);
    assert_eq!(candidate.kind(&undefined).unwrap(), ValueKind::Undefined);
    assert_eq!(candidate.read_number(&number).unwrap(), 42.0);
    assert!(candidate.read_boolean(&boolean).unwrap());
    assert_eq!(candidate.read_text(&text).unwrap(), "staged");
    let items = candidate.field(&root, "items").unwrap().unwrap();
    let first = candidate.element(&items, 0).unwrap().unwrap();
    let second = candidate.element(&items, 1).unwrap().unwrap();
    assert!(candidate.same_value(&first, &second).unwrap());
    assert_eq!(
        candidate
            .read_text(&candidate.element(&items, 2).unwrap().unwrap())
            .unwrap(),
        "base"
    );
    assert_eq!(owner.kind(&root), Err(Error::StaleValue));
    assert_eq!(owner.usage(), baseline);
    assert_eq!(candidate.reserved, reserved);
    assert_eq!(candidate.identity.sequence.get(), sequence);
    owner.commit(candidate).unwrap();
    let committed_items = owner.field(&root, "items").unwrap().unwrap();
    assert!(owner.same_value(&committed_items, &array).unwrap());
    assert_eq!(
        owner
            .text(&owner.element(&committed_items, 0).unwrap().unwrap())
            .unwrap(),
        "staged"
    );
}

#[test]
fn readonly_shape_errors_do_not_poison_candidate_or_change_missing_value_semantics() {
    let mut owner = Owner::new(Budget::default());
    let mut candidate = owner.begin().unwrap();
    let null = candidate.null().unwrap();
    fails(candidate.read_number(&null), Error::WrongKind);
    fails(candidate.read_boolean(&null), Error::WrongKind);
    fails(candidate.read_text(&null), Error::WrongKind);
    fails(candidate.field(&null, "missing"), Error::WrongKind);
    fails(candidate.element(&null, 0), Error::WrongKind);
    let record = candidate.record(vec![]).unwrap();
    let array = candidate.array(vec![]).unwrap();
    assert!(candidate.field(&record, "missing").unwrap().is_none());
    assert!(candidate.element(&array, usize::MAX).unwrap().is_none());
    owner.commit(candidate).unwrap();
    fails(owner.number(&null), Error::WrongKind);
    fails(owner.boolean(&null), Error::WrongKind);
    fails(owner.text(&null), Error::WrongKind);
    fails(owner.field(&null, "missing"), Error::WrongKind);
    fails(owner.element(&null, 0), Error::WrongKind);
    assert!(owner.field(&record, "missing").unwrap().is_none());
    assert!(owner.element(&array, usize::MAX).unwrap().is_none());
}

#[test]
fn every_staged_reader_rejects_foreign_values_without_poisoning_valid_reads() {
    let mut foreign = Owner::new(Budget::default());
    let mut tx = foreign.begin().unwrap();
    let other = tx.null().unwrap();
    foreign.commit(tx).unwrap();
    let owner = Owner::new(Budget::default());
    let mut candidate = owner.begin().unwrap();
    let own = candidate.string("own".into()).unwrap();
    readers_fail(&candidate, &other, Error::ForeignOwner);
    assert_eq!(candidate.read_text(&own).unwrap(), "own");
}

#[test]
fn every_staged_reader_rejects_abandoned_values_even_from_the_same_owner() {
    let owner = Owner::new(Budget::default());
    let mut abandoned = owner.begin().unwrap();
    let escaped = abandoned.string("not published".into()).unwrap();
    drop(abandoned);
    let candidate = owner.begin().unwrap();
    readers_fail(&candidate, &escaped, Error::StaleValue);
}

#[test]
fn staged_readers_observe_original_closed_poison_and_stale_transaction_guards() {
    let mut owner = Owner::new(Budget::default());
    let mut losing = owner.begin().unwrap();
    let value = losing.null().unwrap();
    let mut winner = owner.begin().unwrap();
    winner.null().unwrap();
    owner.commit(winner).unwrap();
    readers_fail(&losing, &value, Error::StaleTransaction);
    drop(losing);

    let mut closed = owner.begin().unwrap();
    let closed_value = closed.null().unwrap();
    owner.close();
    readers_fail(&closed, &closed_value, Error::Closed);

    let poisoned_owner = Owner::new(Budget::new(1, 1, 1, 1).unwrap());
    let mut poisoned = poisoned_owner.begin().unwrap();
    let value = poisoned.null().unwrap();
    fails(poisoned.null(), Error::BudgetExceeded);
    readers_fail(&poisoned, &value, Error::BudgetExceeded);
}

#[test]
fn failed_staged_projection_discards_candidate_and_keeps_committed_readback() {
    fn render(candidate: &Transaction, root: &Value) -> Result<String, Error> {
        let label = candidate.field(root, "label")?.ok_or(Error::WrongKind)?;
        Ok(candidate.read_text(&label)?.to_owned())
    }
    let mut owner = Owner::new(Budget::default());
    let mut original = owner.begin().unwrap();
    let label = original.string("original".into()).unwrap();
    let root = original
        .record(vec![("label".into(), label.clone())])
        .unwrap();
    owner.commit(original).unwrap();
    let baseline = owner.usage();
    let mut candidate = owner.begin().unwrap();
    let incompatible = candidate.number(1.0).unwrap();
    let replacement = candidate
        .record(vec![("label".into(), incompatible)])
        .unwrap();
    fails(render(&candidate, &replacement), Error::WrongKind);
    drop(candidate);
    assert_eq!(owner.usage(), baseline);
    assert_eq!(owner.kind(&replacement), Err(Error::StaleValue));
    assert_eq!(
        owner
            .text(&owner.field(&root, "label").unwrap().unwrap())
            .unwrap(),
        "original"
    );
    assert_eq!(owner.identity.reserved.get(), Usage::default());
}

#[test]
fn aliases_and_deep_equal_allocations_have_different_object_semantics() {
    let mut owner = Owner::new(Budget::default());
    let mut tx = owner.begin().unwrap();
    let one = tx.number(1.0).unwrap();
    let left = tx.record(vec![("count".into(), one.clone())]).unwrap();
    let right = tx.record(vec![("count".into(), one.clone())]).unwrap();
    let array = tx.array(vec![left.clone(), left.clone()]).unwrap();
    assert!(tx.same_value(&left, &left.clone()).unwrap());
    assert!(!tx.same_value(&left, &right).unwrap());
    assert_eq!(owner.kind(&left), Err(Error::StaleValue));
    owner.commit(tx).unwrap();
    assert!(!owner.same_value(&left, &right).unwrap());
    assert!(
        owner
            .same_value(
                &owner.element(&array, 0).unwrap().unwrap(),
                &owner.element(&array, 1).unwrap().unwrap()
            )
            .unwrap()
    );
    assert_eq!(
        owner
            .number(&owner.field(&left, "count").unwrap().unwrap())
            .unwrap(),
        1.0
    );
    assert!(owner.field(&left, "missing").unwrap().is_none());
}
