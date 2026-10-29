use super::*;
#[test]
fn dropping_a_reservation_releases_capacity_without_reusing_its_generation() {
    let mut registry = SurfaceRegistry::default();
    let old = registry.reserve(1).unwrap();
    let generation = old.generation();
    assert!(matches!(registry.reserve(1), Err(SurfaceError::Duplicate)));
    drop(old);
    assert!(registry.reserve(1).unwrap().generation() > generation);
}
#[test]
fn foreign_issuer_and_exhaustion_fail_closed() {
    let mut a = SurfaceRegistry::default();
    let reservation = a.reserve(1).unwrap();
    let b = SurfaceRegistry::default();
    assert!(b.validate(&reservation).is_err());
    a.next = u64::MAX;
    assert!(matches!(a.reserve(2), Err(SurfaceError::CounterExhausted)));
    a.layout = u64::MAX;
    assert!(matches!(
        a.next_layout(&reservation),
        Err(SurfaceError::CounterExhausted)
    ));
}
#[test]
fn layout_observations_have_their_own_monotonic_revision() {
    let mut registry = SurfaceRegistry::default();
    let reservation = registry.reserve(1).unwrap();
    assert_eq!(registry.next_layout(&reservation).unwrap(), 1);
    assert_eq!(registry.next_layout(&reservation).unwrap(), 2);
}
#[test]
fn capacity_is_bounded_and_dropped_owners_are_reclaimed() {
    let mut registry = SurfaceRegistry::default();
    let mut owners = (0..64)
        .map(|node| registry.reserve(node).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(registry.reserve(65), Err(SurfaceError::Capacity)));
    owners.pop();
    assert!(registry.reserve(65).is_ok());
}
