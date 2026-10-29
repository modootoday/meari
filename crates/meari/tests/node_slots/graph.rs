use super::*;
use meari::{
    application::{Language, source_modules::ModuleProgram},
    domain::{
        Node, NodeKind, Value,
        host_callbacks::{CallbackReference, CallbackSignature},
        node_slots::NodeSlot,
        source_modules::{ComponentReference, SourceModuleGraph},
    },
};

const SHELL: &str = "import type {ReactNode} from 'react';type Props={navigation:ReactNode;workspace:ReactNode;inspector:ReactNode;queue:ReactNode};export function Shell(props:Props){return <main><header>{props.navigation}</header><section>{props.workspace}</section>{props.inspector}{props.queue}<footer>stable</footer></main>;}";
const EDITOR: &str = "import {useState,useId} from 'react';type Props={label:string};export function Editor(props:Props){const [value,setValue]=useState('cold');const id=useId();return <article id={id}><span>{props.label}</span><button onClick={()=>setValue('hot')}>{value}</button></article>;}";
fn parse(entry: &str, files: &[(&str, &str)]) -> SourceModuleGraph {
    meari::parse_module_graph(
        &path(entry),
        &files
            .iter()
            .map(|(name, source)| (path(name), (*source).into()))
            .collect(),
    )
    .unwrap()
}
fn values(label: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("label".into(), Value::String(label.into())),
        (
            "onSave".into(),
            Value::CallbackReference(CallbackReference {
                binding: "save".into(),
                signature: CallbackSignature::Action,
            }),
        ),
    ])
}
fn text(node: &Node) -> String {
    match &node.kind {
        NodeKind::Text(Value::String(value)) => value.clone(),
        NodeKind::Element { children, .. } => children.iter().map(text).collect(),
        _ => panic!("projection retained unresolved slot"),
    }
}
fn id(node: &Node) -> Option<String> {
    match &node.kind {
        NodeKind::Element {
            attributes,
            children,
            ..
        } => attributes
            .iter()
            .find_map(|attribute| {
                if attribute.name == "id" {
                    if let Value::String(value) = &attribute.value {
                        Some(value.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .or_else(|| children.iter().find_map(id)),
        _ => None,
    }
}
fn reference(module: &str, export: &str) -> ComponentReference {
    ComponentReference {
        module: path(module),
        export: export.into(),
    }
}

#[test]
fn source_slots_preserve_caller_callbacks_origins_and_editor_state() {
    let caller = "import {Shell} from './shell';import {Editor} from './editor';import {Dock} from './dock';type Props={label:string;onSave:()=>void};export function Caller(props:Props){return <Shell navigation={<button onClick={()=>props.onSave()}>save</button>} workspace={<Editor label={props.label}/>} inspector={<Dock><b>inspect</b><span>{props.label}</span></Dock>} queue={null}/>;}";
    let graph = parse(
        "caller.tsx",
        &[
            ("caller.tsx", caller),
            ("shell.tsx", SHELL),
            ("editor.tsx", EDITOR),
            (
                "dock.tsx",
                "import type {ReactNode} from 'react';type Props={children:ReactNode};export function Dock(props:Props){return <aside>{props.children}</aside>;}",
            ),
        ],
    );
    let mut program = ModuleProgram::mount(graph, values("first"), "slots".into(), 1).unwrap();
    let before = program.project().unwrap().clone();
    assert!(text(&before.component.root).contains("inspectfirststable"));
    let callback = before.callback_bindings();
    assert_eq!(callback.len(), 1);
    let callback_path = callback.keys().next().unwrap();
    assert_eq!(before.origins[callback_path].module, path("caller.tsx"));
    assert!(before.origins[callback_path].span.start > 0);
    let event = before
        .event_paths()
        .find(|path| before.origins[*path].module == super::path("editor.tsx"))
        .unwrap()
        .clone();
    let owner = before.instances[&before.origins[&event].instance]
        .instance
        .clone();
    let editor_id = id(&before.component.root).unwrap();
    program.activate(&event).unwrap();
    let after = program.update_props(values("second")).unwrap();
    assert!(text(&after.component.root).contains("secondhot"));
    assert_eq!(
        after.instances[&after.origins[&event].instance].instance,
        owner
    );
    assert_eq!(id(&after.component.root).unwrap(), editor_id);
    assert_eq!(after.callback_bindings(), callback);
}

fn owned_graph() -> SourceModuleGraph {
    let mut graph = parse(
        "root.tsx",
        &[(
            "root.tsx",
            "import type {ReactNode} from 'react';type Props={body:ReactNode};export function Root(props:Props){return <main><header>shell</header>{props.body}</main>;}",
        )],
    );
    graph
        .modules
        .extend(parse("editor.tsx", &[("editor.tsx", EDITOR)]).modules);
    graph.validate().unwrap();
    graph
}
fn props(body: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([("body".into(), body)])
}
fn editor_props(label: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("label".into(), Value::String(label.into()))])
}

#[test]
fn issued_slots_keep_same_identity_and_reject_foreign_retired_and_raw_values() {
    let graph = owned_graph();
    let mut first = None;
    let mut program =
        ModuleProgram::mount_with_node_slots(graph.clone(), "owned".into(), 1, |factory| {
            let value = factory.component(
                "editor",
                reference("editor.tsx", "Editor"),
                editor_props("one"),
            )?;
            first = Some(value.clone());
            Ok(props(value))
        })
        .unwrap();
    let first = first.unwrap();
    let before = program.project().unwrap().clone();
    let event = before.event_paths().next().unwrap().clone();
    let old_instance = before.origins[&event].instance.clone();
    assert_eq!(before.calls[&old_instance].module, path("root.tsx"));
    assert!(before.calls[&old_instance].span.end > before.calls[&old_instance].span.start);
    let old_id = id(&before.component.root).unwrap();
    program.activate(&event).unwrap();
    let updated = program
        .node_slots()
        .component(
            "editor",
            reference("editor.tsx", "Editor"),
            editor_props("two"),
        )
        .unwrap();
    let next = program.update_props(props(updated)).unwrap();
    assert!(text(&next.component.root).contains("twohot"));
    assert_eq!(id(&next.component.root).unwrap(), old_id);
    let last_good = text(&next.component.root);
    assert!(
        program
            .update_props(props(first.clone()))
            .unwrap_err()
            .diagnostic
            .message
            .contains("retired")
    );
    assert_eq!(text(&program.project().unwrap().component.root), last_good);
    let mut foreign = ModuleProgram::mount(
        graph.clone(),
        props(Value::NodeSlot(NodeSlot::Empty)),
        "foreign".into(),
        1,
    )
    .unwrap();
    assert!(
        foreign
            .update_props(props(first))
            .unwrap_err()
            .diagnostic
            .message
            .contains("foreign")
    );
    let raw = Value::NodeSlot(NodeSlot::Content(vec![meari::runtime::element(
        "script",
        vec![],
        vec![],
    )]));
    let raw: Value = serde_json::from_str(&serde_json::to_string(&raw).unwrap()).unwrap();
    assert!(
        program
            .update_props(props(raw))
            .unwrap_err()
            .diagnostic
            .message
            .contains("raw authoring")
    );
    assert_eq!(text(&program.project().unwrap().component.root), last_good);
    let replaced = program
        .node_slots()
        .component(
            "replacement",
            reference("editor.tsx", "Editor"),
            editor_props("three"),
        )
        .unwrap();
    let final_ = program.update_props(props(replaced)).unwrap();
    assert!(!final_.instances.contains_key(&old_instance));
    assert!(!final_.calls.contains_key(&old_instance));
    assert_ne!(id(&final_.component.root).unwrap(), old_id);
    assert!(text(&final_.component.root).contains("threecold"));
    assert!(ModuleProgram::mount(graph, BTreeMap::new(), "missing".into(), 1).is_err());
}

#[test]
fn null_is_explicit_empty_and_source_node_values_roundtrip() {
    let source = "import type {ReactNode} from 'react';type Props={body:ReactNode};export function Example(props:Props){return <div>{props.body}<span>null is a slot value</span></div>;}";
    let parsed = meari::parse(source, Language::Tsx).unwrap();
    let rust = meari::emit(&parsed, Language::Rust).unwrap();
    assert_eq!(rust.schema_version, 10);
    let restored = meari::parse(&rust.code, Language::Rust).unwrap();
    assert!(parsed.semantic_eq(&restored));
    let tsx = meari::emit(&restored, Language::Tsx).unwrap();
    assert!(parsed.semantic_eq(&meari::parse(&tsx.code, Language::Tsx).unwrap()));
    let graph = parse("root.tsx", &[("root.tsx", source)]);
    let empty = ModuleProgram::mount(
        graph,
        props(Value::NodeSlot(NodeSlot::Empty)),
        "empty".into(),
        1,
    )
    .unwrap();
    assert_eq!(
        text(&empty.project().unwrap().component.root),
        "null is a slot value"
    );
    assert!(meari::parse("type Props={name:string};export function Wrong(props:Props){return <div title={null}/>;}",Language::Tsx).is_err());
}

#[test]
fn a_source_slot_cannot_be_consumed_twice() {
    let graph = parse(
        "caller.tsx",
        &[
            (
                "caller.tsx",
                "import {Twice} from './twice';type Props={};export function Caller(props:Props){return <Twice body={<span>owned once</span>}/>;}",
            ),
            (
                "twice.tsx",
                "import type {ReactNode} from 'react';type Props={body:ReactNode};export function Twice(props:Props){return <div>{props.body}{props.body}</div>;}",
            ),
        ],
    );
    assert!(
        ModuleProgram::mount(graph, BTreeMap::new(), "duplicate".into(), 1)
            .err()
            .unwrap()
            .diagnostic
            .message
            .contains("multiply consumed")
    );
}

#[test]
fn repeats_inside_a_source_slot_preserve_keyed_component_owners_on_reorder() {
    use meari::runtime::IntoValue;
    let graph = parse(
        "list.tsx",
        &[
            (
                "list.tsx",
                "import {Shell} from './shell';import {Counter} from './counter';type Props={refs:string[]};export function List(props:Props){return <Shell navigation={null} workspace={<ul>{props.refs.map(item=><Counter key={item} label={item}/>)}</ul>} inspector={null} queue={null}/>;}",
            ),
            ("shell.tsx", SHELL),
            (
                "counter.tsx",
                "import {useState} from 'react';type Props={label:string};export function Counter(props:Props){const [count,setCount]=useState(0);return <li><button onClick={()=>setCount(previous=>previous+1)}>{props.label}{count}</button></li>;}",
            ),
        ],
    );
    let values = |refs: Vec<&str>| BTreeMap::from([("refs".into(), refs.into_value())]);
    let mut program =
        ModuleProgram::mount(graph, values(vec!["a", "b"]), "slotRepeat".into(), 1).unwrap();
    let initial = program.project().unwrap().clone();
    let event = initial.event_paths().next().unwrap().clone();
    let owner = initial.instances[&initial.origins[&event].instance]
        .instance
        .clone();
    program.activate(&event).unwrap();
    let after = program.update_props(values(vec!["b", "a"])).unwrap();
    assert!(text(&after.component.root).contains("b0a1"));
    assert!(
        after
            .instances
            .values()
            .any(|state| state.instance == owner)
    );
}

#[test]
fn declared_slot_import_cycles_and_excessive_owned_depth_are_rejected() {
    let cycle=meari::parse_module_graph(&path("a.tsx"),&BTreeMap::from([
        (path("a.tsx"),"import type {ReactNode} from 'react';import {B} from './b';type Props={body:ReactNode};export function A(props:Props){return <B body={props.body}/>;}".into()),
        (path("b.tsx"),"import type {ReactNode} from 'react';import {A} from './a';type Props={body:ReactNode};export function B(props:Props){return <A body={props.body}/>;}".into()),
    ])).unwrap_err();
    assert!(cycle.diagnostic.message.contains("cycle"));
    let graph = parse(
        "root.tsx",
        &[(
            "root.tsx",
            "import type {ReactNode} from 'react';type Props={body:ReactNode};export function Root(props:Props){return <main>{props.body}</main>;}",
        )],
    );
    let program = ModuleProgram::mount(
        graph,
        props(Value::NodeSlot(NodeSlot::Empty)),
        "depthBudget".into(),
        1,
    )
    .unwrap();
    let before = program.project().unwrap().component.clone();
    let mut body = Value::NodeSlot(NodeSlot::Empty);
    let mut rejected = false;
    for _ in 0..132 {
        match program
            .node_slots()
            .component("nested", reference("root.tsx", "Root"), props(body))
        {
            Ok(next) => body = next,
            Err(error) => {
                assert!(error.contains("budget"));
                rejected = true;
                break;
            }
        }
    }
    assert!(rejected);
    assert!(program.project().unwrap().component.semantic_eq(&before));
}
