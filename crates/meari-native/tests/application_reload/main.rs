use meari::{
    application::{
        Language,
        app_bundle::{AppBuildStamp, ApplicationDefinition},
    },
    domain::{build::*, component_state::StateValue, viewport::Viewport},
};
use meari_native::{domain::widgets::Action, infrastructure::document::InteractiveDocument};
use std::collections::BTreeMap;
#[path = "composites.rs"]
mod composites;
#[path = "rollback.rs"]
mod rollback;

fn definition() -> ApplicationDefinition {
    ApplicationDefinition {
        component: meari::parse(
            include_str!("../../../meari/fixtures/state-counter.tsx"),
            Language::Tsx,
        )
        .unwrap(),
        props: BTreeMap::new(),
        title: "App reload".into(),
        native_viewport: Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        stamp: AppBuildStamp {
            session: ContentHash::of(b"app-session"),
            generation: 1,
            graph_hash: ContentHash::of(b"initial-source"),
            target: BuildTarget {
                kind: TargetKind::Native,
                triple: "x86_64-pc-windows-msvc".into(),
            },
        },
    }
}

fn successor(original: &ApplicationDefinition) -> ApplicationDefinition {
    let mut next = original.clone();
    next.stamp.generation += 1;
    next.stamp.graph_hash = ContentHash::of(b"next-source");
    next.component = meari::parse(
        &include_str!("../../../meari/fixtures/state-counter.tsx")
            .replace("previous + 1", "previous + 10"),
        Language::Tsx,
    )
    .unwrap();
    next
}

#[test]
fn native_application_commit_advances_stamp_only_after_retained_source_commit() {
    let initial = definition();
    let mut document = InteractiveDocument::from_application(&initial, "").unwrap();
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    let next = successor(&initial);
    let first = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    let duplicate = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    assert_eq!(document.application_stamp(), Some(&initial.stamp));
    assert_eq!(
        document.commit_application_replacement(first).unwrap(),
        next.stamp
    );
    assert!(document.commit_application_replacement(duplicate).is_err());
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    assert_eq!(
        document.component_state().unwrap().values["count"],
        StateValue::Number(serde_json::Number::from_f64(11.0).unwrap())
    );
    assert_eq!(document.application_stamp(), Some(&next.stamp));
}

#[test]
fn native_application_rejects_foreign_stale_and_unsupported_host_updates() {
    let initial = definition();
    let document = InteractiveDocument::from_application(&initial, "").unwrap();
    assert!(
        document
            .prepare_application_replacement(initial.clone())
            .is_err()
    );
    for field in ["session", "target", "title", "viewport", "shape"] {
        let mut invalid = successor(&initial);
        match field {
            "session" => invalid.stamp.session = ContentHash::of(b"foreign"),
            "target" => {
                invalid.stamp.target = BuildTarget {
                    kind: TargetKind::Web,
                    triple: "wasm32-unknown-unknown".into(),
                }
            }
            "title" => invalid.title = "Changed".into(),
            "viewport" => invalid.native_viewport.width += 1,
            "shape" => {
                invalid.component.root = meari::parse(
                    "type Props={};export function Other(props:Props){return <p/>;}",
                    Language::Tsx,
                )
                .unwrap()
                .root
            }
            _ => unreachable!(),
        }
        assert!(
            document.prepare_application_replacement(invalid).is_err(),
            "{field}"
        );
        assert_eq!(document.application_stamp(), Some(&initial.stamp));
    }
}

#[test]
fn state_change_after_prepare_does_not_publish_application_generation() {
    let initial = definition();
    let mut document = InteractiveDocument::from_application(&initial, "").unwrap();
    let prepared = document
        .prepare_application_replacement(successor(&initial))
        .unwrap();
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    let current = document.component_state().unwrap();
    assert!(document.commit_application_replacement(prepared).is_err());
    assert_eq!(document.component_state().unwrap(), current);
    assert_eq!(document.application_stamp(), Some(&initial.stamp));
}

#[test]
fn stateless_application_retains_dom_and_rejects_another_mounts_ticket() {
    let mut initial = definition();
    initial.component = meari::parse(
        "type Props={};export function Page(props:Props){return <p id=\"message\">Before</p>;}",
        Language::Tsx,
    )
    .unwrap();
    let mut first = InteractiveDocument::from_application(&initial, "").unwrap();
    let mut other = InteractiveDocument::from_application(&initial, "").unwrap();
    let node = first
        .renderer
        .document
        .get_element_by_id("message")
        .unwrap();
    let mut next = initial.clone();
    next.stamp.generation += 1;
    next.component = meari::parse(
        "type Props={};export function Page(props:Props){return <p id=\"message\">After</p>;}",
        Language::Tsx,
    )
    .unwrap();
    let ticket = first.prepare_application_replacement(next.clone()).unwrap();
    assert!(other.commit_application_replacement(ticket).is_err());
    assert_eq!(other.application_stamp(), Some(&initial.stamp));
    let ticket = first.prepare_application_replacement(next.clone()).unwrap();
    first.commit_application_replacement(ticket).unwrap();
    assert_eq!(
        first.renderer.document.get_element_by_id("message"),
        Some(node)
    );
    assert_eq!(
        first
            .renderer
            .document
            .get_node(node)
            .unwrap()
            .text_content(),
        "After"
    );
    assert!(first.component_state().unwrap().values.is_empty());
    assert_eq!(first.application_stamp(), Some(&next.stamp));
}
