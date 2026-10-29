use meari::{
    application::{Language, plugins::*, state_program::StateProgram},
    domain::build::{ContentHash, TargetKind},
};
use std::collections::{BTreeMap, BTreeSet};
#[path = "document_callbacks.rs"]
mod document_callbacks;

fn hash(value: &str) -> ContentHash {
    ContentHash::of(value.as_bytes())
}
fn payload(schema: &str) -> PluginPayload {
    PluginPayload {
        schema: hash(schema),
        value: serde_json::json!({"value": 7}),
    }
}
fn scope(generation: u64) -> PluginScope {
    let component = meari::parse(
        "type Props={};export function Demo(props:Props){return <main/>;}",
        Language::Tsx,
    )
    .unwrap();
    let program = StateProgram::mount(component, "plugin-test".into(), 1).unwrap();
    PluginScope {
        instance: program.snapshot().unwrap().instance,
        source_generation: generation,
    }
}
fn manifest() -> PluginManifest {
    PluginManifest {
        api_version: PLUGIN_API_VERSION,
        id: "test.convert".into(),
        version: "1".into(),
        contract: hash("contract"),
        provider: hash("provider"),
        assets: BTreeMap::new(),
        capabilities: BTreeMap::from([(
            "convert".into(),
            CapabilityDescriptor {
                targets: BTreeSet::from([TargetKind::Native]),
                request_schema: hash("request"),
                reply_schema: hash("reply"),
                error_schema: Some(hash("error")),
                event_schema: Some(hash("event")),
            },
        )]),
    }
}
struct Adapter {
    manifest: PluginManifest,
    outcome: PluginDispatch,
    cleanup_busy: bool,
}
impl PluginAdapter for Adapter {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    fn validate_payload(
        &self,
        _: &str,
        _: PayloadDirection,
        payload: &PluginPayload,
    ) -> Result<(), PluginError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Dto {
            value: u32,
        }
        let dto: Dto = payload.decode(&payload.schema)?;
        if dto.value > 10 {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "value out of range",
            ));
        }
        Ok(())
    }
    fn dispatch(&mut self, _: &PluginInvocation) -> Result<PluginDispatch, PluginError> {
        Ok(self.outcome.clone())
    }
    fn cancel(&mut self, _: &PluginTicket) -> Result<CancelReceipt, PluginError> {
        Ok(CancelReceipt::Requested)
    }
    fn shutdown(&mut self) -> Result<(), PluginError> {
        if self.cleanup_busy {
            self.cleanup_busy = false;
            return Err(PluginError::new(PluginErrorCode::Busy, "cleanup pending"));
        }
        Ok(())
    }
}
fn adapter(outcome: PluginDispatch) -> Box<dyn PluginAdapter> {
    Box::new(Adapter {
        manifest: manifest(),
        outcome,
        cleanup_busy: true,
    })
}
fn registry(target: TargetKind) -> PluginRegistry {
    PluginRegistry::new(hash("registry"), target, scope(1), 2).unwrap()
}

#[test]
fn host_callback_checks_signature_identity_generation_and_registry_grants() {
    use meari::{
        application::host_callbacks::HostCallbacks,
        domain::host_callbacks::{CallbackArguments, CallbackSignature},
    };
    let mut registry = registry(TargetKind::Native);
    let plugin = registry
        .register(
            adapter(PluginDispatch::Immediate(Ok(payload("reply")))),
            BTreeSet::from(["convert".into()]),
        )
        .unwrap();
    let mut callbacks = HostCallbacks::new(registry.scope().clone());
    let handle = callbacks
        .bind(
            "onSave".into(),
            CallbackSignature::Action,
            plugin.clone(),
            "convert".into(),
            |_| Ok(payload("request")),
        )
        .unwrap();
    assert!(
        callbacks
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .is_ok()
    );
    assert_eq!(
        callbacks
            .dispatch(
                &handle,
                CallbackArguments::Text("draft".into()),
                &mut registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    let foreign = HostCallbacks::new(registry.scope().clone());
    assert_eq!(
        foreign
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    let text = callbacks
        .bind(
            "onDraftChange".into(),
            CallbackSignature::Text,
            plugin,
            "convert".into(),
            |arguments| {
                let CallbackArguments::Text(value) = arguments else {
                    unreachable!()
                };
                Ok(PluginPayload {
                    schema: hash("request"),
                    value: serde_json::json!({"value": value.len()}),
                })
            },
        )
        .unwrap();
    assert!(
        callbacks
            .dispatch(&text, CallbackArguments::Text("abc".into()), &mut registry)
            .is_ok()
    );
    registry.rebind_scope(scope(2)).unwrap();
    assert_eq!(
        callbacks
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    callbacks.invalidate().unwrap();
    assert_eq!(
        callbacks
            .dispatch(&handle, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
}

#[test]
fn document_callback_export_revokes_queued_handles_without_revoking_host_handles() {
    use meari::{
        application::host_callbacks::HostCallbacks,
        domain::host_callbacks::{CallbackArguments, CallbackReference, CallbackSignature},
    };
    let mut registry = registry(TargetKind::Native);
    let plugin = registry
        .register(
            adapter(PluginDispatch::Immediate(Ok(payload("reply")))),
            BTreeSet::from(["convert".into()]),
        )
        .unwrap();
    let mut callbacks = HostCallbacks::new(registry.scope().clone());
    let host = callbacks
        .bind(
            "onSave".into(),
            CallbackSignature::Action,
            plugin.clone(),
            "convert".into(),
            |_| Ok(payload("request")),
        )
        .unwrap();
    let reference = CallbackReference {
        binding: "onSave".into(),
        signature: CallbackSignature::Action,
    };
    let bindings = callbacks.export_bindings().unwrap();
    assert_eq!(bindings.scope(), registry.scope());
    let queued = bindings.resolve(&reference).unwrap();
    assert!(
        callbacks
            .dispatch(&queued, CallbackArguments::Action, &mut registry)
            .is_ok()
    );
    assert!(
        bindings
            .resolve(&CallbackReference {
                signature: CallbackSignature::Text,
                ..reference.clone()
            })
            .is_err()
    );
    drop(bindings);
    assert_eq!(
        callbacks
            .dispatch(&queued, CallbackArguments::Action, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert!(
        callbacks
            .dispatch(&host, CallbackArguments::Action, &mut registry)
            .is_ok()
    );
    let next = callbacks.export_bindings().unwrap();
    callbacks.invalidate().unwrap();
    assert!(next.resolve(&reference).is_err());
    assert!(callbacks.export_bindings().is_err());
    let orphan = {
        let mut owner = HostCallbacks::new(registry.scope().clone());
        owner
            .bind(
                "onSave".into(),
                CallbackSignature::Action,
                plugin,
                "convert".into(),
                |_| Ok(payload("request")),
            )
            .unwrap();
        owner.export_bindings().unwrap()
    };
    assert!(orphan.resolve(&reference).is_err());
}
fn grants() -> BTreeSet<String> {
    BTreeSet::from(["convert".into()])
}

#[test]
fn permissions_target_schema_and_foreign_handles_prevent_dispatch() {
    let mut registry = registry(TargetKind::Native);
    let handle = registry
        .register(adapter(PluginDispatch::Pending), BTreeSet::new())
        .unwrap();
    assert_eq!(
        registry
            .dispatch(&handle, "convert", payload("request"))
            .unwrap_err()
            .code,
        PluginErrorCode::PermissionDenied
    );
    let mut foreign = self::registry(TargetKind::Native);
    assert_eq!(
        foreign
            .dispatch(&handle, "convert", payload("request"))
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    let handle = foreign
        .register(adapter(PluginDispatch::Pending), grants())
        .unwrap();
    assert_eq!(
        foreign
            .dispatch(&handle, "convert", payload("reply"))
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    let mut invalid = payload("request");
    invalid.value = serde_json::json!({"value":7,"unexpected":true});
    assert_eq!(
        foreign
            .dispatch(&handle, "convert", invalid)
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    let mut web = self::registry(TargetKind::Web);
    let handle = web
        .register(adapter(PluginDispatch::Pending), grants())
        .unwrap();
    assert_eq!(
        web.dispatch(&handle, "convert", payload("request"))
            .unwrap_err()
            .code,
        PluginErrorCode::UnsupportedTarget
    );
}

#[test]
fn canonical_error_details_use_error_schema_and_are_preserved() {
    let mut registry = registry(TargetKind::Native);
    let error = PluginError {
        code: PluginErrorCode::Adapter,
        message: "canonical failure".into(),
        details: Some(payload("error")),
    };
    let handle = registry
        .register(
            adapter(PluginDispatch::Immediate(Err(error.clone()))),
            grants(),
        )
        .unwrap();
    let receipt = registry
        .dispatch(&handle, "convert", payload("request"))
        .unwrap();
    assert_eq!(receipt.outcome, PluginDispatch::Immediate(Err(error)));
    assert_eq!(
        registry.cancel(&receipt.ticket).unwrap(),
        CancelReceipt::AlreadyTerminal
    );
    let mut other = self::registry(TargetKind::Native);
    let wrong = PluginError {
        code: PluginErrorCode::Adapter,
        message: "wrong schema".into(),
        details: Some(payload("reply")),
    };
    let handle = other
        .register(adapter(PluginDispatch::Immediate(Err(wrong))), grants())
        .unwrap();
    assert_eq!(
        other
            .dispatch(&handle, "convert", payload("request"))
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
}

#[test]
fn cancellation_cleanup_and_scope_replacement_reject_late_events() {
    let mut registry = registry(TargetKind::Native);
    let handle = registry
        .register(adapter(PluginDispatch::Pending), grants())
        .unwrap();
    let receipt = registry
        .dispatch(&handle, "convert", payload("request"))
        .unwrap();
    assert_eq!(
        registry.rebind_scope(scope(2)).unwrap_err().code,
        PluginErrorCode::Busy
    );
    assert_eq!(
        registry.unregister(&handle).unwrap_err().code,
        PluginErrorCode::Busy
    );
    assert_eq!(
        registry
            .dispatch(&handle, "convert", payload("request"))
            .unwrap_err()
            .code,
        PluginErrorCode::Closing
    );
    assert_eq!(
        registry.cancel(&receipt.ticket).unwrap(),
        CancelReceipt::Requested
    );
    assert_eq!(
        registry.cancel(&receipt.ticket).unwrap(),
        CancelReceipt::Requested
    );
    let actual = registry
        .accept_event(
            &receipt.ticket,
            PluginEventKind::Completed(payload("reply")),
        )
        .unwrap();
    assert!(actual.cancellation_requested);
    assert!(matches!(actual.event, PluginEventKind::Completed(_)));
    assert_eq!(registry.pending_count(), 0);
    assert_eq!(
        registry
            .accept_event(&receipt.ticket, PluginEventKind::Cancelled)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert_eq!(
        registry.unregister(&handle).unwrap_err().code,
        PluginErrorCode::Busy
    );
    registry.unregister(&handle).unwrap();
    assert_eq!(
        registry.cancel(&receipt.ticket).unwrap_err().code,
        PluginErrorCode::Stale
    );
    registry.rebind_scope(scope(2)).unwrap();
    let replacement = registry
        .register(adapter(PluginDispatch::Pending), grants())
        .unwrap();
    assert_ne!(handle, replacement);
    assert_eq!(
        registry.manifest(&handle).unwrap_err().code,
        PluginErrorCode::Stale
    );
}
