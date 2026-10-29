use meari::{
    application::{
        Language,
        state_program::{ReloadPolicy, StateProgram},
    },
    domain::{
        component_state::StateValue,
        input_event::{InputKind, InputPayload, InputStateBinding},
    },
};

fn program(name: &str) -> StateProgram {
    StateProgram::mount(
        meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx).unwrap(),
        name.into(),
        1,
    )
    .unwrap()
}

#[test]
fn typed_input_projects_text_and_checked_values_and_rejects_wrong_payload_without_mutation() {
    let mut program = program("input");
    let text = program
        .input_handler(InputStateBinding {
            setter: "setLabel".into(),
            kind: InputKind::Text,
        })
        .unwrap();
    let check = program
        .input_handler(InputStateBinding {
            setter: "setDisabled".into(),
            kind: InputKind::Checked,
        })
        .unwrap();
    assert!(
        program
            .input_handler(InputStateBinding {
                setter: "setCount".into(),
                kind: InputKind::Text
            })
            .is_err()
    );
    let before = program.snapshot().unwrap();
    assert!(
        program
            .dispatch_input(&text, &InputPayload::Checked(true))
            .is_err()
    );
    assert_eq!(program.snapshot().unwrap(), before);
    let projection = program
        .dispatch_input(&text, &InputPayload::Text("한글🦀 draft".into()))
        .unwrap();
    assert!(projection.component.states.is_empty());
    assert_eq!(
        program.snapshot().unwrap().values["label"],
        StateValue::String("한글🦀 draft".into())
    );
    program
        .dispatch_input(&check, &InputPayload::Checked(true))
        .unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["disabled"],
        StateValue::Bool(true)
    );
    program
        .dispatch_input(&text, &InputPayload::Text(String::new()))
        .unwrap();
    assert_eq!(
        program.snapshot().unwrap().values["label"],
        StateValue::String(String::new())
    );
}

#[test]
fn input_handlers_are_scoped_to_mount_and_definition_and_survive_ordinary_state_updates() {
    let mut first = program("first-input");
    let mut other = program("other-input");
    let binding = InputStateBinding {
        setter: "setLabel".into(),
        kind: InputKind::Text,
    };
    let handler = first.input_handler(binding.clone()).unwrap();
    assert_eq!(handler, first.input_handler(binding.clone()).unwrap());
    let before = other.snapshot().unwrap();
    assert!(
        other
            .dispatch_input(&handler, &InputPayload::Text("foreign".into()))
            .is_err()
    );
    assert_eq!(other.snapshot().unwrap(), before);
    first
        .dispatch_input(&handler, &InputPayload::Text("one".into()))
        .unwrap();
    first
        .dispatch_input(&handler, &InputPayload::Text("two".into()))
        .unwrap();
    let component =
        meari::parse(include_str!("../fixtures/state-counter.tsx"), Language::Tsx).unwrap();
    let replacement = first
        .prepare_replacement(component, ReloadPolicy::PreserveCompatible)
        .unwrap();
    first.commit_replacement(replacement).unwrap();
    let current = first.snapshot().unwrap();
    assert!(
        first
            .dispatch_input(&handler, &InputPayload::Text("late".into()))
            .is_err()
    );
    assert_eq!(first.snapshot().unwrap(), current);
    let refreshed = first.input_handler(binding).unwrap();
    first
        .dispatch_input(&refreshed, &InputPayload::Text("fresh".into()))
        .unwrap();
}
