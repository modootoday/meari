use meari::{
    application::{Language, binding},
    domain::{PropType, Value, array_value::ArrayValue},
    runtime::IntoValue,
};
use std::collections::BTreeMap;

#[test]
fn typed_array_props_roundtrip_between_tsx_and_rust() {
    let source = "type Props={refs:string[]; flags:Array<boolean>; nested:Array<string[]>};export function Arrays(props:Props){return <p>arrays</p>;}";
    let component = meari::parse(source, Language::Tsx).unwrap();
    for language in [Language::Tsx, Language::Rust] {
        let generated = meari::emit(&component, language).unwrap();
        assert_eq!(generated.schema_version, 8);
        let reparsed = meari::parse(&generated.code, language).unwrap();
        assert!(component.semantic_eq(&reparsed));
    }
    let props = BTreeMap::from([
        ("refs".into(), vec!["cue-a", "cue-b"].into_value()),
        ("flags".into(), Vec::<bool>::new().into_value()),
        ("nested".into(), vec![vec!["원문🦀"], vec![]].into_value()),
    ]);
    binding::bind(&component, &props).unwrap();
    let mut wrong = props;
    wrong.insert("refs".into(), Vec::<bool>::new().into_value());
    assert!(binding::bind(&component, &wrong).is_err());
}

#[test]
fn runtime_vectors_retain_empty_types_order_and_unicode() {
    assert_eq!(
        Vec::<String>::new().into_value(),
        Value::Array(ArrayValue {
            element: PropType::String,
            items: vec![]
        })
    );
    let values = vec!["cue-b", "원문🦀", "cue-a"].into_value();
    assert_eq!(
        values,
        Value::Array(ArrayValue::strings(vec![
            "cue-b".into(),
            "원문🦀".into(),
            "cue-a".into(),
        ]))
    );
    assert!(PropType::Array(Box::new(PropType::String)).accepts(&values));
    assert!(!PropType::Array(Box::new(PropType::Bool)).accepts(&values));
}

#[test]
fn module_forwarding_preserves_both_refs_and_rejects_wrong_child_types() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{NodeKind, build::BuildPath},
    };
    let entry: BuildPath = "parent.tsx".to_owned().try_into().unwrap();
    let child: BuildPath = "child.tsx".to_owned().try_into().unwrap();
    let parent = "import { Child } from './child';type Props={refs:string[]};export function Parent(props:Props){return <Child refs={props.refs}/>;}";
    let values = BTreeMap::from([("refs".into(), vec!["cue-a", "cue-b"].into_value())]);
    let sources = BTreeMap::from([
        (entry.clone(), parent.into()),
        (
            child.clone(),
            "type Props={refs:string[]};export function Child(props:Props){return <p>refs</p>;}"
                .into(),
        ),
    ]);
    let graph = meari::parse_module_graph(&entry, &sources).unwrap();
    let bound = binding::bind(&graph.modules[&entry].component, &values).unwrap();
    let NodeKind::Element { attributes, .. } = &bound.root.kind else {
        panic!()
    };
    assert_eq!(attributes[0].value, values["refs"]);
    assert!(meari::emit(&bound, Language::Tsx).is_err());
    let program = ModuleProgram::mount(graph, values.clone(), "array-forward".into(), 1).unwrap();
    assert_eq!(program.project().unwrap().instances.len(), 2);
    let mut wrong = sources;
    wrong.insert(
        child,
        "type Props={refs:boolean[]};export function Child(props:Props){return <p>refs</p>;}"
            .into(),
    );
    let graph = meari::parse_module_graph(&entry, &wrong).unwrap();
    assert!(ModuleProgram::mount(graph, values, "array-forward".into(), 1).is_err());
}
