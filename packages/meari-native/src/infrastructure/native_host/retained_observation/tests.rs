use super::*;
use crate::{domain::Viewport, infrastructure::document::InteractiveDocument};

fn mounted() -> NativeDocumentHandle {
    let component = meari::parse(
        "export function Root(){return <button>go</button>}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    NativeDocumentHandle::new(
        InteractiveDocument::new(
            component,
            "",
            Viewport {
                width: 320,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap()
}
fn commit(handle: &NativeDocumentHandle) -> RetainedCommitProof {
    let (_, receipt) = handle
        .try_reproject(&handle.stamp().unwrap(), |_| Ok::<_, String>(()))
        .unwrap();
    handle.retain_commit(&receipt).unwrap()
}

#[test]
fn input_and_turn_advance_without_rewriting_original_commit() {
    let handle = mounted();
    let proof = commit(&handle);
    let original = proof.receipt().clone();
    handle
        .dispatch_action(crate::domain::widgets::Action::Activate { path: "0".into() })
        .unwrap();
    handle.next_turn().unwrap();
    let current = handle.validate_retained_commit(&proof).unwrap();
    assert!(current.document_revision > original.after().document_revision);
    assert!(current.turn_sequence > original.after().turn_sequence);
    assert_eq!(proof.receipt(), &original);
    assert!(matches!(
        handle.retain_commit(&original),
        Err(HostAccessError::Stale)
    ));
}

#[test]
fn same_value_new_commit_and_foreign_owner_reject_original_proof() {
    let handle = mounted();
    let proof = commit(&handle);
    assert!(matches!(
        mounted().validate_retained_commit(&proof),
        Err(HostAccessError::Stale)
    ));
    let next = commit(&handle);
    assert!(matches!(
        handle.validate_retained_commit(&proof),
        Err(HostAccessError::Stale)
    ));
    handle.validate_retained_commit(&next).unwrap();
}

#[test]
fn partial_error_and_panic_retire_proof_without_claiming_rollback() {
    for panic in [false, true] {
        let handle = mounted();
        let proof = commit(&handle);
        let before = handle.stamp().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle.try_reproject(&before, |document| {
                document.last_error = Some("partial mutation".into());
                assert!(!panic, "injected apply panic");
                Err::<(), _>("apply failed")
            })
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert_eq!(handle.stamp().unwrap(), before);
        assert!(matches!(
            handle.validate_retained_commit(&proof),
            Err(HostAccessError::Stale)
        ));
        assert_eq!(
            handle
                .try_with(|document| document.last_error.clone())
                .unwrap()
                .as_deref(),
            Some("partial mutation")
        );
    }
}

#[test]
fn busy_before_mutable_entry_preserves_proof() {
    let handle = mounted();
    let proof = commit(&handle);
    let stamp = handle.stamp().unwrap();
    handle
        .try_with(|_| {
            assert!(matches!(
                handle.try_reproject(&stamp, |_| Ok::<_, String>(())),
                Err(super::super::HostUpdateError::Access(HostAccessError::Busy))
            ));
        })
        .unwrap();
    handle.validate_retained_commit(&proof).unwrap();
}

#[test]
fn frame_policy_rejects_retired_context_and_precommit_or_foreign_frames() {
    // Policy injection only: this test does not execute a renderer or OS loop.
    let handle = mounted();
    let active = Rc::new(Cell::new(true));
    let ticket = FrameTicket {
        context_id: "policy-test".into(),
        context_generation: 1,
        scene_generation: 1,
    };
    let before = handle
        .checked_frame_proof(&handle.stamp().unwrap(), &ticket, &active)
        .unwrap();
    let proof = commit(&handle);
    assert!(matches!(
        handle.validate_retained_frame(&proof, &before),
        Err(HostAccessError::Stale)
    ));
    let frame = handle
        .checked_frame_proof(&handle.stamp().unwrap(), &ticket, &active)
        .unwrap();
    handle.validate_retained_frame(&proof, &frame).unwrap();
    handle
        .validate_retained_frame(&proof, &frame.clone())
        .unwrap();
    let foreign = mounted();
    let foreign_frame = foreign
        .checked_frame_proof(&foreign.stamp().unwrap(), &ticket, &active)
        .unwrap();
    assert!(matches!(
        handle.validate_retained_frame(&proof, &foreign_frame),
        Err(HostAccessError::Stale)
    ));
    active.set(false);
    assert!(matches!(
        handle.validate_retained_frame(&proof, &frame),
        Err(HostAccessError::Stale)
    ));
}

#[test]
fn closed_forced_and_poisoned_documents_reject_proof() {
    for forced in [false, true] {
        let handle = mounted();
        let proof = commit(&handle);
        if forced {
            handle.finish_close(true).unwrap();
        } else {
            handle
                .with_internal_mut(|document| document.source_input_poisoned = true)
                .unwrap();
        }
        assert!(handle.validate_retained_commit(&proof).is_err());
    }
    let handle = mounted();
    let proof = commit(&handle);
    handle.begin_close(0).unwrap();
    assert!(matches!(
        handle.validate_retained_commit(&proof),
        Err(HostAccessError::Closing)
    ));
}

#[test]
fn input_mutation_failure_retires_lineage_and_success_keeps_it() {
    let handle = mounted();
    let proof = commit(&handle);
    handle
        .try_reproject_kind(&handle.stamp().unwrap(), MutationKind::Input, |_| {
            Ok::<_, String>(())
        })
        .unwrap();
    handle.validate_retained_commit(&proof).unwrap();
    let failed =
        handle.try_reproject_kind(&handle.stamp().unwrap(), MutationKind::Input, |document| {
            document.last_error = Some("input partial".into());
            Err::<(), _>("input apply failed")
        });
    assert!(failed.is_err());
    assert!(handle.validate_retained_commit(&proof).is_err());
}

#[test]
fn real_hook_input_keeps_lineage_but_explicit_same_props_update_retires_it() {
    use meari::{
        application::hook_values::Budget,
        domain::{Value, build::BuildPath},
    };
    use std::collections::BTreeMap;
    let path = BuildPath::try_from("root.tsx".to_owned()).unwrap();
    let source = "import {useState} from 'react';type Props={label:string};export function Root(props:Props){const [state,setState]=useState(false);return <button onClick={()=>setState(true)}>{props.label}</button>}";
    let graph =
        meari::parse_module_graph(&path, &BTreeMap::from([(path.clone(), source.into())])).unwrap();
    let props = BTreeMap::from([("label".into(), Value::String("go".into()))]);
    let handle = NativeDocumentHandle::mount_hook_modules(
        graph,
        props.clone(),
        ("retained-hooks".into(), 1),
        "",
        Viewport {
            width: 320,
            height: 240,
            dark: false,
        },
        1,
        Budget::default(),
    )
    .unwrap();
    let proof = commit(&handle);
    assert!(
        handle
            .dispatch_action(crate::domain::widgets::Action::Activate { path: "0".into() })
            .unwrap()
    );
    handle.validate_retained_commit(&proof).unwrap();
    assert!(handle.update_hook_props(BTreeMap::new()).is_err());
    handle.validate_retained_commit(&proof).unwrap();
    handle.update_hook_props(props).unwrap();
    assert!(matches!(
        handle.validate_retained_commit(&proof),
        Err(HostAccessError::Stale)
    ));
}
