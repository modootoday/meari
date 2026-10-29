use super::*;

#[test]
fn rollback_restores_previous_behavior_preserves_latest_state_and_invalidates_old_tickets() {
    let initial = definition();
    let mut document = InteractiveDocument::from_application(&initial, "").unwrap();
    let next = successor(&initial);
    let baseline_frame = document.render_application_frame().unwrap();
    assert_eq!(baseline_frame.stamp, initial.stamp);
    assert_eq!(baseline_frame.rgba_bytes, 640 * 480 * 4);
    let old_ticket = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    let prepared = document
        .prepare_application_replacement(next.clone())
        .unwrap();
    let committed = document
        .commit_reversible_application_replacement(prepared)
        .unwrap();
    assert_eq!(committed.stamp(), &next.stamp);
    let committed_frame = document.render_application_frame().unwrap();
    assert_eq!(committed_frame.stamp, next.stamp);
    assert!(committed_frame.definition_revision > baseline_frame.definition_revision);
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    document
        .set_input_value("0/4", "latest rollback draft")
        .unwrap();
    document.focus_path("0/4").unwrap();
    let latest = document.component_state().unwrap();
    let input = document
        .renderer
        .document
        .get_element_by_id("name")
        .unwrap();
    assert_eq!(
        document
            .rollback_application_replacement(committed)
            .unwrap(),
        initial.stamp
    );
    assert_eq!(document.component_state().unwrap(), latest);
    let restored_frame = document.render_application_frame().unwrap();
    assert_eq!(restored_frame.state, latest);
    assert_eq!(restored_frame.stamp, initial.stamp);
    assert!(restored_frame.definition_revision > committed_frame.definition_revision);
    assert_eq!(restored_frame.source_schema, baseline_frame.source_schema);
    assert_eq!(document.input_values["0/4"], "latest rollback draft");
    assert_eq!(document.session.focus.as_deref(), Some("0/4"));
    assert_eq!(
        document.renderer.document.get_element_by_id("name"),
        Some(input)
    );
    let tree = meari_native::infrastructure::accessibility::tree(&document);
    let (_, editable) = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == input.as_u64())
        .unwrap();
    assert_eq!(editable.value(), Some("latest rollback draft"));
    assert!(document.commit_application_replacement(old_ticket).is_err());
    assert!(
        document
            .prepare_application_replacement(next.clone())
            .is_err()
    );
    let mut recovery = next;
    recovery.stamp.generation += 1;
    assert!(document.prepare_application_replacement(recovery).is_ok());
    document
        .dispatch(Action::Activate { path: "0/1".into() })
        .unwrap();
    assert_eq!(
        document.component_state().unwrap().values["count"],
        StateValue::Number(serde_json::Number::from_f64(11.0).unwrap())
    );
}

#[test]
fn rollback_rejects_foreign_instances_and_intervening_source_replacements() {
    let initial = definition();
    let mut first = InteractiveDocument::from_application(&initial, "").unwrap();
    let mut other = InteractiveDocument::from_application(&initial, "").unwrap();
    let next = successor(&initial);
    let prepared = first.prepare_application_replacement(next.clone()).unwrap();
    let foreign = first
        .commit_reversible_application_replacement(prepared)
        .unwrap();
    let prepared = other.prepare_application_replacement(next.clone()).unwrap();
    other.commit_application_replacement(prepared).unwrap();
    assert!(other.rollback_application_replacement(foreign).is_err());
    assert_eq!(other.application_stamp(), Some(&next.stamp));
    let mut third = next.clone();
    third.stamp.generation += 1;
    let prepared = first
        .prepare_application_replacement(third.clone())
        .unwrap();
    let stale = first
        .commit_reversible_application_replacement(prepared)
        .unwrap();
    let prepared = first.prepare_source_replacement(third.component).unwrap();
    first.commit_source_replacement(prepared).unwrap();
    assert!(first.render_application_frame().is_err());
    assert!(first.rollback_application_replacement(stale).is_err());
    assert_eq!(first.application_stamp(), Some(&third.stamp));
}
