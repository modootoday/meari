use super::*;
use meari::application::host_callbacks::CallbackHandle;
use meari_native::infrastructure::native_host::*;
use std::{cell::Cell, collections::VecDeque, io};
#[cfg(all(windows, feature = "window"))]
#[path = "host_window.rs"]
mod window_test;

#[derive(Default)]
struct CloseProgress {
    session: bool,
    parent: bool,
}
struct Consumer {
    handle: NativeDocumentHandle,
    callbacks: HostCallbacks,
    registry: PluginRegistry,
    received: Rc<RefCell<Vec<serde_json::Value>>>,
    queue: VecDeque<(CallbackHandle, CallbackArguments)>,
    values: BTreeMap<String, Value>,
    completed: usize,
    accepted_per_pass: Vec<usize>,
    session_closes: Rc<Cell<usize>>,
    parent_closes: Rc<Cell<usize>>,
    fail_parent_once: bool,
    defer_close_once: bool,
    close_causes: Vec<CloseCause>,
}
impl NativeWindowHost for Consumer {
    type Error = io::Error;
    type CloseCheckpoint = CloseProgress;
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        callback: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        if !self.queue.is_empty() {
            return IntentAdmission::Capacity;
        }
        self.queue.push_back((
            callback.handle.clone(),
            callback.invocation.arguments.clone(),
        ));
        IntentAdmission::Accepted
    }
    fn after_dispatch(&mut self, turn: HostTurn) -> Result<HostEffects, Self::Error> {
        self.handle.try_with(|_| ()).map_err(io::Error::other)?;
        self.accepted_per_pass.push(turn.accepted_callbacks);
        while let Some((handle, arguments)) = self.queue.pop_front() {
            self.callbacks
                .dispatch(&handle, arguments, &mut self.registry)
                .map_err(|error| io::Error::other(error.message))?;
            self.completed += 1;
        }
        if turn.remaining_callbacks != 0 {
            assert!(matches!(
                self.handle
                    .try_reproject(&turn.stamp, |_| Ok::<_, io::Error>(())),
                Err(HostUpdateError::Access(HostAccessError::Busy))
            ));
            return Ok(HostEffects::default());
        }
        if turn.accepted_callbacks == 0 {
            return Ok(HostEffects::default());
        }
        self.values.insert(
            "resultLabel".into(),
            Value::String(format!("completed {}", self.completed)),
        );
        let export = self.callbacks.export_bindings().unwrap();
        let values = self.values.clone();
        let (_, receipt) = self
            .handle
            .try_reproject(&turn.stamp, |document| {
                document
                    .update_module_props_with_callbacks(values, export)
                    .map_err(io::Error::other)
            })
            .map_err(io::Error::other)?;
        Ok(HostEffects {
            committed: Some(receipt),
            redraw: true,
        })
    }
    fn close(
        &mut self,
        request: CloseRequest<Self::CloseCheckpoint>,
    ) -> CloseDecision<Self::CloseCheckpoint, Self::Error> {
        assert!(
            self.handle
                .try_with(|document| document.callback_scope().is_none())
                .unwrap()
        );
        let mut checkpoint = request.checkpoint.unwrap_or_default();
        self.close_causes.push(request.cause);
        if self.defer_close_once {
            self.defer_close_once = false;
            return CloseDecision::Pending { checkpoint };
        }
        if !checkpoint.session {
            self.session_closes.set(self.session_closes.get() + 1);
            checkpoint.session = true;
        }
        if self.fail_parent_once {
            self.fail_parent_once = false;
            return CloseDecision::RetryableFailure {
                checkpoint,
                error: io::Error::other("parent cleanup temporarily failed"),
            };
        }
        if !checkpoint.parent {
            self.parent_closes.set(self.parent_closes.get() + 1);
            checkpoint.parent = true;
        }
        CloseDecision::HostCleanupComplete { checkpoint }
    }
}
fn consumer() -> (NativeDocumentHandle, NativeHostRuntime<Consumer>, String) {
    let (document, host) = mount(CONVERT);
    let action = document
        .module_projection()
        .unwrap()
        .callback_bindings()
        .into_iter()
        .find(|(_, handler)| handler.reference.binding == "onConvert")
        .unwrap()
        .0;
    let handle = NativeDocumentHandle::new(document, 7).unwrap();
    let consumer = Consumer {
        handle: handle.clone(),
        callbacks: host.callbacks,
        registry: host.registry,
        received: host.received,
        queue: VecDeque::new(),
        values: props(CONVERT),
        completed: 0,
        accepted_per_pass: Vec::new(),
        session_closes: Rc::new(Cell::new(0)),
        parent_closes: Rc::new(Cell::new(0)),
        fail_parent_once: false,
        defer_close_once: false,
        close_causes: Vec::new(),
    };
    let runtime = NativeHostRuntime::new(handle.clone(), consumer);
    (handle, runtime, action)
}

#[test]
fn host_preserves_capacity_suffix_and_commits_consumer_reproject_exactly_once() {
    let (handle, mut runtime, action) = consumer();
    for _ in 0..2 {
        handle
            .dispatch_action(Action::Activate {
                path: action.clone(),
            })
            .unwrap();
    }
    assert!(runtime.host().received.borrow().is_empty());
    let first = runtime.after_dispatch(HostTurnReason::Input).unwrap();
    assert_eq!(first.accepted_callbacks, 1);
    assert_eq!(first.remaining_callbacks, 1);
    assert!(first.committed.is_none());
    assert_eq!(runtime.host().completed, 1);
    assert!(matches!(
        handle.dispatch_action(Action::Activate {
            path: action.clone()
        }),
        Err(HostAccessError::Busy)
    ));
    let resumed = runtime
        .after_dispatch(HostTurnReason::ResumePending)
        .unwrap();
    assert_eq!(resumed.accepted_callbacks, 2);
    assert_eq!(resumed.remaining_callbacks, 0);
    assert!(resumed.committed.is_some());
    assert_eq!(runtime.host().accepted_per_pass, [1, 1]);
    assert_eq!(runtime.host().received.borrow().len(), 2);
    assert_eq!(runtime.host().completed, 2);
    runtime.after_dispatch(HostTurnReason::Completion).unwrap();
    assert_eq!(runtime.host().completed, 2);
}

#[test]
fn host_handle_reports_reentrant_borrow_and_stale_update_without_running_apply() {
    let (handle, _runtime, _) = consumer();
    let stamp = handle.stamp().unwrap();
    let result = handle
        .try_with(|_| handle.try_reproject(&stamp, |_| Ok::<_, io::Error>(())))
        .unwrap();
    assert!(matches!(
        result,
        Err(HostUpdateError::Access(HostAccessError::Busy))
    ));
    let mut stale = stamp;
    stale.host_id += 1;
    let called = Cell::new(false);
    assert!(matches!(
        handle.try_reproject(&stale, |_| {
            called.set(true);
            Ok::<_, io::Error>(())
        }),
        Err(HostUpdateError::Access(HostAccessError::Stale))
    ));
    assert!(!called.get());
}

#[test]
fn host_cleanup_failure_keeps_document_and_retries_only_unfinished_parent_step() {
    let (handle, mut runtime, _) = consumer();
    runtime.host_mut().fail_parent_once = true;
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::RetryableFailure
    );
    assert_eq!(handle.lifecycle(), HostLifecycle::CleanupFailed);
    assert!(handle.try_with(|_| ()).is_ok());
    let stamp = handle.stamp().unwrap();
    assert!(matches!(
        handle.try_reproject(&stamp, |_| Ok::<_, io::Error>(())),
        Err(HostUpdateError::Access(HostAccessError::Closing))
    ));
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(runtime.host().session_closes.get(), 1);
    assert_eq!(runtime.host().parent_closes.get(), 1);
    assert_eq!(
        runtime.request_close(CloseCause::User).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    runtime.finish_headless_close().unwrap();
    assert_eq!(handle.lifecycle(), HostLifecycle::Closed);
    assert!(matches!(
        handle.try_with(|_| ()),
        Err(HostAccessError::Closed)
    ));
}

#[test]
fn dropped_runtime_is_forced_incomplete_without_claiming_sdk_cleanup_success() {
    let (handle, runtime, _) = consumer();
    let sessions = runtime.host().session_closes.clone();
    let parents = runtime.host().parent_closes.clone();
    drop(runtime);
    assert_eq!(handle.lifecycle(), HostLifecycle::ForcedIncomplete);
    assert_eq!(sessions.get(), 0);
    assert_eq!(parents.get(), 0);
    assert!(matches!(
        handle.try_with(|_| ()),
        Err(HostAccessError::Closed)
    ));
}

#[test]
fn invalid_reproject_keeps_stamp_last_good_export_and_real_callback_usable() {
    let (handle, mut runtime, action) = consumer();
    let before = handle.stamp().unwrap();
    let mut invalid = runtime.host().values.clone();
    invalid.insert("canConvert".into(), Value::String("invalid boolean".into()));
    let export = runtime.host_mut().callbacks.export_bindings().unwrap();
    assert!(matches!(
        handle.try_reproject(&before, |document| document
            .update_module_props_with_callbacks(invalid, export)),
        Err(HostUpdateError::Apply(_))
    ));
    assert_eq!(handle.stamp().unwrap(), before);
    handle
        .dispatch_action(Action::Activate { path: action })
        .unwrap();
    runtime.after_dispatch(HostTurnReason::Input).unwrap();
    assert_eq!(runtime.host().completed, 1);
}

#[test]
fn pending_close_preserves_first_cause_and_revokes_before_consumer_cleanup() {
    let (handle, mut runtime, _) = consumer();
    runtime.host_mut().defer_close_once = true;
    assert_eq!(
        runtime.request_close(CloseCause::SourceTransition).unwrap(),
        HostCloseStatus::Pending
    );
    assert_eq!(runtime.host().session_closes.get(), 0);
    assert!(
        handle
            .try_with(|document| document.callback_scope().is_none())
            .unwrap()
    );
    assert_eq!(
        runtime.request_close(CloseCause::Host).unwrap(),
        HostCloseStatus::HostCleanupComplete
    );
    assert_eq!(
        runtime.host().close_causes,
        [CloseCause::SourceTransition, CloseCause::SourceTransition]
    );
    runtime.finish_headless_close().unwrap();
}

#[test]
fn standalone_document_has_stable_independent_owner_and_closes_without_source_state() {
    let source = meari::parse(
        "type Props = {}; export function View(props: Props){return <button>Run</button>}",
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
    assert_eq!(handle.stamp().unwrap(), handle.stamp().unwrap());
    assert!(
        handle
            .try_with(|document| document.component_state().is_err())
            .unwrap()
    );
    drop(handle);
}
