use meari::{
    application::{
        Language, binding,
        dom::{self, DomNode},
        state_program::StateProgram,
    },
    domain::Value,
};
use std::collections::BTreeMap;

#[test]
fn shared_bindings_and_dom_plan_resolve_ids_props_and_boolean_attributes() {
    let source = "import {useId,useState} from 'react';type Props={label:string};export function Demo(props:Props){const field=useId();const [disabled,setDisabled]=useState(false);return <div><label htmlFor={field}>{props.label}</label><input id={field} disabled={disabled} aria-hidden={false}/></div>}";
    let definition = meari::parse(source, Language::Tsx).unwrap();
    assert!(binding::bind(&definition, &BTreeMap::new()).is_err());
    let bound = binding::bind(
        &definition,
        &BTreeMap::from([("label".into(), Value::String("Name".into()))]),
    )
    .unwrap();
    assert!(bound.props.is_empty() && bound.hook_order.is_empty());
    let program = StateProgram::mount(bound, "dom-plan".into(), 1).unwrap();
    let DomNode::Element { children, .. } =
        dom::prepare(&program.project().unwrap().component).unwrap()
    else {
        panic!()
    };
    let DomNode::Element {
        attributes: label,
        children: label_text,
        ..
    } = &children[0]
    else {
        panic!()
    };
    let DomNode::Element {
        attributes: input, ..
    } = &children[1]
    else {
        panic!()
    };
    assert_eq!(label["for"], input["id"]);
    assert_eq!(label_text[0], DomNode::Text("Name".into()));
    assert!(!input.contains_key("disabled"));
    assert_eq!(input["aria-hidden"], "false");
    let scoped = binding::bind_with_scope(
        &definition,
        &BTreeMap::from([("label".into(), Value::String("Name".into()))]),
        Some("abcd"),
    )
    .unwrap();
    let DomNode::Element { children, .. } = dom::prepare(
        &StateProgram::mount(scoped, "scoped-dom".into(), 1)
            .unwrap()
            .project()
            .unwrap()
            .component,
    )
    .unwrap() else {
        panic!()
    };
    let DomNode::Element { attributes, .. } = &children[1] else {
        panic!()
    };
    assert_eq!(attributes["id"], "meari-abcd-field");
}

#[test]
fn dom_plan_rejects_unmapped_execution_controlled_state_and_normalized_duplicates() {
    for body in [
        "<script>bad</script>",
        "<div OnClick=\"bad\"/>",
        "<a href=\"javascript:bad()\">Go</a>",
        "<div className=\"a\" class=\"b\"/>",
        "<Button asChild>Go</Button>",
        "<input>Ignored</input>",
    ] {
        let component = meari::parse(
            &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
            Language::Tsx,
        )
        .unwrap();
        assert!(dom::prepare(&component).is_err(), "{body}");
    }
    let component = meari::parse("import {useState} from 'react';type Props={};export function Demo(props:Props){const [value,setValue]=useState('');return <input value={value}/>;}",Language::Tsx).unwrap();
    assert!(dom::validate_source(&component).is_err());
    let component = meari::parse(
        "type Props={};export function Demo(props:Props){return <input value=\"fixed\"/>;}",
        Language::Tsx,
    )
    .unwrap();
    assert!(dom::validate_source(&component).is_err());
}

#[test]
fn dom_plan_applies_design_component_recipes_with_variant_and_size() {
    let component = meari::parse(
        "type Props={};export function Demo(props:Props){return <Card><Button variant=\"outline\" size=\"sm\">Go</Button></Card>;}",
        Language::Tsx,
    )
    .unwrap();
    let DomNode::Element { tag, children, .. } = dom::prepare(&component).unwrap() else {
        panic!()
    };
    assert_eq!(tag, "div");
    let DomNode::Element {
        tag, attributes, ..
    } = &children[0]
    else {
        panic!()
    };
    assert_eq!(tag, "button");
    assert!(!attributes.contains_key("variant") && !attributes.contains_key("size"));
    assert_eq!(
        attributes["class"],
        meari::domain::design::components::recipe("Button", "outline", "sm").trim()
    );
}
