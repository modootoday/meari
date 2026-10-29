use meari::{
    application::{
        Language,
        app_bundle::{AppBuildStamp, ApplicationDefinition},
    },
    domain::{build::*, viewport::Viewport},
    infrastructure::web_document::WebDocument,
};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::HtmlElement;

#[wasm_bindgen_test]
fn browser_application_commit_preserves_state_and_rejects_stale_generation() {
    let original = include_str!("../../fixtures/state-counter.tsx");
    let initial = ApplicationDefinition {
        component: meari::parse(original, Language::Tsx).unwrap(),
        props: BTreeMap::new(),
        title: "Meari app reload".into(),
        native_viewport: Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        stamp: AppBuildStamp {
            session: ContentHash::of(b"web-app-session"),
            generation: 1,
            graph_hash: ContentHash::of(b"web-initial"),
            target: BuildTarget {
                kind: TargetKind::Web,
                triple: "wasm32-unknown-unknown".into(),
            },
        },
    };
    let browser = web_sys::window().unwrap().document().unwrap();
    let old_title = browser.title();
    let mut document = WebDocument::mount_application(&initial, "").unwrap();
    document
        .element("0/1")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    let mut next = initial.clone();
    next.stamp.generation = 2;
    next.stamp.graph_hash = ContentHash::of(b"web-next");
    next.component = meari::parse(
        &original.replace("previous + 1", "previous + 10"),
        Language::Tsx,
    )
    .unwrap();
    let prepared = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    let stale = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    let before_rollback = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    assert_eq!(document.application_stamp(), Some(initial.stamp.clone()));
    let committed = document
        .commit_reversible_application_replacement(prepared)
        .unwrap();
    assert_eq!(committed.stamp(), &next.stamp);
    assert!(document.commit_application_replacement(stale).is_err());
    assert!(
        document
            .prepare_application_replacement(initial.clone())
            .is_err()
    );
    document
        .element("0/1")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    assert_eq!(document.last_error(), None);
    assert_eq!(
        document.element("0/0").unwrap().text_content().unwrap(),
        "Count 11 units"
    );
    assert_eq!(document.application_stamp(), Some(next.stamp.clone()));
    assert_eq!(
        document
            .rollback_application_replacement(committed)
            .unwrap(),
        initial.stamp
    );
    assert!(
        document
            .commit_application_replacement(before_rollback)
            .is_err()
    );
    assert!(document.prepare_application_replacement(next).is_err());
    document
        .element("0/1")
        .unwrap()
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
    assert_eq!(document.last_error(), None);
    assert_eq!(
        document.element("0/0").unwrap().text_content().unwrap(),
        "Count 12 units"
    );
    drop(document);
    browser.set_title(&old_title);
}
