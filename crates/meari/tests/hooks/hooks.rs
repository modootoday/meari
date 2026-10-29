use meari::{
    application::Language,
    domain::{NodeKind, Value},
};

#[test]
fn use_id_roundtrips_without_becoming_a_literal_or_prop() {
    let source = r#""use client";import {useId} from "react";type Props={};export function Demo(props:Props){const id=useId();const help=useId();return <div><label htmlFor={id}>Name</label><input id={id} aria-describedby={help}/><p id={help}>Help</p></div>;}"#;
    let component = meari::parse(source, Language::Tsx).unwrap();
    assert_eq!(meari::domain::hooks::ids(&component.root).len(), 2);
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        let parsed = meari::parse(&output.code, language).unwrap();
        assert!(component.semantic_eq(&parsed), "{}", output.code);
    }
    assert_eq!(meari::runtime::use_id("id"), "bridge-hook-id");
    let NodeKind::Element { children, .. } = component.root.kind else {
        panic!()
    };
    assert!(
        matches!(&children[1].kind,NodeKind::Element{attributes,..} if attributes.iter().any(|a| a.value == Value::Id("id".into())))
    );
}

#[test]
fn use_id_requires_unconditional_named_used_unique_bindings() {
    for source in [
        "type Props={};export function Demo(props:Props){return <input id={missing}/>;}",
        "import {useId} from 'react';type Props={};export function Demo(props:Props){const id=useId();return <div/>;}",
        "import {useId} from 'react';type Props={};export function Demo(props:Props){let id=useId();return <input id={id}/>;}",
        "import {useId} from 'react';type Props={};export function Demo(props:Props){const id=useId('seed');return <input id={id}/>;}",
        "import {useId as makeId} from 'react';type Props={};export function Demo(props:Props){const id=makeId();return <input id={id}/>;}",
        "import {useState} from 'react';type Props={};export function Demo(props:Props){const id=useState(false);return <input id={id}/>;}",
    ] {
        assert!(meari::parse(source, Language::Tsx).is_err(), "{source}");
    }
}

#[test]
fn mixed_hook_order_roundtrips_and_state_updates_preserve_id_links() {
    use meari::{application::state_program::StateProgram, domain::hooks::HookSlot};
    let component = meari::parse(
        include_str!("../../fixtures/mixed-hooks.tsx"),
        Language::Tsx,
    )
    .unwrap();
    assert_eq!(
        component.hook_order,
        vec![
            HookSlot::Id("field".into()),
            HookSlot::State("count".into()),
            HookSlot::Id("help".into()),
            HookSlot::State("enabled".into())
        ]
    );
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&component, language).unwrap();
        let parsed = meari::parse(&output.code, language).unwrap();
        assert!(component.semantic_eq(&parsed), "{}", output.code);
        if matches!(language, Language::Rust) {
            assert!(
                output.code.find("use_id(\"field\");").unwrap()
                    < output
                        .code
                        .find("state_declaration(state, \"count\"")
                        .unwrap()
            );
            assert!(
                output
                    .code
                    .find("state_declaration(state, \"count\"")
                    .unwrap()
                    < output.code.find("use_id(\"help\");").unwrap()
            );
        }
    }
    let mut program = StateProgram::mount(component.clone(), "mixed".into(), 1).unwrap();
    let initial = program.project().unwrap();
    let event = initial.events["0/4"].clone();
    let updated = program.dispatch(&event).unwrap();
    assert!(updated.component.hook_order.is_empty());
    assert_eq!(
        meari::domain::hooks::ids(&initial.component.root),
        meari::domain::hooks::ids(&updated.component.root)
    );
    let mut malformed = component.clone();
    malformed.hook_order.remove(0);
    assert!(meari::emit(&malformed, Language::Rust).is_err());
    let mut malformed = component;
    malformed.states[0].setter = "field".into();
    assert!(meari::emit(&malformed, Language::Tsx).is_err());
}
