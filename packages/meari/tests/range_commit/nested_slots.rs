use super::*;
#[path = "../../fixtures/range-nested/support.rs"]
mod support;
use meari::{
    application::host_callbacks::HostCallbacks,
    domain::{
        host_callbacks::{CallbackArguments, CallbackSource},
        plugins::PluginScope,
    },
};

#[test]
fn nested_owned_workspace_preserves_instances_and_original_callback_export_on_value_feedback() {
    let mut program = support::program("nested-core");
    let initial = program.project().unwrap().clone();
    let identities = support::instances(&initial);
    assert_eq!(
        identities.len(),
        3,
        "ProductFrame, owned ReadRoot, imported Transport"
    );
    let paths = support::display_paths(&initial.component.root);
    let scope = PluginScope {
        instance: initial.instances["root"].instance.clone(),
        source_generation: 7,
    };
    let mut host = HostCallbacks::new(scope.clone());
    host.bind_local("onDraft".into(), CallbackSignature::Text)
        .unwrap();
    host.bind_local("onCommit".into(), CallbackSignature::Text)
        .unwrap();
    let handlers = initial.callback_bindings();
    assert_eq!(handlers.len(), 2);
    assert!(handlers.values().all(
        |handler| handler.source == CallbackSource::CurrentTargetText
            && handler.captured_text.is_none()
    ));
    let draft = handlers
        .iter()
        .find(|(_, handler)| handler.reference.binding == "onDraft")
        .unwrap()
        .0
        .clone();
    let mut queue =
        DocumentCallbacks::new(host.export_bindings().unwrap(), &scope, handlers).unwrap();
    let owner = queue.event_owner(&draft).unwrap();
    queue.enqueue_guarded(&draft, Some("50")).unwrap();
    let retired = program
        .node_slots()
        .component(
            "workspace",
            support::reference(),
            support::reader_props("20"),
        )
        .unwrap();
    program
        .update_props(support::root_props(retired.clone()))
        .unwrap();
    let fresh = program
        .node_slots()
        .component(
            "workspace",
            support::reference(),
            support::reader_props("50"),
        )
        .unwrap();
    let next = program.update_props(support::root_props(fresh)).unwrap();
    queue
        .validate_retained_range_props_with_text(
            &initial.component,
            &next.component,
            &next.callback_bindings(),
            &paths,
        )
        .unwrap();
    assert_eq!(support::instances(&next), identities);
    assert_eq!(queue.event_owner(&draft), Some(owner));
    let foreign_program = support::program("nested-foreign");
    let foreign = foreign_program
        .node_slots()
        .component(
            "workspace",
            support::reference(),
            support::reader_props("50"),
        )
        .unwrap();
    assert!(
        program
            .update_props(support::root_props(foreign))
            .unwrap_err()
            .diagnostic
            .message
            .contains("foreign")
    );
    assert!(
        program
            .update_props(support::root_props(retired))
            .unwrap_err()
            .diagnostic
            .message
            .contains("retired")
    );
    assert_eq!(support::instances(program.project().unwrap()), identities);
    assert!(
        program
            .project()
            .unwrap()
            .component
            .semantic_eq(&next.component)
    );
    let pending = queue.drain().pop().unwrap();
    assert_eq!(
        pending.invocation.arguments,
        CallbackArguments::Text("50".into())
    );
    assert!(
        host.dispatch_local(&pending.handle, pending.invocation.arguments, &scope)
            .is_ok()
    );
    host.invalidate().unwrap();
    assert!(queue.enqueue_guarded(&draft, Some("60")).is_err());
}
