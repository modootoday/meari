use super::*;
use meari::{
    application::host_callbacks::{DocumentCallbacks, HostCallbacks},
    domain::{Span, host_callbacks::*},
};

fn queue() -> (DocumentCallbacks, HostCallbacks, PluginRegistry) {
    let mut registry = registry(TargetKind::Native);
    let registration = registry
        .register(
            adapter(PluginDispatch::Immediate(Ok(payload("reply")))),
            BTreeSet::from(["convert".into()]),
        )
        .unwrap();
    let mut owner = HostCallbacks::new(registry.scope().clone());
    for (name, signature) in [
        ("action", CallbackSignature::Action),
        ("text", CallbackSignature::Text),
        ("boolean", CallbackSignature::Boolean),
    ] {
        owner
            .bind(
                name.into(),
                signature,
                registration.clone(),
                "convert".into(),
                |_| Ok(payload("request")),
            )
            .unwrap();
    }
    let handlers = BTreeMap::from([
        (
            "0/2".into(),
            BoundCallback {
                reference: CallbackReference {
                    binding: "boolean".into(),
                    signature: CallbackSignature::Boolean,
                },
                source: CallbackSource::BooleanValue,
                captured_text: None,
                span: Span::default(),
            },
        ),
        (
            "0/0".into(),
            BoundCallback {
                reference: CallbackReference {
                    binding: "action".into(),
                    signature: CallbackSignature::Action,
                },
                source: CallbackSource::Action,
                captured_text: None,
                span: Span::default(),
            },
        ),
        (
            "0/1".into(),
            BoundCallback {
                reference: CallbackReference {
                    binding: "text".into(),
                    signature: CallbackSignature::Text,
                },
                source: CallbackSource::CurrentTargetText,
                captured_text: None,
                span: Span::default(),
            },
        ),
        (
            "0/3".into(),
            BoundCallback {
                reference: CallbackReference {
                    binding: "text".into(),
                    signature: CallbackSignature::Text,
                },
                source: CallbackSource::CapturedText,
                span: Span::default(),
                captured_text: Some("작업공간/🌊".into()),
            },
        ),
        (
            "0/4".into(),
            BoundCallback {
                reference: CallbackReference {
                    binding: "text".into(),
                    signature: CallbackSignature::Text,
                },
                source: CallbackSource::CapturedText,
                span: Span::default(),
                captured_text: Some("x".repeat(1024 * 1024)),
            },
        ),
    ]);
    (
        DocumentCallbacks::new(owner.export_bindings().unwrap(), registry.scope(), handlers)
            .unwrap(),
        owner,
        registry,
    )
}

#[test]
fn captured_activation_uses_bound_payload_and_rejects_injected_values() {
    let (mut queue, owner, mut registry) = queue();
    assert!(queue.enqueue_guarded("0/3", Some("injected")).is_err());
    assert!(
        queue
            .enqueue_arguments_guarded("0/3", CallbackArguments::Boolean(true))
            .is_err()
    );
    queue.enqueue_guarded("0/3", None).unwrap();
    let pending = queue.drain().pop().unwrap();
    assert_eq!(
        pending.invocation.arguments,
        CallbackArguments::Text("작업공간/🌊".into())
    );
    queue.close();
    assert!(
        owner
            .dispatch(&pending.handle, pending.invocation.arguments, &mut registry)
            .is_err()
    );
}

#[test]
fn captured_bytes_count_toward_queue_budget_and_drain_resets_it() {
    let (mut queue, _owner, _registry) = queue();
    for _ in 0..4 {
        queue.enqueue_guarded("0/4", None).unwrap();
    }
    assert!(queue.enqueue_guarded("0/4", None).is_err());
    assert!(queue.enqueue_guarded("0/3", None).is_err());
    assert_eq!(queue.drain().len(), 4);
    queue.enqueue_guarded("0/3", None).unwrap();
    assert_eq!(queue.drain().len(), 1);
}
#[test]
fn boolean_queue_preserves_value_rejects_coercion_and_revokes_drained_handle() {
    let (mut queue, owner, mut registry) = queue();
    assert!(queue.enqueue_guarded("0/2", Some("true")).is_err());
    assert!(queue.enqueue_guarded("0/2", None).is_err());
    for value in [false, true] {
        queue
            .enqueue_arguments_guarded("0/2", CallbackArguments::Boolean(value))
            .unwrap();
    }
    let mut drained = queue.drain();
    assert_eq!(drained.len(), 2);
    assert_eq!(
        drained.remove(0).invocation.arguments,
        CallbackArguments::Boolean(false)
    );
    let stale = drained.remove(0);
    assert_eq!(stale.invocation.arguments, CallbackArguments::Boolean(true));
    queue.close();
    assert!(
        queue
            .enqueue_arguments_guarded("0/2", CallbackArguments::Boolean(false))
            .is_err()
    );
    assert!(
        owner
            .dispatch(&stale.handle, stale.invocation.arguments, &mut registry)
            .is_err()
    );
}
#[test]
fn queue_enforces_count_text_budgets_and_resets_accounting_on_drain() {
    let (mut queue, _owner, _registry) = queue();
    assert!(
        queue
            .enqueue_guarded("0/1", Some(&"x".repeat(1024 * 1024 + 1)))
            .is_err()
    );
    let text = "x".repeat(1024 * 1024);
    for _ in 0..4 {
        queue.enqueue_guarded("0/1", Some(&text)).unwrap();
    }
    assert!(
        queue
            .enqueue_arguments_guarded("0/2", CallbackArguments::Boolean(true))
            .is_err()
    );
    assert!(queue.enqueue_guarded("0/1", Some("x")).is_err());
    assert_eq!(queue.drain().len(), 4);
    for _ in 0..1024 {
        queue.enqueue_guarded("0/0", None).unwrap();
    }
    assert!(queue.enqueue_guarded("0/0", None).is_err());
    assert_eq!(queue.drain().len(), 1024);
    queue.enqueue_guarded("0/1", Some("after drain")).unwrap();
    queue
        .enqueue_arguments_guarded("0/2", CallbackArguments::Boolean(false))
        .unwrap();
}

#[test]
fn failed_export_candidate_keeps_existing_boolean_queue_live() {
    let (mut queue, owner, mut registry) = queue();
    let mut foreign = registry.scope().clone();
    foreign.source_generation += 1;
    assert!(
        DocumentCallbacks::new(owner.export_bindings().unwrap(), &foreign, BTreeMap::new())
            .is_err()
    );
    queue
        .enqueue_arguments_guarded("0/2", CallbackArguments::Boolean(false))
        .unwrap();
    let pending = queue.drain().remove(0);
    assert!(
        owner
            .dispatch(&pending.handle, pending.invocation.arguments, &mut registry)
            .is_ok()
    );
}

#[test]
fn boolean_producer_receives_false_and_true_without_coercion() {
    use std::{cell::RefCell, rc::Rc};
    let mut registry = registry(TargetKind::Native);
    let registration = registry
        .register(
            adapter(PluginDispatch::Immediate(Ok(payload("reply")))),
            BTreeSet::from(["convert".into()]),
        )
        .unwrap();
    let mut owner = HostCallbacks::new(registry.scope().clone());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let captured = seen.clone();
    let handle = owner
        .bind(
            "boolean".into(),
            CallbackSignature::Boolean,
            registration,
            "convert".into(),
            move |arguments| {
                let CallbackArguments::Boolean(value) = arguments else {
                    panic!("signature validation must precede the payload producer");
                };
                captured.borrow_mut().push(*value);
                Ok(payload("request"))
            },
        )
        .unwrap();
    for value in [false, true] {
        assert!(
            owner
                .dispatch(&handle, CallbackArguments::Boolean(value), &mut registry)
                .is_ok()
        );
    }
    assert_eq!(*seen.borrow(), vec![false, true]);
    assert!(
        owner
            .dispatch(
                &handle,
                CallbackArguments::Text("true".into()),
                &mut registry
            )
            .is_err()
    );
    owner.invalidate().unwrap();
    assert!(
        owner
            .dispatch(&handle, CallbackArguments::Boolean(false), &mut registry)
            .is_err()
    );
    assert_eq!(*seen.borrow(), vec![false, true]);
}
#[test]
fn queue_lease_drop_and_owner_invalidation_reject_deferred_dispatch() {
    let (mut queue, owner, mut registry) = queue();
    assert!(queue.enqueue_guarded("missing", None).is_err());
    assert!(queue.enqueue_guarded("0/1", None).is_err());
    queue.enqueue_guarded("0/0", None).unwrap();
    let request = queue.drain().pop().unwrap();
    drop(queue);
    assert_eq!(
        owner
            .dispatch(&request.handle, request.invocation.arguments, &mut registry)
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
}
