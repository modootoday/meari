use super::*;
use crate::application::hook_values::Budget;

fn updater_definition() -> Component {
    crate::parse(
        include_str!("../../../fixtures/hooks/structured-updater.tsx"),
        Language::Tsx,
    )
    .unwrap()
}

fn mount_definition(doc: &mut HookDocument, id: &InstanceHandle, definition: Component) {
    let prepared = doc
        .prepare_mounts(vec![Mount {
            instance: id.clone(),
            definition,
            definition_revision: 1,
        }])
        .unwrap();
    doc.prepare_publication(prepared).unwrap().publish();
}

#[test]
fn parsed_functional_spread_uses_latest_previous_and_preserves_captured_nested_alias() {
    let id = ids()[0].clone();
    let definition = updater_definition();
    let update = event(&definition);
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount_definition(&mut doc, &id, definition);
    let old = doc.snapshot(&id).unwrap().roots["state"].clone();
    let old_rows = doc.values().field(&old, "rows").unwrap().unwrap();
    let actions = (0..2)
        .map(|_| doc.capture(&id, update.clone()).unwrap())
        .collect();
    let prepared = doc.prepare_actions(actions).unwrap();
    let next = prepared.root(&id, "state").unwrap();
    let count = prepared.values().field(next, "count").unwrap().unwrap();
    let rows = prepared.values().field(next, "rows").unwrap().unwrap();
    assert_eq!(prepared.values().read_number(&count), Ok(2.0));
    assert_eq!(prepared.values().same_value(&rows, &old_rows), Ok(true));
    assert_eq!(
        doc.values()
            .number(&doc.values().field(&old, "count").unwrap().unwrap()),
        Ok(0.0)
    );
    doc.prepare_publication(prepared).unwrap().publish();
    assert_eq!(self::count(&doc, &id), 2.0);
}

#[test]
fn parsed_captured_spread_uses_original_frame_and_same_root_setter_remains_a_noop() {
    let source = include_str!("../../../fixtures/hooks/structured-updater.tsx").replace(
        "current => ({ ...current, count: current.count + 1, rows: state.rows })",
        "({ ...state, count: state.count + 1 })",
    );
    let definition = crate::parse(&source, Language::Tsx).unwrap();
    let update = event(&definition);
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount_definition(&mut doc, &id, definition);
    let actions = (0..2)
        .map(|_| doc.capture(&id, update.clone()).unwrap())
        .collect();
    let prepared = doc.prepare_actions(actions).unwrap();
    doc.prepare_publication(prepared).unwrap().publish();
    assert_eq!(count(&doc, &id), 1.0);
    for replacement in ["state", "current => current"] {
        let definition = crate::parse(
            &source.replace("({ ...state, count: state.count + 1 })", replacement),
            Language::Tsx,
        )
        .unwrap();
        let baseline = doc.stamp();
        let prepared = doc
            .prepare_actions(vec![doc.capture(&id, event(&definition)).unwrap()])
            .unwrap();
        assert!(!prepared.changed());
        assert_eq!(
            doc.prepare_publication(prepared).unwrap().publish(),
            baseline
        );
    }
}

#[test]
fn absent_array_element_cannot_publish_an_undefined_value_into_a_numeric_slot() {
    let source = include_str!("../../../fixtures/hooks/structured-updater.tsx")
        .replace("current.count + 1", "current.rows[50]");
    let definition = crate::parse(&source, Language::Tsx).unwrap();
    let update = event(&definition);
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount_definition(&mut doc, &id, definition);
    let baseline = doc.stamp();
    let usage = doc.values().usage();
    let failure = doc
        .prepare_actions(vec![doc.capture(&id, update).unwrap()])
        .err()
        .unwrap();
    assert!(failure.diagnostic.message.contains("declared state shape"));
    assert_eq!(failure.actions.len(), 1);
    assert!(failure.candidate.is_some());
    assert_eq!(doc.stamp(), baseline);
    assert_eq!(doc.values().usage(), usage);
    assert_eq!(count(&doc, &id), 0.0);
    let rejected = doc
        .owner
        .prepare_publish(failure.candidate.unwrap())
        .err()
        .unwrap();
    assert_eq!(rejected.error, Error::InvalidExpression);
}

#[test]
fn empty_batch_cannot_issue_prepublication_frame_actions_but_keeps_published_action_live() {
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount(&mut doc, std::slice::from_ref(&id));
    let baseline = doc.stamp();
    let published = doc.capture(&id, event(&definition())).unwrap();
    let prepared = doc.prepare_actions(Vec::new()).unwrap();
    assert!(!prepared.changed());
    assert!(prepared.capture(&id, event(&definition())).is_err());
    assert_eq!(
        doc.prepare_publication(prepared).unwrap().publish(),
        baseline
    );
    let prepared = doc.prepare_actions(vec![published]).unwrap();
    assert!(prepared.changed());
    doc.prepare_publication(prepared).unwrap().publish();
    assert_eq!(count(&doc, &id), 1.0);
}
use crate::{
    application::{Language, component_state::ComponentState},
    domain::{
        NodeKind, Value as SourceValue,
        component_state::{InstanceKey, StateSchema},
    },
};
fn definition() -> Component {
    crate::parse(
        include_str!("../../../fixtures/hooks/structured-state.tsx"),
        Language::Tsx,
    )
    .unwrap()
}
fn event(component: &Component) -> StateEvent {
    let NodeKind::Element { attributes, .. } = &component.root.kind else {
        panic!()
    };
    attributes
        .iter()
        .find_map(|attribute| match &attribute.value {
            SourceValue::Event(event) => Some(event.clone()),
            _ => None,
        })
        .unwrap()
}
fn ids() -> [InstanceHandle; 2] {
    let mut state = ComponentState::new("hook-root-instances".into(), 1).unwrap();
    ["left", "right"].map(|key| {
        state
            .mount(
                None,
                InstanceKey::Explicit(key.into()),
                "StructuredState".into(),
                StateSchema {
                    revision: "instance-v1".into(),
                    slots: BTreeMap::new(),
                },
            )
            .unwrap()
    })
}
fn mount(doc: &mut HookDocument, ids: &[InstanceHandle]) {
    let prepared = doc
        .prepare_mounts(
            ids.iter()
                .map(|instance| Mount {
                    instance: instance.clone(),
                    definition: definition(),
                    definition_revision: 1,
                })
                .collect(),
        )
        .unwrap();
    doc.prepare_publication(prepared).unwrap().publish();
}
fn count(doc: &HookDocument, instance: &InstanceHandle) -> f64 {
    let snapshot = doc.snapshot(instance).unwrap();
    let count = doc
        .values()
        .field(&snapshot.roots["state"], "count")
        .unwrap()
        .unwrap();
    doc.values().number(&count).unwrap()
}

#[test]
fn parsed_instances_share_one_candidate_and_heap_roots_publish_together() {
    let ids = ids();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount(&mut doc, &ids);
    let baseline = doc.stamp();
    let actions = ids
        .iter()
        .map(|id| doc.capture(id, event(&definition())).unwrap())
        .collect();
    let prepared = doc.prepare_actions(actions).unwrap();
    assert!(prepared.changed());
    for id in &ids {
        let root = prepared.root(id, "state").unwrap();
        let value = prepared.values().field(root, "count").unwrap().unwrap();
        assert_eq!(prepared.values().read_number(&value), Ok(1.0));
        assert_eq!(count(&doc, id), 0.0);
    }
    let prepared = doc.prepare_publication(prepared).unwrap().abort();
    assert_eq!(doc.stamp(), baseline);
    assert_eq!(count(&doc, &ids[0]), 0.0);
    let next = doc.prepare_publication(prepared).unwrap().publish();
    assert_eq!(next.revision, baseline.revision + 1);
    for id in &ids {
        assert_eq!(count(&doc, id), 1.0);
    }
}

#[test]
fn unpublished_frame_actions_are_inactive_and_old_frame_actions_expire_after_publish() {
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    let prepared = doc
        .prepare_mounts(vec![Mount {
            instance: id.clone(),
            definition: definition(),
            definition_revision: 1,
        }])
        .unwrap();
    let future = prepared.capture(&id, event(&definition())).unwrap();
    let failure = doc.prepare_actions(vec![future]).err().unwrap();
    assert!(failure.diagnostic.message.contains("scope"));
    assert_eq!(failure.actions.len(), 1);
    doc.prepare_publication(prepared).unwrap().publish();
    let stale = doc.capture(&id, event(&definition())).unwrap();
    let prepared = doc.prepare_actions(failure.actions).unwrap();
    doc.prepare_publication(prepared).unwrap().publish();
    assert!(doc.prepare_actions(vec![stale]).is_err());
    assert_eq!(count(&doc, &id), 1.0);
}

#[test]
fn second_instance_budget_failure_keeps_both_instances_and_original_failure_candidate() {
    let ids = ids();
    let mut doc = HookDocument::new(1, Budget::new(18, 64, 1024, 32).unwrap()).unwrap();
    mount(&mut doc, &ids);
    let baseline = doc.stamp();
    let usage = doc.values().usage();
    let actions = ids
        .iter()
        .map(|id| doc.capture(id, event(&definition())).unwrap())
        .collect();
    let failure = doc.prepare_actions(actions).err().unwrap();
    assert_eq!(failure.cause, Some(Error::BudgetExceeded));
    assert!(failure.candidate.is_some());
    assert_eq!(failure.actions.len(), 2);
    assert_eq!(doc.values().usage(), usage);
    assert_eq!(doc.stamp(), baseline);
    for id in &ids {
        assert_eq!(count(&doc, id), 0.0);
    }
    drop(failure);
    let mut peer = doc.values().begin().unwrap();
    assert!(peer.null().is_ok());
}

#[test]
fn two_functional_scalar_updates_use_latest_candidate_and_noop_keeps_original_epoch() {
    let source = "import { useState } from \"react\"; export function Toggle() { const [flag, setFlag] = useState(false); return <button onClick={() => setFlag(previous => !previous)}>toggle</button>; }";
    let component = crate::parse(source, Language::Tsx).unwrap();
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    let prepared = doc
        .prepare_mounts(vec![Mount {
            instance: id.clone(),
            definition: component.clone(),
            definition_revision: 1,
        }])
        .unwrap();
    doc.prepare_publication(prepared).unwrap().publish();
    let baseline = doc.stamp();
    let usage = doc.values().usage();
    let still_live = doc.capture(&id, event(&component)).unwrap();
    let first = doc.capture(&id, event(&component)).unwrap();
    let second = doc.capture(&id, event(&component)).unwrap();
    let prepared = doc.prepare_actions(vec![first, second]).unwrap();
    assert!(!prepared.changed());
    assert!(prepared.capture(&id, event(&component)).is_err());
    assert_eq!(
        doc.prepare_publication(prepared).unwrap().publish(),
        baseline
    );
    assert_eq!(doc.values().usage(), usage);
    let prepared = doc.prepare_actions(vec![still_live]).unwrap();
    assert!(prepared.changed());
    doc.prepare_publication(prepared).unwrap().publish();
    let snapshot = doc.snapshot(&id).unwrap();
    let value = &snapshot.roots["flag"];
    assert_eq!(doc.values().boolean(value), Ok(true));
}

#[test]
fn guard_drop_closed_and_foreign_documents_do_not_publish_or_lose_the_source_action() {
    let id = ids()[0].clone();
    let mut doc = HookDocument::new(1, Budget::default()).unwrap();
    mount(&mut doc, std::slice::from_ref(&id));
    let mut foreign = HookDocument::new(1, Budget::default()).unwrap();
    mount(&mut foreign, std::slice::from_ref(&id));
    let action = doc.capture(&id, event(&definition())).unwrap();
    let failure = foreign.prepare_actions(vec![action]).err().unwrap();
    assert_eq!(failure.actions.len(), 1);
    let prepared = doc.prepare_actions(failure.actions).unwrap();
    drop(doc.prepare_publication(prepared).unwrap());
    assert_eq!(count(&doc, &id), 0.0);
    let action = doc.capture(&id, event(&definition())).unwrap();
    doc.close();
    let failure = doc.prepare_actions(vec![action]).err().unwrap();
    assert_eq!(failure.cause, Some(Error::Closed));
    assert_eq!(failure.actions.len(), 1);
    assert!(doc.snapshot(&id).is_err());
    assert_eq!(count(&foreign, &id), 0.0);
}
