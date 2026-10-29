//! Public contract checks. No window or renderer submission is simulated as real.
use meari::application::host_callbacks::PendingCallback;
use meari_native::{
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*},
};
use std::io;

struct Consumer {
    stale: Option<HostCommitReceipt>,
}
impl NativeWindowHost for Consumer {
    type Error = io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        IntentAdmission::Rejected(io::Error::other("unexpected callback"))
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Self::Error> {
        Ok(HostEffects {
            committed: self.stale.clone(),
            redraw: false,
        })
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Self::Error> {
        CloseDecision::KeepOpen
    }
}
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
#[test]
fn actual_runtime_input_and_completion_keep_original_proof_and_receipt() {
    let handle = mounted();
    let (_, receipt) = handle
        .try_reproject(&handle.stamp().unwrap(), |_| Ok::<_, String>(()))
        .unwrap();
    let proof = handle.retain_commit(&receipt).unwrap();
    let mut runtime = NativeHostRuntime::new(handle.clone(), Consumer { stale: None });
    handle
        .dispatch_action(meari_native::domain::widgets::Action::Activate { path: "0".into() })
        .unwrap();
    let turn = runtime.after_dispatch(HostTurnReason::Input).unwrap();
    assert!(turn.committed.is_none());
    assert!(turn.stamp.document_revision > receipt.after().document_revision);
    assert!(turn.stamp.turn_sequence > receipt.after().turn_sequence);
    runtime.after_dispatch(HostTurnReason::Completion).unwrap();
    handle.validate_retained_commit(&proof).unwrap();
    assert_eq!(proof.receipt(), &receipt);
    assert!(matches!(
        handle.retain_commit(&receipt),
        Err(HostAccessError::Stale)
    ));
}

#[test]
fn retained_observation_does_not_relax_synchronous_runtime_receipt_gate() {
    let handle = mounted();
    let (_, receipt) = handle
        .try_reproject(&handle.stamp().unwrap(), |_| Ok::<_, String>(()))
        .unwrap();
    let proof = handle.retain_commit(&receipt).unwrap();
    let mut runtime = NativeHostRuntime::new(
        handle.clone(),
        Consumer {
            stale: Some(receipt),
        },
    );
    assert!(matches!(
        runtime.after_dispatch(HostTurnReason::Completion),
        Err(HostAccessError::Stale)
    ));
    handle.validate_retained_commit(&proof).unwrap();
}
