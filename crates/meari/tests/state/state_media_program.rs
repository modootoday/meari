use meari::{
    application::{
        Language,
        media::MediaChange,
        responsive::ResponsiveCommit,
        state_program::{ReloadPolicy, StateProgram},
    },
    domain::{component_state::StateValue, host::HostEnvironment},
};

fn program(name: &str) -> StateProgram {
    StateProgram::mount(
        meari::parse(
            include_str!("../../fixtures/state-counter.tsx"),
            Language::Tsx,
        )
        .unwrap(),
        name.into(),
        1,
    )
    .unwrap()
}
fn commit(revision: u64, matches: bool) -> ResponsiveCommit {
    let environment: HostEnvironment = serde_json::from_value(serde_json::json!({
        "target":"web","layout":{"width":400,"height":300,"dark":false},
        "visual":{"width":400,"height":300,"offset_x":0,"offset_y":0,"scale":1},
        "device_scale":1,"layout_zoom":null,"safe_area":null,
        "capabilities":{"client_resize":"unsupported","preview_resize":"unknown","reduced_motion":"unknown","contrast":"unknown","pointer_hover":"unknown","accessibility":"unknown"}
    })).unwrap();
    ResponsiveCommit {
        revision,
        environment,
        media_changes: vec![MediaChange {
            subscription: 1,
            matches,
        }],
    }
}

#[test]
fn media_state_projection_rejects_bad_slots_stale_revisions_and_old_source_tickets_atomically() {
    let mut program = program("media-program");
    let mut bindings = program.media_bindings(1).unwrap();
    let initial = MediaChange {
        subscription: 1,
        matches: true,
    };
    let before = program.snapshot().unwrap();
    assert!(
        program
            .bind_media(&mut bindings, &initial, "count".into())
            .is_err()
    );
    assert_eq!(program.snapshot().unwrap(), before);
    let projection = program
        .bind_media(&mut bindings, &initial, "disabled".into())
        .unwrap();
    assert!(projection.component.states.is_empty());
    assert_eq!(
        program.snapshot().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
    let source = meari::parse(
        include_str!("../../fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let ticket = program
        .prepare_replacement(source, ReloadPolicy::PreserveCompatible)
        .unwrap();
    program
        .apply_media(&mut bindings, &commit(2, false))
        .unwrap();
    let current = program.snapshot().unwrap();
    assert_eq!(current.values["disabled"], StateValue::Bool(false));
    assert!(
        program
            .apply_media(&mut bindings, &commit(2, true))
            .is_err()
    );
    assert!(program.commit_replacement(ticket).is_err());
    assert_eq!(program.snapshot().unwrap(), current);
    let mut invalid = commit(3, true);
    invalid.environment.device_scale = f64::NAN;
    assert!(program.apply_media(&mut bindings, &invalid).is_err());
    program
        .apply_media(&mut bindings, &commit(3, true))
        .unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
}

#[test]
fn media_bindings_cannot_be_applied_to_another_component_instance() {
    let mut first = program("first-media-program");
    let mut second = program("second-media-program");
    let mut bindings = first.media_bindings(1).unwrap();
    let initial = MediaChange {
        subscription: 1,
        matches: true,
    };
    let before = second.snapshot().unwrap();
    assert!(
        second
            .bind_media(&mut bindings, &initial, "disabled".into())
            .is_err()
    );
    assert_eq!(second.snapshot().unwrap(), before);
    first
        .bind_media(&mut bindings, &initial, "disabled".into())
        .unwrap();
    assert!(
        second
            .apply_media(&mut bindings, &commit(2, false))
            .is_err()
    );
    first.apply_media(&mut bindings, &commit(2, false)).unwrap();
    assert_eq!(
        first.snapshot().unwrap().values["disabled"],
        StateValue::Bool(false)
    );
}
