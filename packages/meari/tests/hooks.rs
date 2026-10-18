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
