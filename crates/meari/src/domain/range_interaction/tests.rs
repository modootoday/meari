use super::*;
#[test]
fn repeated_pointer_down_starts_new_interaction_instead_of_key_repeat_semantics() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    s.begin(7, RangeGesture::Pointer(1), "30").unwrap();
    assert!(
        s.finish(&7, &RangeGesture::Pointer(1), "30")
            .unwrap()
            .is_none()
    );
}
#[test]
fn pointer_commit_reads_actual_release_value_once() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    let value = s
        .finish(&7, &RangeGesture::Pointer(1), "70")
        .unwrap()
        .unwrap();
    assert_eq!((value.owner, value.value), (7, "70".into()));
    assert!(
        s.finish(&7, &RangeGesture::Pointer(1), "80")
            .unwrap()
            .is_none()
    );
}
#[test]
fn same_final_value_produces_no_commit() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    assert!(
        s.finish(&7, &RangeGesture::Pointer(1), "20")
            .unwrap()
            .is_none()
    );
}
#[test]
fn foreign_pointer_cannot_finish_original_interaction() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    assert!(
        s.finish(&7, &RangeGesture::Pointer(2), "50")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.finish(&7, &RangeGesture::Pointer(1), "30")
            .unwrap()
            .unwrap()
            .value,
        "30"
    );
}
#[test]
fn replaced_owner_cancels_old_release() {
    let mut s = RangeInteraction::default();
    s.begin((1, 2), RangeGesture::Pointer(1), "20").unwrap();
    assert!(
        s.finish(&(1, 3), &RangeGesture::Pointer(1), "40")
            .unwrap()
            .is_none()
    );
    assert!(
        s.finish(&(1, 2), &RangeGesture::Pointer(1), "40")
            .unwrap()
            .is_none()
    );
}
#[test]
fn key_repeat_preserves_initial_value_and_keyup_commits_once() {
    let mut s = RangeInteraction::default();
    let key = RangeGesture::Key("ArrowRight".into());
    s.begin(7, key.clone(), "20").unwrap();
    s.begin(7, key.clone(), "30").unwrap();
    assert!(
        s.finish(&7, &RangeGesture::Key("ArrowLeft".into()), "40")
            .unwrap()
            .is_none()
    );
    assert_eq!(s.finish(&7, &key, "40").unwrap().unwrap().value, "40");
    assert!(s.finish(&7, &key, "40").unwrap().is_none());
}
#[test]
fn cancellation_never_commits_or_claims_draft_rollback() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    s.cancel();
    assert!(
        s.finish(&7, &RangeGesture::Pointer(1), "90")
            .unwrap()
            .is_none()
    );
}
#[test]
fn distinct_interactions_are_not_globally_deduplicated_by_value() {
    let mut s = RangeInteraction::default();
    s.begin(7, RangeGesture::Pointer(1), "20").unwrap();
    let first = s
        .finish(&7, &RangeGesture::Pointer(1), "40")
        .unwrap()
        .unwrap();
    s.begin(7, RangeGesture::Pointer(1), "30").unwrap();
    let second = s
        .finish(&7, &RangeGesture::Pointer(1), "40")
        .unwrap()
        .unwrap();
    assert!(second.interaction > first.interaction);
}
#[test]
fn overflow_does_not_wrap_interaction_identity() {
    let mut s = RangeInteraction {
        next: u64::MAX,
        active: None,
    };
    assert!(s.begin(7, RangeGesture::Pointer(1), "20").is_err());
    assert!(
        s.finish(&7, &RangeGesture::Pointer(1), "40")
            .unwrap()
            .is_none()
    );
}
