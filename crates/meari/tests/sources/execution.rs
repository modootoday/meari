use meari::{self as application, application::Language};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "meari-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generated_rust_compiles_and_evaluates_props_without_moving_repeated_strings() {
    let source = include_str!("../../fixtures/card.tsx");
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    let code = format!(
        "{}\nfn main() {{\n let node = Card(Props {{ label: \"한글\".into(), disabled: true }});\n let meari::domain::NodeKind::Element {{ tag, children, .. }} = node.kind else {{ panic!() }};\n assert_eq!(tag, \"section\");\n let meari::domain::NodeKind::Element {{ attributes, children: texts, .. }} = &children[0].kind else {{ panic!() }};\n assert_eq!(attributes[0].value, meari::domain::Value::Bool(true));\n assert_eq!(texts[0].kind, meari::domain::NodeKind::Text(meari::domain::Value::String(\"한글\".into())));\n let meari::domain::NodeKind::Element {{ tag, attributes, .. }} = &children[1].kind else {{ panic!() }};\n assert_eq!(tag, \"Badge\");\n assert_eq!(attributes[0].value, meari::domain::Value::String(\"한글\".into()));\n}}",
        output.code
    );
    compile_and_run(&code);
}

#[test]
fn generated_boolean_predicates_execute_with_the_same_guard_truth_table() {
    let source = "type Props={canSave:boolean;busy:boolean;refreshRequired:boolean};export function Flags(props:Props){return <button disabled={!props.canSave || props.busy || props.refreshRequired}/>;}";
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        r#"{}
fn main() {{
    for can_save in [false,true] {{ for busy in [false,true] {{ for refresh in [false,true] {{
        let node = Flags(Props {{ canSave:can_save,busy,refreshRequired:refresh }});
        let meari::domain::NodeKind::Element {{ attributes,.. }} = node.kind else {{ panic!() }};
        assert_eq!(attributes[0].value,meari::domain::Value::Bool(!can_save || busy || refresh));
    }} }} }}
}}
"#,
        output.code
    ));
}

#[test]
fn generated_repeat_rust_keeps_the_typed_source_and_lexical_template() {
    let source = "type Props={refs:string[]};export function List(props:Props){return <ul>{props.refs.map(reference => <li key={reference}>{reference}</li>)}</ul>;}";
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        r#"{}
fn main() {{
    let node=List(Props{{refs:vec!["cue-a".into(),"원문🦀".into()]}});
    let meari::domain::NodeKind::Element{{children,..}}=node.kind else{{panic!()}};
    let meari::domain::NodeKind::Text(meari::domain::Value::Repeat(repeat))=&children[0].kind else{{panic!()}};
    assert_eq!(repeat.item,"reference");
    let meari::domain::Value::Array(source)=&repeat.source else{{panic!()}};
    assert_eq!(source.items.len(),2);
    assert_eq!(source.items[1],meari::domain::Value::String("원문🦀".into()));
}}
"#,
        output.code
    ));
}

#[test]
fn generated_shadcn_rust_preserves_recipe_inputs_at_runtime() {
    let source = "type Props = { label: string; disabled: boolean; }; export function Showcase(props: Props) { return <Button className=\"px-4 md:px-6\" variant=\"outline\" disabled={props.disabled}>{props.label}</Button>; }";
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    assert!(output.design_system.native_syntax_supported);
    let code = format!(
        "{}\nfn main() {{ let node = Showcase(Props {{ label: \"저장\".into(), disabled: true }}); let meari::domain::NodeKind::Element {{ tag, attributes, children }} = node.kind else {{ panic!() }}; assert_eq!(tag, \"Button\"); assert_eq!(attributes[0].value, meari::domain::Value::String(\"px-4 md:px-6\".into())); assert_eq!(attributes[1].value, meari::domain::Value::String(\"outline\".into())); assert_eq!(attributes[2].value, meari::domain::Value::Bool(true)); assert_eq!(children.len(), 1); }}",
        output.code
    );
    compile_and_run(&code);
}

#[test]
fn generated_use_id_rust_compiles_and_shares_label_identity() {
    let source = "import {useId} from 'react';type Props={label:string};export function Demo(props:Props){const id=useId();return <div><label htmlFor={id}>{props.label}</label><input id={id}/></div>;}";
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        "{}\nfn main() {{let node=Demo(Props{{label:\"Name\".into()}});let meari::domain::NodeKind::Element{{children,..}}=node.kind else{{panic!()}};let meari::domain::NodeKind::Element{{attributes:a,..}}=&children[0].kind else{{panic!()}};let meari::domain::NodeKind::Element{{attributes:b,..}}=&children[1].kind else{{panic!()}};assert_eq!(a[0].value,b[0].value);assert_eq!(a[0].value,meari::domain::Value::String(\"bridge-hook-id\".into()));}}",
        output.code
    ));
}

#[test]
fn generated_stateful_rust_executes_with_an_explicit_instance_snapshot() {
    let source = include_str!("../../fixtures/state-counter.tsx");
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        r#"{}
fn main() {{
    let source = {:?};
    let component = meari::parse(source, meari::application::Language::Tsx).unwrap();
    let program = meari::application::state_program::StateProgram::mount(component,"generated-instance".into(),1).unwrap();
    let state = StateContext {{ snapshot: program.snapshot().unwrap() }};
    let node = Counter(Props {{}}, &state).unwrap();
    let meari::domain::NodeKind::Element {{children,..}} = node.kind else {{panic!()}};
    let meari::domain::NodeKind::Element {{children: status,..}} = &children[0].kind else {{panic!()}};
    assert_eq!(status[1].kind,meari::domain::NodeKind::Text(meari::domain::Value::String("0".into())));
    let meari::domain::NodeKind::Element {{attributes,..}} = &children[1].kind else {{panic!()}};
    assert!(attributes.iter().any(|attribute| matches!(attribute.value,meari::domain::Value::Event(_))));
}}
"#,
        output.code, source
    ));
}

#[test]
fn generated_expression_handlers_capture_values_but_keep_previous_dynamic() {
    let source = include_str!("../../fixtures/state-capture.tsx");
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        r#"{}
fn main() {{
    let definition=meari::parse({:?},meari::application::Language::Tsx).unwrap();
    let mut program=meari::application::state_program::StateProgram::mount(definition,"generated-capture".into(),1).unwrap();
    let state=StateContext{{snapshot:program.snapshot().unwrap()}};
    let node=CaptureDemo(Props{{}},&state).unwrap();
    let meari::domain::NodeKind::Element{{children,..}}=node.kind else{{panic!()}};
    let event=|index:usize|{{let meari::domain::NodeKind::Element{{attributes,..}}=&children[index].kind else{{panic!()}};let meari::domain::Value::Event(event)=&attributes.iter().find(|attribute|attribute.name=="onClick").unwrap().value else{{panic!()}};event.clone()}};
    let capture=event(2);let updater=event(3);let step=event(4);
    program.dispatch(&capture).unwrap();program.dispatch(&capture).unwrap();
    program.dispatch(&step).unwrap();program.dispatch(&updater).unwrap();
    let meari::domain::component_state::StateValue::Number(count)=&program.snapshot().unwrap().values["count"] else{{panic!()}};
    assert_eq!(count.as_f64(),Some(4.0));
}}
"#,
        output.code, source
    ));
}

#[test]
fn generated_mixed_hooks_compile_and_keep_label_identity_after_state_dispatch() {
    let source = include_str!("../../fixtures/mixed-hooks.tsx");
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    compile_and_run(&format!(
        r#"{}
fn main() {{
 let definition=meari::parse({:?},meari::application::Language::Tsx).unwrap();
 let mut program=meari::application::state_program::StateProgram::mount(definition,"mixed-generated".into(),1).unwrap();
 let node=MixedHooks(Props{{}},&StateContext{{snapshot:program.snapshot().unwrap()}}).unwrap();
 let meari::domain::NodeKind::Element{{children,..}}=node.kind else{{panic!()}};
 let meari::domain::NodeKind::Element{{attributes:label,..}}=&children[0].kind else{{panic!()}};
 let meari::domain::NodeKind::Element{{attributes:input,..}}=&children[1].kind else{{panic!()}};
 assert_eq!(label[0].value,input[0].value);
 let event=program.project().unwrap().events["0/4"].clone(); program.dispatch(&event).unwrap();
 let node=MixedHooks(Props{{}},&StateContext{{snapshot:program.snapshot().unwrap()}}).unwrap();
 let meari::domain::NodeKind::Element{{children:updated,..}}=node.kind else{{panic!()}};
 assert_eq!(children[1],updated[1]);
}}
"#,
        output.code, source
    ));
}

#[test]
fn generated_callback_rust_compiles_and_preserves_references_text_ids_and_negated_guard() {
    let mut convert = application::convert(
        include_str!("../../fixtures/host-callback-convert.tsx"),
        Language::Tsx,
        Language::Rust,
    )
    .unwrap()
    .code;
    convert.push_str(r#"
fn main() {
    let node = SubtitleConvertIntent(Props { canConvert: true, onConvert: ActionCallback::new("convert_file").unwrap(), resultLabel: "ready".into() });
    let meari::domain::NodeKind::Element { children, .. } = node.kind else { panic!() };
    let meari::domain::NodeKind::Element { attributes, .. } = &children[0].kind else { panic!() };
    assert_eq!(attributes.iter().find(|attribute| attribute.name == "disabled").unwrap().value, meari::domain::Value::Bool(false));
    let meari::domain::Value::BoundCallback(callback) = &attributes.iter().find(|attribute| attribute.name == "onClick").unwrap().value else { panic!() };
    assert_eq!(callback.invoke(None).unwrap().prop, "convert_file");
}
"#);
    compile_and_run(&convert);
    let mut review = application::convert(
        include_str!("../../fixtures/host-callback-review.tsx"),
        Language::Tsx,
        Language::Rust,
    )
    .unwrap()
    .code;
    review.push_str(r#"
fn main() {
    let node = ProposalReviewIntent(Props { proposalId: "p".into(), sourceSha256: "sha".into(), documentRevision: "1".into(), workingText: "draft".into(), errorText: "".into(), inputDisabled: false, saveDisabled: false, acceptDisabled: true, refreshDisabled: false, onDraftChange: TextCallback::new("draft_change").unwrap(), onSave: ActionCallback::new("save").unwrap(), onAccept: ActionCallback::new("accept").unwrap(), onRefresh: ActionCallback::new("refresh").unwrap() });
    let meari::domain::NodeKind::Element { children, .. } = node.kind else { panic!() };
    let meari::domain::NodeKind::Element { attributes: label, .. } = &children[1].kind else { panic!() };
    let meari::domain::NodeKind::Element { attributes: input, .. } = &children[2].kind else { panic!() };
    assert_eq!(label.iter().find(|attribute| attribute.name == "htmlFor").unwrap().value, input.iter().find(|attribute| attribute.name == "id").unwrap().value);
    let meari::domain::Value::BoundCallback(callback) = &input.iter().find(|attribute| attribute.name == "onChange").unwrap().value else { panic!() };
    assert_eq!(callback.invoke(Some("edited")).unwrap().arguments, meari::domain::host_callbacks::CallbackArguments::Text("edited".into()));
    assert_eq!(callback.reference.binding, "draft_change");
}
"#);
    compile_and_run(&review);
}

#[test]
fn generated_navigation_capture_rust_compiles_and_activates_accepted_string_prop() {
    let mut generated = application::convert(
        include_str!("../../fixtures/node-slots/studio/navigation-item.tsx"),
        Language::Tsx,
        Language::Rust,
    )
    .unwrap()
    .code;
    assert!(generated.contains("host_text_capture(&props.onNavigate, &props.workspaceId)"));
    generated.push_str(r#"
fn main() {
    for workspace in ["", "작업공간/🌊"] {
        let node = StudioNavigationItem(Props {
            workspaceId: workspace.into(), label: "작업 열기".into(), iconClass: "icon".into(),
            selected: true, onNavigate: TextCallback::new("navigate_workspace").unwrap(),
        });
        let meari::domain::NodeKind::Element { attributes, .. } = node.kind else { panic!() };
        let meari::domain::Value::BoundCallback(callback) = &attributes.iter().find(|attribute| attribute.name == "onClick").unwrap().value else { panic!() };
        assert_eq!(callback.source, meari::domain::host_callbacks::CallbackSource::CapturedText);
        assert_eq!(callback.reference.binding, "navigate_workspace");
        assert_eq!(callback.reference.signature, meari::domain::host_callbacks::CallbackSignature::Text);
        assert_eq!(callback.captured_text.as_deref(), Some(workspace));
        assert_eq!(callback.invoke(None).unwrap().arguments, meari::domain::host_callbacks::CallbackArguments::Text(workspace.into()));
        assert!(callback.invoke(Some("injected")).is_err());
    }
}
"#);
    compile_and_run(&generated);
}

#[test]
fn generated_navigation_predicates_compile_without_moving_repeated_string_props() {
    let entry =
        meari::domain::build::BuildPath::try_from("studio/navigation.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &entry,
        &std::collections::BTreeMap::from([
            (
                entry.clone(),
                include_str!("../../fixtures/node-slots/studio/navigation.tsx").into(),
            ),
            (
                meari::domain::build::BuildPath::try_from("studio/navigation-item.tsx".to_owned())
                    .unwrap(),
                include_str!("../../fixtures/node-slots/studio/navigation-item.tsx").into(),
            ),
        ]),
    )
    .unwrap();
    let mut generated = meari::emit(&graph.modules[&entry].component, Language::Rust)
        .unwrap()
        .code;
    generated.push_str(r#"
fn visit(node: &meari::domain::Node, selected: &str) -> usize {
    let meari::domain::NodeKind::Element { tag, attributes, children } = &node.kind else { return 0 };
    let count = if tag == "StudioNavigationItem" {
        let meari::domain::Value::String(workspace) = &attributes.iter().find(|attribute| attribute.name == "workspaceId").unwrap().value else { panic!() };
        assert_eq!(attributes.iter().find(|attribute| attribute.name == "selected").unwrap().value, meari::domain::Value::Bool(workspace == selected));
        1
    } else { 0 };
    count + children.iter().map(|child| visit(child, selected)).sum::<usize>()
}
fn main() {
    for selected in ["SEG", "SUB", "NONE"] {
        let node = StudioNavigation(Props { selectedId: selected.into(), onNavigate: TextCallback::new("navigate").unwrap() });
        assert_eq!(visit(&node, selected), 13);
    }
}
"#);
    compile_and_run(&generated);
}

#[test]
fn generated_original_queue_footer_reads_live_boolean_state_predicates() {
    let source = include_str!("../../fixtures/node-slots/studio/queue-footer.tsx");
    let generated = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    assert_eq!(generated.schema_version, 14);
    compile_and_run(&format!(
        r#"{}
fn main() {{
 let definition=meari::parse({:?},meari::application::Language::Tsx).unwrap();
 let definition=meari::application::binding::bind(&definition,&std::collections::BTreeMap::from([("summaryLabel".into(),meari::domain::Value::String("Summary".into())),("detailsLabel".into(),meari::domain::Value::String("Details".into()))])).unwrap();
 let mut program=meari::application::state_program::StateProgram::mount(definition,"generated-queue-predicate".into(),1).unwrap();
 for hidden in [true,false,true] {{
  let node=QueueFooter(Props{{summaryLabel:"Summary".into(),detailsLabel:"Details".into()}},&StateContext{{snapshot:program.snapshot().unwrap()}}).unwrap();
  let meari::domain::NodeKind::Element{{children,..}}=node.kind else{{panic!()}};
  let meari::domain::NodeKind::Element{{attributes,..}}=&children[1].kind else{{panic!()}};
  assert_eq!(attributes.iter().find(|a|a.name=="hidden").unwrap().value,meari::domain::Value::Bool(hidden));
  let event=program.project().unwrap().events["0/0/0"].clone();
  program.dispatch(&event).unwrap();
 }}
}}
"#,
        generated.code, source
    ));
}

fn compile_and_run(code: &str) {
    let scratch = Scratch::new();
    let input = scratch.0.join("generated.rs");
    fs::write(&input, code).unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let library = fs::read_dir(&deps)
        .unwrap()
        .map(Result::unwrap)
        .map(|e| e.path())
        .filter(|p| {
            (p.file_name().unwrap() == "libmeari.rlib"
                || p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("libmeari-"))
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .max_by_key(|p| fs::metadata(p).unwrap().modified().unwrap())
        .expect("Cargo-built library artifact");
    let executable = scratch
        .0
        .join(format!("generated{}", std::env::consts::EXE_SUFFIX));
    let compiled = Command::new("rustc")
        .args([
            "--edition=2024",
            "-Dwarnings",
            "--crate-name",
            "bridge_generated",
        ])
        .arg(&input)
        .arg("--extern")
        .arg(format!("meari={}", library.display()))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let run = Command::new(executable).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
}

fn cli(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_meari-cli"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cli_native_plan_uses_shared_theme_binding_and_error_contract() {
    use meari::domain::design::utilities::COLORS;
    let colors: serde_json::Map<String, serde_json::Value> = COLORS
        .iter()
        .map(|name| ((*name).into(), serde_json::json!([10, 20, 30, 255])))
        .collect();
    let mut request = serde_json::json!({
        "source": "type Props = { label: string; disabled: boolean; }; export function Demo(props: Props) { return <Button disabled={props.disabled}>{props.label}</Button>; }",
        "language": "tsx", "props": {"label": {"String":"실행"}, "disabled": {"Bool":true}},
        "theme": {"light":colors, "dark":colors},
        "environment": {"viewport_width":800.0, "root_font_px":16.0, "dark":false}
    });
    let output = cli(&["native-plan"], &request.to_string());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["profile"], "tailwind-shadcn-v1");
    assert_eq!(plan["root"]["children"][0]["text"], "실행");
    assert_eq!(plan["root"]["style"]["Height"]["Px"], 36.0);
    request["props"]["disabled"] = serde_json::json!({"String":"wrong"});
    let failed = cli(&["native-plan"], &request.to_string());
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "native-binding");
}

#[test]
fn cli_converts_real_sources_in_both_directions_and_reports_errors() {
    let source = include_str!("../../fixtures/card.rs");
    let tsx = cli(&["rust-to-tsx", "--json"], source);
    assert!(tsx.status.success());
    assert!(tsx.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&tsx.stdout).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["design_system"]["profile"], "tailwind-shadcn-v1");
    assert_eq!(json["mappings"].as_array().unwrap().len(), 4);
    let rust = cli(&["tsx-to-rust"], json["code"].as_str().unwrap());
    assert!(rust.status.success());
    let ir =
        application::parse(std::str::from_utf8(&rust.stdout).unwrap(), Language::Rust).unwrap();
    assert!(ir.semantic_eq(&application::parse(source, Language::Rust).unwrap()));
    for (args, source, expected) in [
        (&["tsx-to-rust"][..], "@@@", "syntax"),
        (&["unknown"][..], "", "input"),
    ] {
        let error = cli(args, source);
        assert!(!error.status.success());
        assert!(error.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&error.stderr).unwrap()["code"],
            expected
        );
    }
}
