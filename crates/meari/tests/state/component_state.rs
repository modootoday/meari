use meari::{
    application::component_state::ComponentState,
    domain::component_state::{InstanceKey, StateAction, StateSchema, StateValue},
};

fn schema() -> StateSchema {
    serde_json::from_value(serde_json::json!({"revision":"counter-v1","slots":{
        "count":{"kind":"number","initial":{"type":"number","value":0}},
        "open":{"kind":"bool","initial":{"type":"bool","value":false}},
        "label":{"kind":"string","initial":{"type":"string","value":"initial"}},
        "data":{"kind":"json","initial":{"type":"json","value":{"rows":[]}}}
    }}))
    .unwrap()
}

#[test]
fn discarded_staging_clone_cannot_reuse_an_instance_id_and_state_remains_send_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ComponentState>();
    let mut state = ComponentState::new("reserved-identity".into(), 1).unwrap();
    let schema = StateSchema {
        revision: "1".into(),
        slots: std::collections::BTreeMap::new(),
    };
    let mut discarded = state.clone();
    let unpublished = discarded
        .mount(
            None,
            InstanceKey::Explicit("row".into()),
            "Row".into(),
            schema.clone(),
        )
        .unwrap();
    drop(discarded);
    let published = state
        .mount(
            None,
            InstanceKey::Explicit("row".into()),
            "Row".into(),
            schema,
        )
        .unwrap();
    assert_ne!(unpublished, published);
    assert!(state.snapshot(&unpublished).is_err());
    assert!(state.snapshot(&published).is_ok());
}
fn key(value: &str) -> InstanceKey {
    InstanceKey::Explicit(value.into())
}

#[test]
fn keyed_siblings_preserve_state_and_parent_replacement_invalidates_descendant_actions() {
    let mut state = ComponentState::new("document-a".into(), 1).unwrap();
    let root = state
        .mount(None, key("root"), "app".into(), schema())
        .unwrap();
    let a = state
        .mount(Some(&root), key("a"), "row".into(), schema())
        .unwrap();
    let b = state
        .mount(Some(&root), key("b"), "row".into(), schema())
        .unwrap();
    state
        .dispatch(
            &a,
            0,
            &[StateAction::Add {
                slot: "count".into(),
                delta: 5.into(),
            }],
        )
        .unwrap();
    assert_eq!(
        state
            .mount(Some(&root), key("b"), "row".into(), schema())
            .unwrap(),
        b
    );
    assert_eq!(
        state
            .mount(Some(&root), key("a"), "row".into(), schema())
            .unwrap(),
        a
    );
    assert_eq!(
        state.snapshot(&a).unwrap().values["count"],
        StateValue::Number(5.into())
    );
    assert_eq!(
        state.snapshot(&b).unwrap().values["count"],
        StateValue::Number(0.into())
    );
    let mut changed_initializer = schema();
    changed_initializer.slots.get_mut("count").unwrap().initial = StateValue::Number(99.into());
    assert_eq!(
        state
            .mount(Some(&root), key("a"), "row".into(), changed_initializer)
            .unwrap(),
        a
    );
    assert_eq!(
        state.snapshot(&a).unwrap().values["count"],
        StateValue::Number(5.into())
    );
    let mut changed = schema();
    changed.revision = "counter-v2".into();
    assert!(
        state
            .mount(Some(&root), key("a"), "row".into(), changed)
            .is_err()
    );
    assert!(state.snapshot(&a).is_ok());
    let next = state
        .mount(None, key("root"), "other-app".into(), schema())
        .unwrap();
    assert_ne!(next, root);
    assert!(state.snapshot(&a).is_err());
    assert!(state.snapshot(&b).is_err());
    assert!(
        state
            .dispatch(
                &a,
                1,
                &[StateAction::Toggle {
                    slot: "open".into()
                }]
            )
            .is_err()
    );
    assert!(
        state
            .mount(Some(&root), key("child"), "row".into(), schema())
            .is_err()
    );
    state.unmount(&next).unwrap();
    let remount = state
        .mount(None, key("root"), "other-app".into(), schema())
        .unwrap();
    assert_ne!(remount, next);
    assert_eq!(state.snapshot(&remount).unwrap().revision, 0);
    let mut replacement = ComponentState::new("document-a".into(), 2).unwrap();
    replacement
        .mount(None, key("root"), "other-app".into(), schema())
        .unwrap();
    assert!(replacement.snapshot(&remount).is_err());
}

#[test]
fn typed_action_batches_are_atomic_sequential_and_revision_checked() {
    let mut state = ComponentState::new("document".into(), 1).unwrap();
    let handle = state
        .mount(None, InstanceKey::Position(0), "counter".into(), schema())
        .unwrap();
    assert!(
        !state
            .dispatch(
                &handle,
                0,
                &[StateAction::Set {
                    slot: "open".into(),
                    value: StateValue::Bool(false)
                }]
            )
            .unwrap()
            .changed
    );
    let commit = state
        .dispatch(
            &handle,
            0,
            &[
                StateAction::Add {
                    slot: "count".into(),
                    delta: 1.into(),
                },
                StateAction::Add {
                    slot: "count".into(),
                    delta: 2.into(),
                },
                StateAction::Toggle {
                    slot: "open".into(),
                },
                StateAction::Set {
                    slot: "data".into(),
                    value: StateValue::Json(serde_json::json!([1,{"id":"a"}])),
                },
            ],
        )
        .unwrap();
    assert_eq!(commit.values["count"], StateValue::Number(3.into()));
    assert_eq!(commit.values["open"], StateValue::Bool(true));
    assert_eq!(commit.revision, 1);
    assert!(state.dispatch(&handle, 0, &[]).is_err());
    assert!(
        state
            .dispatch(
                &handle,
                1,
                &[
                    StateAction::Set {
                        slot: "label".into(),
                        value: StateValue::String("must-not-commit".into())
                    },
                    StateAction::Set {
                        slot: "open".into(),
                        value: StateValue::String("wrong-type".into())
                    },
                ]
            )
            .is_err()
    );
    assert_eq!(state.snapshot(&handle).unwrap().values, commit.values);
    assert!(
        state
            .dispatch(
                &handle,
                1,
                &[StateAction::Toggle {
                    slot: "missing".into()
                }]
            )
            .is_err()
    );
    assert!(
        state
            .dispatch(
                &handle,
                1,
                &[
                    StateAction::Set {
                        slot: "count".into(),
                        value: StateValue::Number(u64::MAX.into())
                    },
                    StateAction::Add {
                        slot: "count".into(),
                        delta: 1.into()
                    },
                ]
            )
            .is_err()
    );
    assert_eq!(state.snapshot(&handle).unwrap().revision, 1);
    let commit = state
        .dispatch(
            &handle,
            1,
            &[
                StateAction::Set {
                    slot: "count".into(),
                    value: StateValue::Number(serde_json::Number::from_f64(1.5).unwrap()),
                },
                StateAction::Add {
                    slot: "count".into(),
                    delta: serde_json::Number::from_f64(0.25).unwrap(),
                },
            ],
        )
        .unwrap();
    assert_eq!(
        commit.values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.75).unwrap())
    );
}
