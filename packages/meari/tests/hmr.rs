use meari::application::hmr::{Artifact, Coordinator, Target};

fn artifact(hash: &str) -> Artifact {
    Artifact {
        source_hash: hash.into(),
        artifact_id: "opaque-target-artifact".into(),
        state_schema: "v1".into(),
    }
}

#[test]
fn same_revision_requires_both_prepares_and_real_frame_acknowledgements() {
    let mut coordinator = Coordinator::default();
    let stale = coordinator.request("old".into()).unwrap();
    let revision = coordinator.request("new".into()).unwrap();
    assert!(
        coordinator
            .prepare(stale, Target::Web, artifact("old"))
            .is_err()
    );
    assert!(
        coordinator
            .prepare(revision, Target::Web, artifact("wrong"))
            .is_err()
    );
    coordinator
        .prepare(revision, Target::Web, artifact("new"))
        .unwrap();
    assert!(coordinator.publish(revision).is_err());
    coordinator
        .prepare(revision, Target::Native, artifact("new"))
        .unwrap();
    let publication = coordinator.publish(revision).unwrap();
    assert_eq!(publication.artifacts.len(), 2);
    assert!(coordinator.last_good().is_none());
    assert!(coordinator.request("queued".into()).is_err());
    assert!(!coordinator.acknowledge(revision, Target::Web).unwrap());
    assert!(coordinator.acknowledge(revision, Target::Web).is_err());
    assert!(coordinator.acknowledge(revision, Target::Native).unwrap());
    assert_eq!(coordinator.last_good().unwrap().revision, revision);
}

#[test]
fn one_host_failure_requires_two_rollback_acknowledgements_before_recovery() {
    let mut coordinator = Coordinator::default();
    let good = coordinator.request("good".into()).unwrap();
    for target in [Target::Native, Target::Web] {
        coordinator.prepare(good, target, artifact("good")).unwrap();
    }
    coordinator.publish(good).unwrap();
    coordinator.acknowledge(good, Target::Native).unwrap();
    coordinator.acknowledge(good, Target::Web).unwrap();
    let failed = coordinator.request("bad".into()).unwrap();
    for target in [Target::Native, Target::Web] {
        coordinator
            .prepare(failed, target, artifact("bad"))
            .unwrap();
    }
    coordinator.publish(failed).unwrap();
    coordinator.acknowledge(failed, Target::Web).unwrap();
    assert_eq!(coordinator.fail(failed).unwrap().unwrap().revision, good);
    assert!(coordinator.acknowledge(failed, Target::Native).is_err());
    assert!(coordinator.request("recover".into()).is_err());
    assert!(
        !coordinator
            .acknowledge_rollback(failed, Target::Web)
            .unwrap()
    );
    assert!(
        coordinator
            .acknowledge_rollback(failed, Target::Native)
            .unwrap()
    );
    let next = coordinator.request("recover".into()).unwrap();
    assert!(next > failed);
    assert_eq!(coordinator.last_good().unwrap().revision, good);
}
