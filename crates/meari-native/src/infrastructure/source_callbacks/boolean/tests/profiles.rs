//! Additional public widget profiles use the same real registry and document factory.
use super::*;

#[test]
fn public_checkbox_space_and_accessibility_requests_preserve_accepted_value() {
    let source = "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <Checkbox checked={props.checked} disabled={props.disabled} onCheckedChange={props.change} aria-label=\"terms\"/>;}";
    let (mut document, host, _registry, calls) = mount_source(source);
    document.focus_path("0").unwrap();
    document
        .dispatch(Action::Key {
            path: "0".into(),
            key: " ".into(),
            shift: false,
        })
        .unwrap();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    assert_eq!(document.elements()[0].attributes["aria-checked"], "false");
    let node = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0").unwrap())
        .unwrap();
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    assert_eq!(
        document
            .renderer
            .document
            .get_element_by_id(document.path_id("0").unwrap()),
        Some(node)
    );
    crate::infrastructure::accessibility::act(
        &mut document,
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: accesskit::NodeId(node.as_u64()),
            data: None,
        },
    )
    .unwrap();
    let pending = document.drain_callbacks();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].invocation.arguments,
        CallbackArguments::Boolean(false)
    );
    let tree = crate::infrastructure::accessibility::tree(&document);
    let accessible = &tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == node.as_u64())
        .unwrap()
        .1;
    assert_eq!(accessible.role(), accesskit::Role::CheckBox);
    assert_eq!(accessible.toggled(), Some(accesskit::Toggled::True));
    assert_eq!(calls.get(), 0);
}

#[test]
fn public_alert_dialog_action_cancel_require_acceptance_and_do_not_dismiss_implicitly() {
    let source = "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <div><Button>outside</Button><AlertDialog open={props.checked} onOpenChange={props.change}><AlertDialogTrigger disabled={props.disabled}>open</AlertDialogTrigger><AlertDialogContent aria-label=\"confirm\"><AlertDialogCancel>cancel</AlertDialogCancel><AlertDialogAction>confirm</AlertDialogAction></AlertDialogContent></AlertDialog></div>;}";
    let (mut document, host, _registry, calls) = mount_source(source);
    document
        .dispatch(Action::Activate {
            path: "0/1/0".into(),
        })
        .unwrap();
    assert_eq!(
        document.drain_callbacks()[0].invocation.arguments,
        CallbackArguments::Boolean(true)
    );
    assert!(!document.session.widgets["0/1"].open);
    document
        .update_module_props_with_callbacks(props(true, false), host.export_bindings().unwrap())
        .unwrap();
    document
        .dispatch(Action::Key {
            path: "0/1/1/0".into(),
            key: "Escape".into(),
            shift: false,
        })
        .unwrap();
    document
        .dispatch(Action::DismissOutside { path: "0/0".into() })
        .unwrap();
    assert!(document.drain_callbacks().is_empty());
    for path in ["0/1/1/0", "0/1/1/1"] {
        document
            .dispatch(Action::Activate { path: path.into() })
            .unwrap();
        let pending = document.drain_callbacks();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].invocation.arguments,
            CallbackArguments::Boolean(false)
        );
        assert!(document.session.widgets["0/1"].open);
    }
    let content = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/1/1").unwrap())
        .unwrap();
    let tree = crate::infrastructure::accessibility::tree(&document);
    assert_eq!(
        tree.nodes
            .iter()
            .find(|(id, _)| id.0 == content.as_u64())
            .unwrap()
            .1
            .role(),
        accesskit::Role::AlertDialog
    );
    document
        .update_module_props_with_callbacks(props(false, false), host.export_bindings().unwrap())
        .unwrap();
    assert!(!document.session.widgets["0/1"].open);
    let trigger = document
        .renderer
        .document
        .get_element_by_id(document.path_id("0/1/0").unwrap())
        .unwrap();
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(trigger)
    );
    assert_eq!(calls.get(), 0);
}
