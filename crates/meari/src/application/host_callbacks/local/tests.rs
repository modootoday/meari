use super::*;
use crate::domain::{component_state::InstanceHandle, host_callbacks::CallbackReference};

fn scope() -> PluginScope {
    PluginScope {
        instance: InstanceHandle {
            document: "local-unit".into(),
            epoch: 1,
            id: 1,
        },
        source_generation: 1,
    }
}
fn reference(prop: &str, signature: CallbackSignature) -> CallbackReference {
    CallbackReference {
        binding: prop.into(),
        signature,
    }
}
fn invalid(result: Result<LocalActionInvocation, LocalDispatchError>, code: PluginErrorCode) {
    match result {
        Err(LocalDispatchError::Invalid(error)) => assert_eq!(error.code, code),
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[test]
fn rust_keys_use_identity_instead_of_equal_unit_contents_or_prop_names() {
    let current = scope();
    let mut first = HostCallbacks::new(current.clone());
    let mut second = HostCallbacks::new(current.clone());
    let key = first
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let other = first
        .bind_local("other".into(), CallbackSignature::Action)
        .unwrap();
    let foreign = second
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    assert_eq!(key, key.clone());
    assert_ne!(key, other);
    assert_ne!(key, foreign);
    let export = first.export_bindings().unwrap();
    let handle = export
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    let action = first
        .dispatch_local(&handle, CallbackArguments::Action, &current)
        .unwrap();
    assert_eq!(action.key(), &key);
    assert!(action.matches_key(&key));
    assert!(!action.matches_key(&other));
    assert_eq!(action.scope(), &current);
}

#[test]
fn local_actions_require_an_actual_export_lease() {
    let current = scope();
    let mut owner = HostCallbacks::new(current.clone());
    owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let host_only = CallbackHandle {
        owner: owner.owner.clone(),
        lease: None,
        revision: owner.revision,
        prop: "resume".into(),
    };
    invalid(
        owner.dispatch_local(&host_only, CallbackArguments::Action, &current),
        PluginErrorCode::Stale,
    );
    let export = owner.export_bindings().unwrap();
    let handle = export
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    assert!(
        owner
            .dispatch_local(&handle, CallbackArguments::Action, &current)
            .is_ok()
    );
}

#[test]
fn owner_scope_epoch_generation_and_revision_mismatches_are_stale() {
    let current = scope();
    let mut owner = HostCallbacks::new(current.clone());
    owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let export = owner.export_bindings().unwrap();
    let handle = export
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    let mut foreign_owner = HostCallbacks::new(current.clone());
    foreign_owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    invalid(
        foreign_owner.dispatch_local(&handle, CallbackArguments::Action, &current),
        PluginErrorCode::Stale,
    );
    let mut epoch = current.clone();
    epoch.instance.epoch += 1;
    let mut generation = current.clone();
    generation.source_generation += 1;
    let mut instance = current.clone();
    instance.instance.id += 1;
    for foreign in [epoch, generation, instance] {
        invalid(
            owner.dispatch_local(&handle, CallbackArguments::Action, &foreign),
            PluginErrorCode::Stale,
        );
    }
    let mut old = handle.clone();
    old.revision += 1;
    invalid(
        owner.dispatch_local(&old, CallbackArguments::Action, &current),
        PluginErrorCode::Stale,
    );
}

#[test]
fn all_existing_argument_signatures_keep_their_values_and_validation() {
    let current = scope();
    let mut owner = HostCallbacks::new(current.clone());
    for (name, signature) in [
        ("a", CallbackSignature::Action),
        ("t", CallbackSignature::Text),
        ("b", CallbackSignature::Boolean),
    ] {
        owner.bind_local(name.into(), signature).unwrap();
    }
    let export = owner.export_bindings().unwrap();
    for (name, value) in [
        ("a", CallbackArguments::Action),
        ("t", CallbackArguments::Text("input".into())),
        ("b", CallbackArguments::Boolean(false)),
    ] {
        let handle = export.resolve(&reference(name, value.signature())).unwrap();
        assert_eq!(
            owner
                .dispatch_local(&handle, value.clone(), &current)
                .unwrap()
                .arguments(),
            &value
        );
    }
    let action = export
        .resolve(&reference("a", CallbackSignature::Action))
        .unwrap();
    invalid(
        owner.dispatch_local(&action, CallbackArguments::Boolean(true), &current),
        PluginErrorCode::InvalidPayload,
    );
    let text = export
        .resolve(&reference("t", CallbackSignature::Text))
        .unwrap();
    invalid(
        owner.dispatch_local(
            &text,
            CallbackArguments::Text("x".repeat(1024 * 1024 + 1)),
            &current,
        ),
        PluginErrorCode::InvalidPayload,
    );
    let mut foreign = current.clone();
    foreign.source_generation += 1;
    invalid(
        owner.dispatch_local(
            &text,
            CallbackArguments::Text("x".repeat(1024 * 1024 + 1)),
            &foreign,
        ),
        PluginErrorCode::Stale,
    );
}

#[test]
fn old_exports_revoke_without_remapping_the_original_rust_key() {
    let current = scope();
    let mut owner = HostCallbacks::new(current.clone());
    let key = owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let old = owner.export_bindings().unwrap();
    let old_handle = old
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    let new = owner.export_bindings().unwrap();
    let new_handle = new
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    drop(old);
    invalid(
        owner.dispatch_local(&old_handle, CallbackArguments::Action, &current),
        PluginErrorCode::Stale,
    );
    assert!(
        owner
            .dispatch_local(&new_handle, CallbackArguments::Action, &current)
            .unwrap()
            .matches_key(&key)
    );
    owner.invalidate().unwrap();
    invalid(
        owner.dispatch_local(&new_handle, CallbackArguments::Action, &current),
        PluginErrorCode::Stale,
    );
}

#[test]
fn owner_drop_closes_exports_even_when_a_key_or_queued_handle_is_retained() {
    let mut owner = HostCallbacks::new(scope());
    let key = owner
        .bind_local("resume".into(), CallbackSignature::Action)
        .unwrap();
    let export = owner.export_bindings().unwrap();
    let handle = export
        .resolve(&reference("resume", CallbackSignature::Action))
        .unwrap();
    drop(owner);
    assert_eq!(
        export
            .resolve(&reference("resume", CallbackSignature::Action))
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    assert!(
        !handle
            .owner
            .active
            .load(std::sync::atomic::Ordering::Acquire)
    );
    assert_eq!(key, key.clone());
}

#[test]
fn common_binding_count_name_and_duplicate_limits_are_not_bypassed() {
    let mut owner = HostCallbacks::new(scope());
    for name in ["".to_owned(), "bad-name".to_owned(), "x".repeat(257)] {
        assert_eq!(
            owner
                .bind_local(name, CallbackSignature::Action)
                .unwrap_err()
                .code,
            PluginErrorCode::InvalidPayload
        );
    }
    for index in 0..1024 {
        owner
            .bind_local(format!("action_{index}"), CallbackSignature::Action)
            .unwrap();
    }
    assert!(
        owner
            .bind_local("action_0".into(), CallbackSignature::Action)
            .is_err()
    );
    assert!(
        owner
            .bind_local("overflow".into(), CallbackSignature::Action)
            .is_err()
    );
    owner.invalidate().unwrap();
    assert!(
        owner
            .bind_local("closed".into(), CallbackSignature::Action)
            .is_err()
    );
}

#[test]
fn zero_source_generation_cannot_mint_a_local_binding() {
    let mut current = scope();
    current.source_generation = 0;
    let mut owner = HostCallbacks::new(current);
    assert_eq!(
        owner
            .bind_local("resume".into(), CallbackSignature::Action)
            .unwrap_err()
            .code,
        PluginErrorCode::InvalidPayload
    );
    assert!(owner.bindings.is_empty());
}
