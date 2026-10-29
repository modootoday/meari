use super::*;
use meari_native::infrastructure::{document::InteractiveDocument, native_host::*};

struct Host {
    document: NativeDocumentHandle,
    permit: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
    step: Rc<Cell<usize>>,
    fail_once: bool,
    refuse_after_revoke: bool,
}
impl NativeWindowHost for Host {
    type Error = io::Error;
    type CloseCheckpoint = bool;
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<io::Error> {
        IntentAdmission::Rejected(io::Error::other("no business callbacks in cleanup fixture"))
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, io::Error> {
        Ok(HostEffects::default())
    }
    fn can_close(&mut self, _: &HostStamp, _: CloseCause) -> Result<bool, io::Error> {
        Ok(self.permit.get())
    }
    fn close(&mut self, request: CloseRequest<bool>) -> CloseDecision<bool, io::Error> {
        self.document.try_with(|_| ()).unwrap();
        assert!(matches!(
            self.document.lifecycle(),
            HostLifecycle::Closing | HostLifecycle::CleanupFailed
        ));
        self.calls.set(self.calls.get() + 1);
        if self.refuse_after_revoke {
            return CloseDecision::KeepOpen;
        }
        let complete_step = request.checkpoint.unwrap_or(false);
        if !complete_step {
            self.step.set(self.step.get() + 1);
        }
        if self.fail_once {
            self.fail_once = false;
            CloseDecision::RetryableFailure {
                checkpoint: true,
                error: io::Error::other("host cleanup retry"),
            }
        } else {
            CloseDecision::HostCleanupComplete { checkpoint: true }
        }
    }
}
type Runtime = NativeHostRuntime<ChildWindowHost<Host, Child>>;
type HostFixture = (
    NativeDocumentHandle,
    Runtime,
    Rc<Cell<usize>>,
    Rc<Cell<usize>>,
    Rc<Cell<bool>>,
);
fn runtime(owner: &Owner, fail_once: bool, refuse: bool) -> HostFixture {
    let source = meari::parse(
        "type Props={};export function View(props:Props){return <main/>;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "",
        meari_native::domain::Viewport {
            width: 320,
            height: 240,
            dark: false,
        },
    )
    .unwrap();
    let handle = NativeDocumentHandle::new(document, 1).unwrap();
    let calls = Rc::new(Cell::new(0));
    let step = Rc::new(Cell::new(0));
    let permit = Rc::new(Cell::new(true));
    let host = Host {
        document: handle.clone(),
        permit: permit.clone(),
        calls: calls.clone(),
        step: step.clone(),
        fail_once,
        refuse_after_revoke: refuse,
    };
    let runtime = NativeHostRuntime::new(handle.clone(), ChildWindowHost::new(host, owner.clone()));
    (handle, runtime, calls, step, permit)
}

#[test]
fn child_pending_then_host_retry_preserves_each_completed_step() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let child = control(Mode::Pending);
    let lease = register(&owner, &child);
    let (document, mut runtime, calls, step, permit) = runtime(&owner, true, false);
    permit.set(false);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::KeepOpen
    );
    assert_eq!(child.borrow().starts, 0);
    owner.validate_access(&lease).unwrap();
    permit.set(true);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(document.lifecycle(), HostLifecycle::Closing);
    assert!(document.try_with(|_| ()).is_ok());
    finish(&child);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(step.get(), 1);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(step.get(), 1);
    assert_eq!(child.borrow().starts, 1);
    runtime.finish_headless_close().unwrap();
    // Headless ownership simulation only; the real driver must invoke this after actual removal.
    owner.window_removed().unwrap();
    assert_eq!(document.lifecycle(), HostLifecycle::Closed);
}

#[test]
fn completed_child_checkpoint_cannot_bypass_changed_window_lifetime() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    register(&owner, &control(Mode::Complete));
    let (_, mut runtime, calls, _, _) = runtime(&owner, true, false);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(calls.get(), 1);
    owner.force_loss().unwrap();
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert!(matches!(
        runtime.last_host_error(),
        Some(ChildHostError::Lifecycle(LifecycleError::Stale))
    ));
    assert_eq!(calls.get(), 1);
}

#[test]
fn foreign_checkpoint_never_starts_another_owners_child_or_business_cleanup() {
    let (first, _, _) = owner(1);
    ready(&first);
    register(&first, &control(Mode::Pending));
    let (document, mut source, _, _, _) = runtime(&first, false, false);
    let stamp = document.stamp().unwrap();
    let checkpoint = match NativeWindowHost::close(
        source.host_mut(),
        CloseRequest {
            stamp: stamp.clone(),
            cause: CloseCause::User,
            callbacks: CallbackCloseReceipt {
                stamp: stamp.clone(),
                discarded_callbacks: 0,
            },
            checkpoint: None,
        },
    ) {
        CloseDecision::Pending { checkpoint } => checkpoint,
        _ => panic!("technical child must remain pending"),
    };
    let (second, _, _) = owner(1);
    ready(&second);
    let child = control(Mode::Complete);
    let lease = register(&second, &child);
    let (_, mut target, calls, _, _) = runtime(&second, false, false);
    let result = NativeWindowHost::close(
        target.host_mut(),
        CloseRequest {
            stamp: stamp.clone(),
            cause: CloseCause::User,
            callbacks: CallbackCloseReceipt {
                stamp,
                discarded_callbacks: 0,
            },
            checkpoint: Some(checkpoint),
        },
    );
    assert!(matches!(
        result,
        CloseDecision::RetryableFailure {
            error: ChildHostError::Lifecycle(LifecycleError::Stale),
            ..
        }
    ));
    assert_eq!(child.borrow().starts, 0);
    assert_eq!(calls.get(), 0);
    second.validate_access(&lease).unwrap();
}

#[test]
fn changed_revocation_attempt_rejects_old_checkpoint_without_duplicate_teardown() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let child = control(Mode::FailOnce);
    register(&owner, &child);
    let (_, mut runtime, calls, _, _) = runtime(&owner, false, false);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    let before = owner.barrier_stamp().unwrap();
    owner.retry_failed().unwrap();
    let after = owner.barrier_stamp().unwrap();
    assert_ne!(before.revocation(), after.revocation());
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert!(matches!(
        runtime.last_host_error(),
        Some(ChildHostError::Lifecycle(LifecycleError::Stale))
    ));
    assert_eq!(child.borrow().starts, 2);
    assert_eq!(calls.get(), 0);
}

#[test]
fn keep_open_after_irreversible_cleanup_is_retained_protocol_failure_without_replay() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let child = control(Mode::Complete);
    let lease = register(&owner, &child);
    let (document, mut runtime, calls, _, _) = runtime(&owner, false, true);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(document.lifecycle(), HostLifecycle::CleanupFailed);
    assert_eq!(owner.phase().unwrap(), WindowPhase::Closing);
    assert_eq!(owner.validate_access(&lease), Err(LifecycleError::Stale));
    assert!(matches!(
        runtime.last_host_error(),
        Some(ChildHostError::HostKeptOpenAfterRevocation)
    ));
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(child.borrow().starts, 1);
    assert_eq!(
        runtime.request_close(CloseCause::ForcedOsDestroy).unwrap(),
        HostCloseStatus::ForcedIncomplete
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(document.lifecycle(), HostLifecycle::ForcedIncomplete);
}

#[test]
fn forced_loss_retains_residual_children_and_does_not_certify_business_cleanup() {
    let (owner, _, _) = owner(1);
    ready(&owner);
    let child = control(Mode::Pending);
    register(&owner, &child);
    let (document, mut runtime, calls, _, _) = runtime(&owner, false, false);
    assert_eq!(
        runtime.request_close(CloseCause::ForcedOsDestroy).unwrap(),
        HostCloseStatus::ForcedIncomplete
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(owner.report().unwrap().remaining, 1);
    assert_eq!(document.lifecycle(), HostLifecycle::ForcedIncomplete);
    finish(&child);
    assert_eq!(owner.poll().unwrap().remaining, 0);
    assert_eq!(owner.phase().unwrap(), WindowPhase::ForcedIncomplete);
    assert_eq!(calls.get(), 0);
}
