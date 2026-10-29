use meari::{
    application::{
        Language,
        state_program::{ReloadDisposition, ReloadPolicy, StateProgram},
    },
    domain::{Component, component_state::StateValue, state_program::StateEvent},
};

fn component() -> Component {
    meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx).unwrap()
}
fn event(program: &StateProgram) -> StateEvent {
    program
        .project()
        .unwrap()
        .events
        .into_values()
        .find(|event| event.setter == "setCount")
        .unwrap()
}
fn count(program: &StateProgram) -> f64 {
    let snapshot = program.snapshot().unwrap();
    let StateValue::Number(value) = &snapshot.values["count"] else {
        panic!()
    };
    value.as_f64().unwrap()
}

#[test]
fn replacement_preserves_values_but_executes_new_source_only_after_commit() {
    let mut program = StateProgram::mount(component(), "reload-test".into(), 1).unwrap();
    program.dispatch(&event(&program)).unwrap();
    let baseline = program.snapshot().unwrap();
    let next = include_str!("../fixtures/state-counter.tsx")
        .replace("useState(0)", "useState(100)")
        .replace("previous + 1", "previous + 10");
    let prepared = program
        .prepare_replacement(
            meari::parse(&next, Language::Tsx).unwrap(),
            ReloadPolicy::PreserveCompatible,
        )
        .unwrap();
    assert_eq!(prepared.disposition(), ReloadDisposition::Preserved);
    assert_eq!(prepared.definition_revision(), 2);
    assert_eq!(prepared.snapshot().unwrap(), baseline);
    assert_eq!(program.definition_revision(), 1);
    program.commit_replacement(prepared).unwrap();
    assert_eq!(program.definition_revision(), 2);
    assert_eq!(program.snapshot().unwrap().instance, baseline.instance);
    program.dispatch(&event(&program)).unwrap();
    assert_eq!(count(&program), 11.0);
}

#[test]
fn state_changes_and_other_commits_reject_stale_preparations() {
    let mut program = StateProgram::mount(component(), "reload-test".into(), 1).unwrap();
    let prepared = program
        .prepare_replacement(component(), ReloadPolicy::PreserveCompatible)
        .unwrap();
    program.dispatch(&event(&program)).unwrap();
    assert!(program.commit_replacement(prepared).is_err());
    assert_eq!(count(&program), 1.0);
    let prepared = program
        .prepare_replacement(component(), ReloadPolicy::PreserveCompatible)
        .unwrap();
    program.commit_replacement(prepared.clone()).unwrap();
    assert!(program.commit_replacement(prepared).is_err());
    assert_eq!(program.definition_revision(), 2);
    assert_eq!(count(&program), 1.0);
}

#[test]
fn identity_or_order_changes_require_explicit_reset_with_fresh_instance() {
    let mut program = StateProgram::mount(component(), "reload-test".into(), 1).unwrap();
    program.dispatch(&event(&program)).unwrap();
    let baseline = program.snapshot().unwrap();
    let mut reordered = component();
    reordered.states.swap(0, 1);
    assert!(
        program
            .prepare_replacement(reordered, ReloadPolicy::PreserveCompatible)
            .is_err()
    );
    let mut renamed = component();
    renamed.name = "FreshCounter".into();
    assert!(
        program
            .prepare_replacement(renamed.clone(), ReloadPolicy::PreserveCompatible)
            .is_err()
    );
    let prepared = program
        .prepare_replacement(renamed, ReloadPolicy::Reset)
        .unwrap();
    assert_eq!(prepared.disposition(), ReloadDisposition::Reset);
    assert_ne!(prepared.snapshot().unwrap().instance, baseline.instance);
    assert_eq!(program.snapshot().unwrap(), baseline);
    program.commit_replacement(prepared).unwrap();
    assert_eq!(count(&program), 0.0);
    assert_eq!(program.snapshot().unwrap().revision, 0);
}

#[test]
fn invalid_definition_does_not_change_current_program() {
    let program = StateProgram::mount(component(), "reload-test".into(), 1).unwrap();
    let before = program.snapshot().unwrap();
    let mut invalid = component();
    invalid.states[1].name = "count".into();
    assert!(
        program
            .prepare_replacement(invalid, ReloadPolicy::Reset)
            .is_err()
    );
    assert_eq!(program.snapshot().unwrap(), before);
    assert_eq!(program.definition_revision(), 1);
}
