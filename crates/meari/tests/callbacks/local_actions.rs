//! Real Core exports/queue/registry; Native DOM and controller borrowing remain consumer gates.
use meari::{
    application::{Language, host_callbacks::*, plugins::*, state_program::StateProgram},
    domain::{
        Span,
        build::{ContentHash, TargetKind},
        host_callbacks::*,
    },
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

fn scope(document: &str, generation: u64) -> PluginScope {
    let component = meari::parse(
        "type Props={};export function Local(props:Props){return <main/>;}",
        Language::Tsx,
    )
    .unwrap();
    let program = StateProgram::mount(component, document.into(), 1).unwrap();
    PluginScope {
        instance: program.snapshot().unwrap().instance,
        source_generation: generation,
    }
}
fn reference(binding: &str, signature: CallbackSignature) -> CallbackReference {
    CallbackReference {
        binding: binding.into(),
        signature,
    }
}
fn handler(binding: &str, signature: CallbackSignature, source: CallbackSource) -> BoundCallback {
    BoundCallback {
        reference: reference(binding, signature),
        source,
        span: Span::default(),
        captured_text: None,
    }
}
fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
}
fn payload(schema: &str) -> PluginPayload {
    PluginPayload {
        schema: hash(schema),
        value: serde_json::json!({"value":7}),
    }
}

#[test]
fn source_export_queue_delivers_all_typed_local_intents_without_sdk_registration() {
    let current = scope("local-queue", 1);
    let mut owner = HostCallbacks::new(current.clone());
    let cases = [
        ("action", CallbackArguments::Action, CallbackSource::Action),
        (
            "text",
            CallbackArguments::Text("typed/🌊".into()),
            CallbackSource::CurrentTargetText,
        ),
        (
            "boolean",
            CallbackArguments::Boolean(false),
            CallbackSource::BooleanValue,
        ),
    ];
    let mut keys = Vec::new();
    let mut handlers = BTreeMap::new();
    for (index, (name, args, source)) in cases.iter().enumerate() {
        keys.push(owner.bind_local((*name).into(), args.signature()).unwrap());
        handlers.insert(
            format!("0/{index}"),
            handler(name, args.signature(), *source),
        );
    }
    let mut queue =
        DocumentCallbacks::new(owner.export_bindings().unwrap(), &current, handlers).unwrap();
    for (index, (_, args, _)) in cases.iter().enumerate() {
        queue
            .enqueue_arguments_guarded(&format!("0/{index}"), args.clone())
            .unwrap();
    }
    let drained = queue.drain();
    assert_eq!(drained.len(), 3);
    for ((pending, key), (_, expected, _)) in drained.into_iter().zip(&keys).zip(cases) {
        let result = owner
            .dispatch_local(&pending.handle, pending.invocation.arguments, &current)
            .unwrap();
        assert!(result.matches_key(key));
        assert_eq!(result.arguments(), &expected);
        assert_eq!(result.scope(), &current);
    }
}

#[test]
fn already_drained_callbacks_are_revoked_and_only_new_export_is_accepted() {
    let current = scope("local-reexport", 1);
    let mut owner = HostCallbacks::new(current.clone());
    let key = owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let mut queue = DocumentCallbacks::new(
        owner.export_bindings().unwrap(),
        &current,
        BTreeMap::from([(
            "0/0".into(),
            handler("resume", CallbackSignature::Action, CallbackSource::Action),
        )]),
    )
    .unwrap();
    queue.enqueue_guarded("0/0", None).unwrap();
    let pending = queue.drain().pop().unwrap();
    queue.close();
    assert!(matches!(
        owner.dispatch_local(
            &pending.handle,
            pending.invocation.arguments.clone(),
            &current
        ),
        Err(LocalDispatchError::Invalid(PluginError {
            code: PluginErrorCode::Stale,
            ..
        }))
    ));
    let fresh = owner.export_bindings().unwrap();
    let handle = fresh
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    assert!(
        owner
            .dispatch_local(&handle, CallbackArguments::Action, &current)
            .unwrap()
            .matches_key(&key)
    );
    owner.invalidate().unwrap();
    assert!(matches!(
        owner.dispatch_local(&handle, CallbackArguments::Action, &current),
        Err(LocalDispatchError::Invalid(_))
    ));
}

struct Adapter {
    manifest: PluginManifest,
    calls: Rc<Cell<usize>>,
}
impl PluginAdapter for Adapter {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn validate_payload(
        &self,
        _: &str,
        _: PayloadDirection,
        _: &PluginPayload,
    ) -> Result<(), PluginError> {
        Ok(())
    }
    fn dispatch(&mut self, _: &PluginInvocation) -> Result<PluginDispatch, PluginError> {
        self.calls.set(self.calls.get() + 1);
        Ok(PluginDispatch::Immediate(Ok(payload("reply"))))
    }
    fn cancel(&mut self, _: &PluginTicket) -> Result<CancelReceipt, PluginError> {
        Ok(CancelReceipt::Requested)
    }
    fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
fn registry(granted: bool) -> (PluginRegistry, RegistrationHandle, Rc<Cell<usize>>) {
    let current = scope("mixed-local-plugin", 1);
    let mut registry =
        PluginRegistry::new(hash("registry"), TargetKind::Native, current, 4).unwrap();
    let calls = Rc::new(Cell::new(0));
    let manifest = PluginManifest {
        api_version: PLUGIN_API_VERSION,
        id: "test.local.compat".into(),
        version: "1".into(),
        contract: hash("contract"),
        provider: hash("provider"),
        assets: BTreeMap::new(),
        capabilities: BTreeMap::from([(
            "query".into(),
            CapabilityDescriptor {
                targets: BTreeSet::from([TargetKind::Native]),
                request_schema: hash("request"),
                reply_schema: hash("reply"),
                error_schema: Some(hash("error")),
                event_schema: Some(hash("event")),
            },
        )]),
    };
    let grants = if granted {
        BTreeSet::from(["query".into()])
    } else {
        BTreeSet::new()
    };
    let registration = registry
        .register(
            Box::new(Adapter {
                manifest,
                calls: calls.clone(),
            }),
            grants,
        )
        .unwrap();
    (registry, registration, calls)
}

#[test]
fn mixed_queue_routes_only_validated_notlocal_and_cross_target_calls_no_sdk() {
    let (mut registry, registration, calls) = registry(true);
    let current = registry.scope().clone();
    let mut owner = HostCallbacks::new(current.clone());
    let produced = Rc::new(Cell::new(0));
    let counter = produced.clone();
    let legacy = owner
        .bind(
            "query".into(),
            CallbackSignature::Action,
            registration,
            "query".into(),
            move |_| {
                counter.set(counter.get() + 1);
                Ok(payload("request"))
            },
        )
        .unwrap();
    let key = owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    assert!(
        owner
            .bind_local("query".into(), CallbackSignature::Action)
            .is_err()
    );
    let export = owner.export_bindings().unwrap();
    let local = export
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    assert_eq!(
        owner
            .dispatch(&local, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    assert_eq!(produced.get(), 0);
    assert_eq!(calls.get(), 0);
    assert!(matches!(
        owner.dispatch_local(&legacy, CallbackArguments::Boolean(true), &current),
        Err(LocalDispatchError::Invalid(PluginError {
            code: PluginErrorCode::InvalidPayload,
            ..
        }))
    ));
    assert_eq!(produced.get(), 0);
    assert_eq!(calls.get(), 0);
    assert!(matches!(
        owner.dispatch_local(&legacy, CallbackArguments::Action, &current),
        Err(LocalDispatchError::NotLocal)
    ));
    let mut queue = DocumentCallbacks::new(
        export,
        &current,
        BTreeMap::from([
            (
                "0/0".into(),
                handler("resume", CallbackSignature::Action, CallbackSource::Action),
            ),
            (
                "0/1".into(),
                handler("query", CallbackSignature::Action, CallbackSource::Action),
            ),
        ]),
    )
    .unwrap();
    queue.enqueue_guarded("0/0", None).unwrap();
    queue.enqueue_guarded("0/1", None).unwrap();
    let mut local_count = 0;
    for pending in queue.drain() {
        match owner.dispatch_local(
            &pending.handle,
            pending.invocation.arguments.clone(),
            &current,
        ) {
            Ok(local) => {
                assert!(local.matches_key(&key));
                local_count += 1;
            }
            Err(LocalDispatchError::NotLocal) => {
                owner
                    .dispatch(&pending.handle, pending.invocation.arguments, &mut registry)
                    .unwrap();
            }
            Err(LocalDispatchError::Invalid(error)) => {
                panic!("validation error must never fall back: {error}")
            }
        }
    }
    assert_eq!(local_count, 1);
    assert_eq!(produced.get(), 1);
    assert_eq!(calls.get(), 1);
    // Closing the source export still leaves the original legacy plugin host handle usable.
    queue.close();
    owner
        .dispatch(&legacy, CallbackArguments::Action, &mut registry)
        .unwrap();
    assert_eq!(produced.get(), 2);
    assert_eq!(calls.get(), 2);
}

#[test]
fn plugin_error_priority_payload_failure_and_scope_guards_are_unchanged() {
    let (mut registry, registration, calls) = registry(true);
    let current = registry.scope().clone();
    let mut owner = HostCallbacks::new(current.clone());
    let produced = Rc::new(Cell::new(0));
    let counter = produced.clone();
    let handle = owner
        .bind(
            "query".into(),
            CallbackSignature::Action,
            registration,
            "query".into(),
            move |_| {
                counter.set(counter.get() + 1);
                Err(PluginError::new(
                    PluginErrorCode::InvalidPayload,
                    "original producer error",
                ))
            },
        )
        .unwrap();
    registry
        .rebind_scope(scope("mixed-local-plugin", 2))
        .unwrap();
    assert_eq!(
        owner
            .dispatch(&handle, CallbackArguments::Boolean(true), &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert_eq!(produced.get(), 0);
    assert_eq!(calls.get(), 0);
    registry.rebind_scope(current.clone()).unwrap();
    assert_eq!(
        owner
            .dispatch(&handle, CallbackArguments::Boolean(true), &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    assert_eq!(produced.get(), 0);
    assert_eq!(
        owner
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .message,
        "original producer error"
    );
    assert_eq!(produced.get(), 1);
    assert_eq!(calls.get(), 0);
    owner.invalidate().unwrap();
    assert!(matches!(
        owner.dispatch_local(&handle, CallbackArguments::Action, &current),
        Err(LocalDispatchError::Invalid(PluginError {
            code: PluginErrorCode::Stale,
            ..
        }))
    ));
}

#[test]
fn plugin_grant_rejection_stays_in_plugin_dispatch_after_payload_production() {
    let (mut registry, registration, calls) = registry(false);
    let current = registry.scope().clone();
    let mut owner = HostCallbacks::new(current.clone());
    let produced = Rc::new(Cell::new(0));
    let counter = produced.clone();
    let handle = owner
        .bind(
            "query".into(),
            CallbackSignature::Action,
            registration,
            "query".into(),
            move |_| {
                counter.set(counter.get() + 1);
                Ok(payload("request"))
            },
        )
        .unwrap();
    assert!(matches!(
        owner.dispatch_local(&handle, CallbackArguments::Action, &current),
        Err(LocalDispatchError::NotLocal)
    ));
    assert_eq!(produced.get(), 0);
    assert_eq!(calls.get(), 0);
    assert!(
        owner
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .is_err()
    );
    assert_eq!(produced.get(), 1);
    assert_eq!(calls.get(), 0);
}
