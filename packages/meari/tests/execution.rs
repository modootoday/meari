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
    let source = include_str!("../fixtures/card.tsx");
    let output = application::convert(source, Language::Tsx, Language::Rust).unwrap();
    let code = format!(
        "{}\nfn main() {{\n let node = Card(Props {{ label: \"한글\".into(), disabled: true }});\n let meari::domain::NodeKind::Element {{ tag, children, .. }} = node.kind else {{ panic!() }};\n assert_eq!(tag, \"section\");\n let meari::domain::NodeKind::Element {{ attributes, children: texts, .. }} = &children[0].kind else {{ panic!() }};\n assert_eq!(attributes[0].value, meari::domain::Value::Bool(true));\n assert_eq!(texts[0].kind, meari::domain::NodeKind::Text(meari::domain::Value::String(\"한글\".into())));\n let meari::domain::NodeKind::Element {{ tag, attributes, .. }} = &children[1].kind else {{ panic!() }};\n assert_eq!(tag, \"Badge\");\n assert_eq!(attributes[0].value, meari::domain::Value::String(\"한글\".into()));\n}}",
        output.code
    );
    compile_and_run(&code);
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
    let source = include_str!("../fixtures/card.rs");
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
