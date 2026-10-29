use meari::{
    application::hook_values::Budget,
    domain::{Value, build::BuildPath},
};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::native_host::NativeDocumentHandle,
};
use std::collections::BTreeMap;

fn props(caption: &str, disabled: bool) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("caption".into(), Value::String(caption.into())),
        ("disabled".into(), Value::Bool(disabled)),
    ])
}
fn mount() -> NativeDocumentHandle {
    let path = |name: &str| BuildPath::try_from(name.to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &path("root.tsx"),
        &BTreeMap::from([
            (
                path("root.tsx"),
                include_str!("../../fixtures/hooks/modules/root.tsx").into(),
            ),
            (
                path("counter.tsx"),
                include_str!("../../fixtures/hooks/modules/counter.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    NativeDocumentHandle::mount_hook_modules(
        graph,
        props("before", false),
        ("native-hooks".into(), 1),
        "button{display:block;width:120px;height:32px}input{display:block;width:120px;height:32px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        1,
        Budget::default(),
    )
    .unwrap()
}
fn count(handle: &NativeDocumentHandle, key: &str) -> f64 {
    handle
        .with_hook_state(|owner, projection| {
            let root = owner
                .snapshot(projection.instance(key).unwrap())
                .unwrap()
                .roots["state"]
                .clone();
            owner
                .values()
                .number(&owner.values().field(&root, "count").unwrap().unwrap())
                .unwrap()
        })
        .unwrap()
}

#[test]
fn native_real_dispatch_and_host_receipt_publish_separate_instances_without_replacing_dirty_input()
{
    let handle = mount();
    let expected = handle.stamp().unwrap();
    let ((input, button), _) = handle
        .try_reproject(&expected, |document| {
            document.set_input_value("0/0/1", "dirty")?;
            document.focus_path("0/0/1")?;
            Ok::<_, String>((
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id("0/0/1").unwrap())
                    .unwrap(),
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id("0/0/0").unwrap())
                    .unwrap(),
            ))
        })
        .unwrap();
    assert!(
        handle
            .dispatch_action(Action::Activate {
                path: "0/0/0".into()
            })
            .unwrap()
    );
    assert_eq!(count(&handle, "root@0/0"), 1.0);
    assert_eq!(count(&handle, "root@0/1"), 0.0);
    handle
        .try_with(|document| {
            assert_eq!(
                document
                    .renderer
                    .document
                    .get_element_by_id(document.path_id("0/0/1").unwrap()),
                Some(input)
            );
            assert_eq!(
                document.input_values.get("0/0/1").map(String::as_str),
                Some("dirty")
            );
            assert_eq!(
                document.renderer.document.get_focussed_node_id(),
                Some(input)
            );
            let accessibility = meari_native::infrastructure::accessibility::tree(document);
            assert!(
                accessibility
                    .nodes
                    .iter()
                    .any(|(id, _)| id.0 == button.as_u64())
            );
        })
        .unwrap();
}

#[test]
fn native_props_only_frame_keyboard_disabled_and_rejected_props_keep_last_good_heap() {
    let handle = mount();
    let stamp = handle.with_hook_state(|owner, _| owner.stamp()).unwrap();
    let receipt = handle
        .update_hook_props(props("after", false))
        .unwrap()
        .unwrap();
    assert!(receipt.after().document_revision > receipt.before().document_revision);
    assert!(
        handle
            .with_hook_state(|owner, _| owner.stamp().revision > stamp.revision)
            .unwrap()
    );
    assert!(
        handle
            .dispatch_action(Action::Key {
                path: "0/1/0".into(),
                key: "Enter".into(),
                shift: false
            })
            .unwrap()
    );
    assert_eq!(count(&handle, "root@0/1"), 1.0);
    handle.update_hook_props(props("disabled", true)).unwrap();
    assert!(
        !handle
            .dispatch_action(Action::Activate {
                path: "0/1/0".into()
            })
            .unwrap()
    );
    let before = handle.with_hook_state(|owner, _| owner.stamp()).unwrap();
    assert!(
        handle
            .update_hook_props(BTreeMap::from([
                ("caption".into(), Value::Bool(true)),
                ("disabled".into(), Value::Bool(false))
            ]))
            .is_err()
    );
    assert_eq!(
        handle.with_hook_state(|owner, _| owner.stamp()).unwrap(),
        before
    );
    assert_eq!(count(&handle, "root@0/1"), 1.0);
    assert!(
        handle
            .with_hook_failure(|failure| failure.is_some())
            .unwrap()
    );
}

#[test]
fn real_controlled_focus_patch_failure_preserves_heap_program_and_original_frame_handler() {
    let entry = BuildPath::try_from("modal.tsx".to_owned()).unwrap();
    let source = "import {useState} from 'react';type Props={open:boolean};export function Modal(props:Props){const [state,setState]=useState({count:0});return <main><button onClick={()=>setState(previous=>({...previous,count:previous.count+1}))}>increment</button><Dialog open={props.open}><DialogTrigger>open</DialogTrigger><DialogContent><DialogTitle>Details</DialogTitle><input defaultValue='keep'/></DialogContent></Dialog></main>;}";
    let graph =
        meari::parse_module_graph(&entry, &BTreeMap::from([(entry.clone(), source.into())]))
            .unwrap();
    let handle = NativeDocumentHandle::mount_hook_modules(
        graph,
        BTreeMap::from([("open".into(), Value::Bool(false))]),
        ("modal-hooks".into(), 1),
        "button,input{display:block;width:120px;height:32px}",
        Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        1,
        Budget::default(),
    )
    .unwrap();
    let expected = handle.stamp().unwrap();
    let (trigger, _) = handle
        .try_reproject(&expected, |document| {
            document.focus_path("0/1/0")?;
            let trigger = document
                .renderer
                .document
                .get_element_by_id(document.path_id("0/1/0").unwrap())
                .unwrap();
            document.replace_stylesheet("input,[role=dialog]{visibility:hidden}")?;
            Ok::<_, String>(trigger)
        })
        .unwrap();
    let before = handle.with_hook_state(|owner, _| owner.stamp()).unwrap();
    let error = handle
        .update_hook_props(BTreeMap::from([("open".into(), Value::Bool(true))]))
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("no visible focus target"),
        "{error}"
    );
    assert_eq!(
        handle.with_hook_state(|owner, _| owner.stamp()).unwrap(),
        before
    );
    assert_eq!(count(&handle, "root"), 0.0);
    handle
        .try_with(|document| {
            assert!(!document.session.widgets["0/1"].open);
            assert_eq!(
                document.renderer.document.get_focussed_node_id(),
                Some(trigger)
            );
        })
        .unwrap();
    assert!(
        handle
            .dispatch_action(Action::Activate { path: "0/0".into() })
            .unwrap()
    );
    assert_eq!(count(&handle, "root"), 1.0);
}
