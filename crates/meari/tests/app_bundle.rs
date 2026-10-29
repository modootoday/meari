use meari::{
    application::{
        Language,
        app_bundle::{AppBuildStamp, ApplicationDefinition},
    },
    domain::{build::*, viewport::Viewport},
};
use std::collections::BTreeMap;

fn definition() -> ApplicationDefinition {
    ApplicationDefinition {
        component: meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx)
            .unwrap(),
        props: BTreeMap::new(),
        title: "Meari Counter".into(),
        native_viewport: Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        stamp: AppBuildStamp {
            session: ContentHash::of(b"session"),
            generation: 1,
            graph_hash: ContentHash::of(b"graph"),
            target: BuildTarget {
                kind: TargetKind::Native,
                triple: "x86_64-pc-windows-msvc".into(),
            },
        },
    }
}
#[test]
fn generated_app_definition_preserves_state_and_rejects_invalid_host_metadata() {
    let definition = definition();
    definition.validate().unwrap();
    definition.validate_target(TargetKind::Native).unwrap();
    assert!(definition.validate_target(TargetKind::Web).is_err());
    let copy: ApplicationDefinition =
        serde_json::from_slice(&serde_json::to_vec(&definition).unwrap()).unwrap();
    copy.validate().unwrap();
    assert_eq!(copy.bound_component().unwrap(), definition.component);
    for title in ["", "\n", "Meari\0Counter"] {
        let mut invalid = definition.clone();
        invalid.title = title.into();
        assert!(invalid.validate().is_err());
    }
    let mut invalid = definition.clone();
    invalid.stamp.generation = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = definition;
    invalid.native_viewport.width = 0;
    assert!(invalid.validate().is_err());
}

#[test]
fn successor_requires_monotonic_generation_within_the_same_session_and_target() {
    let initial = definition();
    assert!(initial.validate_successor(&initial).is_err());
    let mut next = initial.clone();
    next.stamp.generation += 1;
    next.validate_successor(&initial).unwrap();
    let mut foreign = next.clone();
    foreign.stamp.session = ContentHash::of(b"foreign-session");
    assert!(foreign.validate_successor(&initial).is_err());
    let mut web = next.clone();
    web.stamp.target = BuildTarget {
        kind: TargetKind::Web,
        triple: "wasm32-unknown-unknown".into(),
    };
    assert!(web.validate_successor(&initial).is_err());
    let mut invalid = next;
    invalid.stamp.generation = 0;
    assert!(invalid.validate_successor(&initial).is_err());
}
