use meari::{
    application::{
        component_state::{ComponentState, media::MediaStateBindings},
        media::MediaChange,
        responsive::ResponsiveCommit,
    },
    domain::{
        component_state::{InstanceKey, StateSchema, StateValue},
        host::HostEnvironment,
    },
};

#[test]
fn initial_media_state_and_revisions_share_the_component_action_lifetime() {
    let mut state = ComponentState::new("page".into(), 1).unwrap();
    let schema: StateSchema =
        serde_json::from_value(serde_json::json!({"revision":"media-v1","slots":{
            "wide":{"kind":"bool","initial":{"type":"bool","value":false}}
        }}))
        .unwrap();
    let owner = state
        .mount(
            None,
            InstanceKey::Position(0),
            "responsive-view".into(),
            schema,
        )
        .unwrap();
    let mut bindings = MediaStateBindings::new(owner.clone(), 1).unwrap();
    let initial = MediaChange {
        subscription: 1,
        matches: true,
    };
    let initial = bindings.bind(&mut state, &initial, "wide".into()).unwrap();
    assert_eq!(initial.values["wide"], StateValue::Bool(true));
    let environment: HostEnvironment = serde_json::from_value(serde_json::json!({
        "target":"native","layout":{"width":400,"height":300,"dark":false},
        "visual":{"width":400,"height":300,"offset_x":0,"offset_y":0,"scale":1},
        "device_scale":1,"layout_zoom":1,"safe_area":null,
        "capabilities":{"client_resize":"supported","preview_resize":"unsupported","reduced_motion":"unknown","contrast":"unknown","pointer_hover":"unknown","accessibility":"unknown"}
    })).unwrap();
    let mut commit = ResponsiveCommit {
        revision: 2,
        environment,
        media_changes: vec![
            MediaChange {
                subscription: 1,
                matches: false,
            },
            MediaChange {
                subscription: 1,
                matches: true,
            },
        ],
    };
    assert!(bindings.apply(&mut state, &commit).is_err());
    assert_eq!(
        state.snapshot(&owner).unwrap().values["wide"],
        StateValue::Bool(true)
    );
    commit.media_changes.pop();
    assert_eq!(
        bindings.apply(&mut state, &commit).unwrap().values["wide"],
        StateValue::Bool(false)
    );
    assert!(bindings.apply(&mut state, &commit).is_err());
    assert!(bindings.unbind(1));
    commit.revision = 3;
    commit.media_changes[0].matches = true;
    assert!(!bindings.apply(&mut state, &commit).unwrap().changed);
    state.unmount(&owner).unwrap();
    commit.revision = 4;
    assert!(bindings.apply(&mut state, &commit).is_err());
}
