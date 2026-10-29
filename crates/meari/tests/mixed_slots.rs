#[path = "../fixtures/node-slots/mixed_support.rs"]
mod support;
use meari::{
    application::source_modules::child_placement,
    domain::{Node, NodeKind, Span, Value, node_slots::NodeSlot},
};
use std::collections::BTreeMap;

fn mixed(root: &Node) -> BTreeMap<String, Vec<Option<String>>> {
    fn visit(node: &Node, path: String, found: &mut BTreeMap<String, Vec<Option<String>>>) {
        if let NodeKind::Element { children, .. } = &node.kind {
            let keys = children
                .iter()
                .map(|child| match &child.kind {
                    NodeKind::Element { attributes, .. } => {
                        attributes.iter().find_map(|attribute| {
                            if attribute.name == meari::application::repeat::IDENTITY_ATTRIBUTE
                                && let Value::String(value) = &attribute.value
                            {
                                return Some(value.clone());
                            }
                            None
                        })
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            if keys.iter().any(Option::is_some) && keys.iter().any(Option::is_none) {
                found.insert(path.clone(), keys);
            }
            for (index, child) in children.iter().enumerate() {
                visit(child, format!("{path}/{index}"), found);
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(root, "0".into(), &mut found);
    found
}

#[test]
fn opaque_nested_slot_refresh_keeps_both_original_mixed_parent_placements() {
    let mut program = support::program("core-mixed", false, false);
    let before = program.project().unwrap().clone();
    let values = support::factory_values(&program.node_slots(), true, true).unwrap();
    let after = program.update_props(values).unwrap();
    assert_eq!(
        before.instances.keys().collect::<Vec<_>>(),
        after.instances.keys().collect::<Vec<_>>()
    );
    let old = mixed(&before.component.root);
    let next = mixed(&after.component.root);
    for path in ["0/1", "0/1/1/1"] {
        assert_eq!(
            old[path].iter().map(Option::is_some).collect::<Vec<_>>(),
            vec![false, true]
        );
        let a = old[path].iter().map(Option::as_deref).collect::<Vec<_>>();
        let b = next[path].iter().map(Option::as_deref).collect::<Vec<_>>();
        assert_eq!(
            child_placement::plan(&a, &b, Span::default()).unwrap(),
            vec![Some(0), Some(1)]
        );
    }
    assert_eq!(
        support::tag_path(&before.component.root, "input", "0"),
        support::tag_path(&after.component.root, "input", "0")
    );
}

#[test]
fn source_reference_changes_are_not_stable_placements_and_raw_keys_are_not_slot_authority() {
    let mut program = support::program("core-mixed-authority", false, false);
    let before = mixed(&program.project().unwrap().component.root);
    let factory = program.node_slots();
    let editor = factory
        .component(
            "editor",
            support::alternate_reference(),
            support::editor_props("Alternate"),
        )
        .unwrap();
    let dock = factory
        .component(
            "dock",
            support::dock_reference(),
            support::dock_props(editor, false, false),
        )
        .unwrap();
    let after = program.update_props(support::frame_props(dock)).unwrap();
    let after = mixed(&after.component.root);
    let a = before["0/1/1/1"]
        .iter()
        .map(Option::as_deref)
        .collect::<Vec<_>>();
    let b = after["0/1/1/1"]
        .iter()
        .map(Option::as_deref)
        .collect::<Vec<_>>();
    assert!(child_placement::plan(&a, &b, Span::default()).is_err());
    let raw = Value::NodeSlot(NodeSlot::Content(vec![Node {
        kind: NodeKind::Element {
            tag: "article".into(),
            attributes: vec![meari::domain::Attribute {
                name: meari::application::repeat::IDENTITY_ATTRIBUTE.into(),
                value: Value::String("forged-key".into()),
            }],
            children: vec![],
        },
        span: Span::default(),
    }]));
    assert!(
        program
            .node_slots()
            .component(
                "dock",
                support::dock_reference(),
                support::dock_props(raw, false, false)
            )
            .is_err()
    );
}
