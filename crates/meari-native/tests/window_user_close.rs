//! Original runtime close guard/checkpoint tests; no OS window or proxy is initialized.
use meari_native::{
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*},
};
struct Consumer {
    allowed: bool,
    causes: Vec<CloseCause>,
    closes: usize,
    observations: usize,
}
impl NativeWindowHost for Consumer {
    type Error = std::io::Error;
    type CloseCheckpoint = u8;
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        IntentAdmission::Capacity
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Self::Error> {
        Ok(Default::default())
    }
    fn can_close(&mut self, _: &HostStamp, cause: CloseCause) -> Result<bool, Self::Error> {
        self.causes.push(cause);
        Ok(cause != CloseCause::User || self.allowed)
    }
    fn close(&mut self, request: CloseRequest<u8>) -> CloseDecision<u8, Self::Error> {
        assert!(request.checkpoint.is_none());
        self.closes += 1;
        CloseDecision::Pending { checkpoint: 7 }
    }
    fn observe_close(&mut self, request: CloseRequest<u8>) -> CloseDecision<u8, Self::Error> {
        assert_eq!(request.checkpoint, Some(7));
        self.observations += 1;
        CloseDecision::HostCleanupComplete { checkpoint: 7 }
    }
}
fn runtime() -> NativeHostRuntime<Consumer> {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>close</main>}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "",
        Viewport {
            width: 320,
            height: 240,
            dark: false,
        },
    )
    .unwrap();
    let handle = NativeDocumentHandle::new(document, 1).unwrap();
    NativeHostRuntime::new(
        handle,
        Consumer {
            allowed: false,
            causes: vec![],
            closes: 0,
            observations: 0,
        },
    )
}
#[test]
fn user_close_veto_keeps_owner_open_then_allowed_close_observes_original_checkpoint_once() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::KeepOpen
    );
    assert_eq!(runtime.document().lifecycle(), HostLifecycle::Open);
    assert_eq!(runtime.host().closes, 0);
    runtime.host_mut().allowed = true;
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(runtime.host().closes, 1);
    assert_eq!(
        runtime.observe_close(CloseCause::User).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(runtime.host().observations, 1);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(runtime.host().closes, 1);
    assert_eq!(
        runtime.host().causes,
        vec![CloseCause::User, CloseCause::User]
    );
}
#[test]
fn explicit_host_close_retains_original_programmatic_cause_instead_of_user_veto() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(runtime.host().causes, vec![CloseCause::Host]);
    assert_eq!(
        runtime.observe_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(runtime.host().closes, 1);
}
