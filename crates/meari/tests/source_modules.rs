use meari::{application::Language, domain::build::BuildPath};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn sources() -> BTreeMap<BuildPath, String> {
    BTreeMap::from([
        (path("viewing.tsx"), "import { WindowChrome } from './components/window-chrome'; type Props={productName:string}; export function Viewing(props:Props){return <WindowChrome title={props.productName}/>;}".into()),
        (path("components/window-chrome.tsx"), "type Props={title:string}; export function WindowChrome(props:Props){return <header><h1>{props.title}</h1></header>;}".into()),
    ])
}

#[test]
fn consumer_local_import_resolves_without_deleting_source_or_changing_single_source_policy() {
    let input = sources();
    let before = input.clone();
    assert!(meari::parse(&input[&path("viewing.tsx")], Language::Tsx).is_err());
    let graph = meari::parse_module_graph(&path("viewing.tsx"), &input).unwrap();
    let viewing = graph.definition(&graph.entry).unwrap();
    let import = &viewing.imports["WindowChrome"];
    assert_eq!(import.target.module, path("components/window-chrome.tsx"));
    assert_eq!(import.target.export, "WindowChrome");
    assert_eq!(
        &input[&path("viewing.tsx")][import.span.start..import.span.end],
        "import { WindowChrome } from './components/window-chrome';"
    );
    assert_eq!(graph.modules.len(), 2);
    assert_eq!(input, before);
    graph.fingerprint().unwrap();
}

#[test]
fn graph_errors_retain_the_importing_file_and_original_import_range() {
    let mut input = sources();
    input.remove(&path("components/window-chrome.tsx"));
    let error = meari::parse_module_graph(&path("viewing.tsx"), &input).unwrap_err();
    assert_eq!(error.file, path("viewing.tsx"));
    assert_eq!(error.diagnostic.span.start, 0);
    assert!(error.diagnostic.span.end > 0);
    let mut input = sources();
    let source = input
        .get_mut(&path("components/window-chrome.tsx"))
        .unwrap();
    *source = source.replace("function WindowChrome", "function DifferentChrome");
    let error = meari::parse_module_graph(&path("viewing.tsx"), &input).unwrap_err();
    assert_eq!(error.file, path("viewing.tsx"));
    assert!(error.diagnostic.message.contains("export"));
}

#[test]
fn module_instances_keep_separate_state_ids_and_child_state_when_parent_props_change() {
    use meari::{
        application::source_modules::ModuleProgram,
        domain::{Value, component_state::StateValue},
    };
    let sources = BTreeMap::from([
        (path("parent.tsx"), "import { Counter } from './counter'; type Props={caption:string}; export function Parent(props:Props){return <main><Counter caption={props.caption}/><Counter caption={props.caption}/></main>; }".into()),
        (path("counter.tsx"), "import { useState,useId } from 'react'; type Props={caption:string}; export function Counter(props:Props){const field=useId(); const [count,setCount]=useState(0); return <section><label htmlFor={field}>{props.caption}</label><input id={field}/><button onClick={() => setCount(previous => previous+1)}>{count}</button></section>; }".into()),
    ]);
    let graph = meari::parse_module_graph(&path("parent.tsx"), &sources).unwrap();
    let mut program = ModuleProgram::mount(
        graph,
        BTreeMap::from([("caption".into(), Value::String("A".into()))]),
        "module-test".into(),
        1,
    )
    .unwrap();
    let before = program.project().unwrap().clone();
    assert_eq!(before.instances.len(), 3);
    assert_ne!(
        before.instances["root@0/0"].instance,
        before.instances["root@0/1"].instance
    );
    let after = program.activate("0/0/2").unwrap();
    assert_eq!(
        after.instances["root@0/0"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(1.0).unwrap())
    );
    assert_eq!(
        after.instances["root@0/1"].values["count"],
        StateValue::Number(serde_json::Number::from_f64(0.0).unwrap())
    );
    let updated = program
        .update_props(BTreeMap::from([(
            "caption".into(),
            Value::String("B".into()),
        )]))
        .unwrap();
    assert_eq!(
        updated.instances["root@0/0"].values["count"],
        after.instances["root@0/0"].values["count"]
    );
    assert_eq!(
        updated.instances["root@0/0"].instance,
        after.instances["root@0/0"].instance
    );
    assert_eq!(updated.origins["0/0/2"].module, path("counter.tsx"));
    let plan = meari::application::dom::prepare(&updated.component).unwrap();
    assert!(format!("{plan:?}").contains('B'));
    let previous = program.project().unwrap().instances.clone();
    assert!(program.update_props(BTreeMap::new()).is_err());
    assert_eq!(program.project().unwrap().instances, previous);
}
