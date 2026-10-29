use meari::{
    application::Language,
    domain::{component_state::StateValue, host::HostEnvironment},
};
use meari_native::{
    domain::{Viewport, widgets::Action},
    infrastructure::{accessibility, document::InteractiveDocument},
};

fn environment(width: u32) -> HostEnvironment {
    serde_json::from_value(serde_json::json!({
        "target":"native","layout":{"width":width,"height":600,"dark":false},
        "visual":{"width":width,"height":600,"offset_x":0,"offset_y":0,"scale":1},
        "device_scale":1.5,"layout_zoom":1.25,"safe_area":null,
        "capabilities":{"client_resize":"supported","preview_resize":"unsupported","reduced_motion":"unknown","contrast":"unknown","pointer_hover":"unknown","accessibility":"unknown"}
    })).unwrap()
}
fn document() -> InteractiveDocument {
    let source = meari::parse(
        include_str!("../../meari/fixtures/state-counter.tsx"),
        Language::Tsx,
    )
    .unwrap();
    let mut document = InteractiveDocument::new(
        source,
        "input{width:100px;height:30px}button{width:100px;height:30px}",
        Viewport {
            width: 800,
            height: 600,
            dark: false,
        },
    )
    .unwrap();
    document.commit_host_environment(environment(800)).unwrap();
    document
}

#[test]
fn native_media_state_updates_disabled_dom_and_accessibility_without_replacing_input() {
    let mut document = document();
    document
        .set_input_value("0/4", "native media draft")
        .unwrap();
    document.focus_path("0/4").unwrap();
    let input = document
        .renderer
        .document
        .get_element_by_id("name")
        .unwrap();
    let button = document
        .renderer
        .document
        .get_element_by_id("increment")
        .unwrap();
    assert!(
        document
            .subscribe_media_state("(min-width:600px)", "count".into())
            .is_err()
    );
    let subscription = document
        .subscribe_media_state("(min-width:600px)", "disabled".into())
        .unwrap();
    assert!(subscription.matches);
    assert_eq!(
        document.component_state().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
    assert!(
        !document
            .dispatch(Action::Activate { path: "0/1".into() })
            .unwrap()
    );
    let commit = document.commit_host_environment(environment(400)).unwrap();
    assert_eq!(commit.revision, 2);
    assert_eq!(commit.media_changes.len(), 1);
    assert_eq!(
        commit.media_changes[0].subscription,
        subscription.subscription
    );
    assert!(!commit.media_changes[0].matches);
    assert_eq!(
        document.component_state().unwrap().values["disabled"],
        StateValue::Bool(false)
    );
    assert_eq!(
        document.renderer.document.get_element_by_id("name"),
        Some(input)
    );
    assert_eq!(document.input_values["0/4"], "native media draft");
    assert_eq!(document.session.focus.as_deref(), Some("0/4"));
    let tree = accessibility::tree(&document);
    let (_, editable) = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == input.as_u64())
        .unwrap();
    assert_eq!(editable.value(), Some("native media draft"));
    let (_, enabled) = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == button.as_u64())
        .unwrap();
    assert!(!enabled.is_disabled());
    assert!(
        document
            .dispatch(Action::Activate { path: "0/1".into() })
            .unwrap()
    );
    document.commit_host_environment(environment(800)).unwrap();
    assert_eq!(
        document.component_state().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
    assert!(document.unsubscribe_media(subscription.subscription));
    document
        .dispatch(Action::Activate { path: "0/3".into() })
        .unwrap();
    assert!(
        document
            .commit_host_environment(environment(400))
            .unwrap()
            .media_changes
            .is_empty()
    );
    assert_eq!(
        document.component_state().unwrap().values["disabled"],
        StateValue::Bool(false)
    );
}

#[test]
fn failed_native_media_dom_patch_restores_host_and_never_publishes_state_or_notifications() {
    let mut document = document();
    document
        .subscribe_media_state("(min-width:600px)", "disabled".into())
        .unwrap();
    let before = document.component_state().unwrap();
    let notifications = document.media_changes.len();
    let status = document
        .renderer
        .document
        .get_element_by_id("status")
        .unwrap();
    document
        .renderer
        .document
        .mutate()
        .remove_and_drop_all_children(status);
    assert!(document.commit_host_environment(environment(400)).is_err());
    assert_eq!(document.component_state().unwrap(), before);
    assert_eq!(document.viewport.width, 800);
    assert_eq!(
        document.renderer.document.get_viewport().logical_size(),
        (800.0, 600.0)
    );
    assert_eq!(document.media_changes.len(), notifications);
}
