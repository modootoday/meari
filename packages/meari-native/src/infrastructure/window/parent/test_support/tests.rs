use super::*;
use std::cell::RefCell;

#[test]
fn borrowed_observer_retains_notice_until_one_explicit_successful_retry() {
    let target = Rc::new(RefCell::new(Vec::new()));
    let observed = target.clone();
    let harness = ObserverDeliveryHarness::new(
        17,
        Box::new(move |notice| {
            assert!(notice.window.is_none());
            assert!(notice.frame.is_none());
            observed
                .try_borrow_mut()
                .map_err(|_| "delivery target borrowed".to_owned())?
                .push((notice.host_id, notice.revision, notice.kind));
            Ok(())
        }),
    );
    let borrow = target.borrow_mut();
    assert!(matches!(
        harness.publish_metadata(MetadataProbeKind::Created),
        Err(NativeNoticeError::Callback(_))
    ));
    let pending = harness.state().unwrap();
    assert!(pending.paused);
    assert_eq!(pending.pending.len(), 1);
    let identity = &pending.pending[0];
    assert_eq!(identity.host_id, 17);
    assert_eq!(identity.kind, NativeWindowNoticeKind::Created);
    assert!(matches!(harness.flush(), Err(NativeNoticeError::Paused)));
    assert_eq!(harness.state().unwrap(), pending);
    assert!(borrow.is_empty());
    drop(borrow);

    harness.retry_observer().unwrap();
    assert_eq!(
        target.borrow().as_slice(),
        &[(17, identity.revision, identity.kind)]
    );
    assert_eq!(
        harness.state().unwrap(),
        MetadataDeliveryState {
            paused: false,
            pending: vec![]
        }
    );
    harness.retry_observer().unwrap();
    harness.flush().unwrap();
    assert_eq!(target.borrow().len(), 1);
}

#[test]
fn paused_queue_replaces_same_kind_but_preserves_other_kind() {
    let target = Rc::new(RefCell::new(Vec::new()));
    let observed = target.clone();
    let harness = ObserverDeliveryHarness::new(
        19,
        Box::new(move |notice| {
            assert!(notice.window.is_none());
            assert!(notice.frame.is_none());
            observed
                .try_borrow_mut()
                .map_err(|_| "delivery target borrowed".to_owned())?
                .push((notice.revision, notice.kind));
            Ok(())
        }),
    );
    let borrow = target.borrow_mut();
    assert!(matches!(
        harness.publish_metadata(MetadataProbeKind::Created),
        Err(NativeNoticeError::Callback(_))
    ));
    let first = harness.state().unwrap().pending[0].revision;
    assert!(matches!(
        harness.publish_metadata(MetadataProbeKind::GeometryChanged),
        Err(NativeNoticeError::Paused)
    ));
    assert!(matches!(
        harness.publish_metadata(MetadataProbeKind::Created),
        Err(NativeNoticeError::Paused)
    ));
    let pending = harness.state().unwrap();
    assert!(pending.paused);
    assert_eq!(pending.pending.len(), 2);
    assert!(pending.pending.iter().all(|notice| notice.revision > first));
    assert_eq!(
        pending.pending[0].kind,
        NativeWindowNoticeKind::GeometryChanged
    );
    assert_eq!(pending.pending[1].kind, NativeWindowNoticeKind::Created);
    assert!(borrow.is_empty());
    drop(borrow);

    harness.retry_observer().unwrap();
    let expected = pending
        .pending
        .iter()
        .map(|notice| (notice.revision, notice.kind))
        .collect::<Vec<_>>();
    assert_eq!(*target.borrow(), expected);
    assert!(harness.state().unwrap().pending.is_empty());
    harness.retry_observer().unwrap();
    assert_eq!(target.borrow().len(), 2);
}
