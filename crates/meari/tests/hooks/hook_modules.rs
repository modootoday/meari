use meari::{
    application::{
        dom,
        hook_document::HookDocument,
        hook_values::Budget,
        source_modules::{ModuleProgram, ModuleProjection, PreparedHookModule},
    },
    domain::{Value, build::BuildPath, source_modules::SourceModuleGraph},
};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn graph() -> SourceModuleGraph {
    meari::parse_module_graph(
        &path("root.tsx"),
        &BTreeMap::from([
            (
                path("root.tsx"),
                include_str!("../../fixtures/hooks/modules/root.tsx").into(),
            ),
            (
                path("counter.tsx"),
                include_str!("../../fixtures/hooks/modules/counter.tsx").into(),
            ),
        ]),
    )
    .unwrap()
}
fn props(caption: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("caption".into(), Value::String(caption.into())),
        ("disabled".into(), Value::Bool(false)),
    ])
}
fn publish(owner: &mut HookDocument, prepared: PreparedHookModule) -> ModuleProgram {
    prepared
        .publish_with(owner, |program| {
            dom::prepare(&program.project().unwrap().component)
        })
        .unwrap()
        .0
}
fn mount() -> (HookDocument, ModuleProgram) {
    let mut owner = HookDocument::new(1, Budget::default()).unwrap();
    let prepared =
        ModuleProgram::prepare_hook_mount(graph(), props("before"), "hooks".into(), 1, &owner)
            .unwrap();
    let program = publish(&mut owner, prepared);
    (owner, program)
}
fn count(owner: &HookDocument, projection: &ModuleProjection, key: &str) -> f64 {
    let root = owner
        .snapshot(projection.instance(key).unwrap())
        .unwrap()
        .roots["state"]
        .clone();
    owner
        .values()
        .number(&owner.values().field(&root, "count").unwrap().unwrap())
        .unwrap()
}

#[test]
fn real_module_census_projects_two_instances_and_publishes_one_heap_candidate() {
    let (mut owner, program) = mount();
    let before = program.project().unwrap();
    assert!(before.uses_hook_document());
    assert!(before.instances.is_empty()); // Live roots must not become scalar JSON snapshots.
    assert_ne!(before.instance("root@0/0"), before.instance("root@0/1"));
    let prepared = program
        .prepare_hook_actions(
            &owner,
            vec![
                before.hook_action("0/0/0").unwrap(),
                before.hook_action("0/1/0").unwrap(),
            ],
        )
        .unwrap();
    let next = prepared.program().project().unwrap();
    assert_eq!(next.instance("root@0/0"), before.instance("root@0/0"));
    assert_eq!(count(&owner, before, "root@0/0"), 0.0);
    let program = publish(&mut owner, prepared);
    assert_eq!(count(&owner, program.project().unwrap(), "root@0/0"), 1.0);
    assert_eq!(count(&owner, program.project().unwrap(), "root@0/1"), 1.0);
}

#[test]
fn props_only_frame_preserves_root_identity_and_invalidates_previous_frame_actions() {
    let (mut owner, program) = mount();
    let instance = program
        .project()
        .unwrap()
        .instance("root@0/0")
        .unwrap()
        .clone();
    let root = owner.snapshot(&instance).unwrap().roots["state"].clone();
    let stale = program.project().unwrap().hook_action("0/0/0").unwrap();
    let previous = program.clone();
    let prepared = program.prepare_hook_props(&owner, props("after")).unwrap();
    assert!(prepared.changed());
    let program = publish(&mut owner, prepared);
    let next = owner.snapshot(&instance).unwrap().roots["state"].clone();
    assert_eq!(owner.values().same_value(&root, &next), Ok(true));
    let failure = previous
        .prepare_hook_actions(&owner, vec![stale])
        .err()
        .unwrap();
    assert_eq!(failure.rejected_actions.len(), 1);
    let noop = program.prepare_hook_props(&owner, props("after")).unwrap();
    assert!(!noop.changed());
}

#[test]
fn rejected_application_receipt_keeps_old_frame_live_and_returns_original_candidate() {
    let (mut owner, program) = mount();
    let stamp = owner.stamp();
    let prepared = program.prepare_hook_action(&owner, "0/0/0").unwrap();
    let unpublished = prepared
        .program()
        .project()
        .unwrap()
        .hook_action("0/0/0")
        .unwrap();
    assert!(owner.prepare_actions(vec![unpublished]).is_err());
    let failure = prepared
        .publish_with(&mut owner, |_| Err::<(), _>("receipt rejected"))
        .err()
        .unwrap();
    assert_eq!(owner.stamp(), stamp);
    assert_eq!(count(&owner, program.project().unwrap(), "root@0/0"), 0.0);
    let retry = publish(&mut owner, failure.prepared);
    assert_eq!(count(&owner, retry.project().unwrap(), "root@0/0"), 1.0);
}

#[test]
fn failed_props_projection_and_foreign_owner_leave_original_program_and_roots_usable() {
    let (owner, program) = mount();
    let baseline = owner.stamp();
    let failure = program
        .prepare_hook_props(
            &owner,
            BTreeMap::from([
                ("caption".into(), Value::Bool(true)),
                ("disabled".into(), Value::Bool(false)),
            ]),
        )
        .err()
        .unwrap();
    assert!(failure.candidate().is_some());
    assert_eq!(owner.stamp(), baseline);
    assert_eq!(count(&owner, program.project().unwrap(), "root@0/0"), 0.0);
    assert!(program.prepare_hook_action(&owner, "0/0/0").is_ok());
    let foreign = HookDocument::new(1, Budget::default()).unwrap();
    assert!(program.prepare_hook_action(&foreign, "0/0/0").is_err());
}

#[test]
fn actual_owned_slot_census_preserves_same_key_and_retires_replaced_instance_roots() {
    use meari::domain::source_modules::ComponentReference;
    let mut graph = meari::parse_module_graph(&path("root.tsx"), &BTreeMap::from([(path("root.tsx"),
        "import type {ReactNode} from 'react';type Props={body:ReactNode};export function Root(props:Props){return <main>{props.body}</main>;}".into())])).unwrap();
    graph.modules.extend(
        meari::parse_module_graph(
            &path("counter.tsx"),
            &BTreeMap::from([(
                path("counter.tsx"),
                include_str!("../../fixtures/hooks/modules/counter.tsx").into(),
            )]),
        )
        .unwrap()
        .modules,
    );
    let target = ComponentReference {
        module: path("counter.tsx"),
        export: "Counter".into(),
    };
    let mut owner = HookDocument::new(1, Budget::default()).unwrap();
    let prepared = ModuleProgram::prepare_hook_mount_with_node_slots(
        graph,
        "slot-hooks".into(),
        1,
        &owner,
        |factory| {
            Ok(BTreeMap::from([(
                "body".into(),
                factory.component("left", target.clone(), props("before"))?,
            )]))
        },
    )
    .unwrap();
    let mut program = publish(&mut owner, prepared);
    let child = program.project().unwrap().origins["0/0"].instance.clone();
    let original = program.project().unwrap().instance(&child).unwrap().clone();
    let prepared = program.prepare_hook_action(&owner, "0/0/0").unwrap();
    program = publish(&mut owner, prepared);
    let body = program
        .node_slots()
        .component("left", target.clone(), props("same key"))
        .unwrap();
    let prepared = program
        .prepare_hook_props(&owner, BTreeMap::from([("body".into(), body)]))
        .unwrap();
    program = publish(&mut owner, prepared);
    assert_eq!(program.project().unwrap().instance(&child), Some(&original));
    assert_eq!(count(&owner, program.project().unwrap(), &child), 1.0);
    let body = program
        .node_slots()
        .component("right", target, props("new key"))
        .unwrap();
    let prepared = program
        .prepare_hook_props(&owner, BTreeMap::from([("body".into(), body)]))
        .unwrap();
    program = publish(&mut owner, prepared);
    assert!(owner.snapshot(&original).is_err());
    let next_child = &program.project().unwrap().origins["0/0"].instance;
    assert_ne!(
        program.project().unwrap().instance(next_child),
        Some(&original)
    );
    assert_eq!(count(&owner, program.project().unwrap(), next_child), 0.0);
}

#[test]
fn keyed_receiver_reorder_preserves_captured_child_state_with_its_render_parent() {
    use meari::domain::array_value::ArrayValue;
    let source = "import {Wrapper} from './wrapper';import {Counter} from './counter';type Props={rows:string[]};export function Root(props:Props){return <main>{props.rows.map(row=><Wrapper key={row}><Counter caption={row} disabled={false}/></Wrapper>)}</main>;}";
    let graph = meari::parse_module_graph(&path("root.tsx"), &BTreeMap::from([
        (path("root.tsx"), source.into()),
        (path("wrapper.tsx"), "import type {ReactNode} from 'react';type Props={children:ReactNode};export function Wrapper(props:Props){return <article>{props.children}</article>; }".into()),
        (path("counter.tsx"), include_str!("../../fixtures/hooks/modules/counter.tsx").into()),
    ])).unwrap();
    let rows = |items: &[&str]| {
        BTreeMap::from([(
            "rows".into(),
            Value::Array(ArrayValue::strings(
                items.iter().map(|value| (*value).into()).collect(),
            )),
        )])
    };
    let mut owner = HookDocument::new(1, Budget::default()).unwrap();
    let prepared = ModuleProgram::prepare_hook_mount(
        graph,
        rows(&["a", "b"]),
        "captured-hooks".into(),
        1,
        &owner,
    )
    .unwrap();
    let mut program = publish(&mut owner, prepared);
    let child = program.project().unwrap().origins["0/0/0"].instance.clone();
    let instance = program.project().unwrap().instance(&child).unwrap().clone();
    let prepared = program.prepare_hook_action(&owner, "0/0/0/0").unwrap();
    program = publish(&mut owner, prepared);
    let prepared = program
        .prepare_hook_props(&owner, rows(&["b", "a"]))
        .unwrap();
    program = publish(&mut owner, prepared);
    assert_eq!(program.project().unwrap().origins["0/1/0"].instance, child);
    assert_eq!(program.project().unwrap().instance(&child), Some(&instance));
    assert_eq!(count(&owner, program.project().unwrap(), &child), 1.0);
    let prepared = program.prepare_hook_props(&owner, rows(&["b"])).unwrap();
    program = publish(&mut owner, prepared);
    assert!(owner.snapshot(&instance).is_err());
    assert!(
        !program
            .project()
            .unwrap()
            .origins
            .values()
            .any(|origin| origin.instance == child)
    );
}

#[test]
fn unpublished_initial_module_candidates_never_alias_a_later_published_instance() {
    let mut owner = HookDocument::new(1, Budget::default()).unwrap();
    let first =
        ModuleProgram::prepare_hook_mount(graph(), props("first"), "same-input".into(), 1, &owner)
            .unwrap();
    let stale = first
        .program()
        .project()
        .unwrap()
        .instance("root")
        .unwrap()
        .clone();
    let second =
        ModuleProgram::prepare_hook_mount(graph(), props("second"), "same-input".into(), 1, &owner)
            .unwrap();
    assert_ne!(
        second.program().project().unwrap().instance("root"),
        Some(&stale)
    );
    let program = publish(&mut owner, second);
    assert!(owner.snapshot(&stale).is_err());
    assert!(program.prepare_hook_action(&owner, "0/0/0").is_ok());
    assert!(
        first
            .publish_with(&mut owner, |_| Ok::<_, String>(()))
            .is_err()
    );
}
