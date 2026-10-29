//! Explicit integration run: invokes trusted local Rust tools and preserves private evidence.
#[path = "real_apps/assets.rs"]
mod assets;
#[path = "real_apps/modules.rs"]
mod modules;
use meari::{application::builder::BuildFailure, domain::build::*};
use meari_builder::{
    application::{
        build::{BuildRequest, ToolRegistration, execute},
        project::{ProjectFile, ProjectRequest},
    },
    infrastructure::files::hash_file,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn tool(name: &str) -> ToolRegistration {
    let path = PathBuf::from(
        std::env::var_os(name).unwrap_or_else(|| panic!("set {name} to the verified executable")),
    );
    ToolRegistration {
        sha256: hash_file(&path).unwrap(),
        path,
    }
}
fn run(executable: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(executable).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
#[ignore = "actual Native + Web app builds; requires registered tools and a reserved build slot"]
fn real_native_and_web_apps_preserve_last_good_on_invalid_next_source() {
    let framework = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let root = PathBuf::from(
        std::env::var_os("MEARI_APP_PROOF_ROOT").expect("set private MEARI_APP_PROOF_ROOT"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("project");
    std::fs::create_dir(&source).expect("fresh proof root required");
    std::fs::write(
        source.join("counter.tsx"),
        include_str!("../../meari/fixtures/state-counter.tsx"),
    )
    .unwrap();
    std::fs::write(source.join("styles.css"),"main {padding: 16px} button {display: block; width: 160px; height: 40px; background: #223344; color: white} input {width:200px;height:32px} p {color:#112233}").unwrap();
    let config = serde_json::json!({"language":"tsx","title":"Meari actual app proof","native_viewport":{"width":640,"height":480,"dark":false},"props":{},"stylesheet":{"mode":"css","entry":"styles.css","bridge_theme":false}});
    std::fs::write(
        source.join("app.json"),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    let request = || BuildRequest {
        project_root: source.clone(),
        framework_root: framework.clone(),
        work_root: root.join("work"),
        target_dir: PathBuf::from(
            std::env::var_os("MEARI_APP_TARGET_DIR").expect("set isolated MEARI_APP_TARGET_DIR"),
        ),
        store_root: root.join("store"),
        project: ProjectRequest {
            entry: path("counter.tsx"),
            configuration: path("app.json"),
            files: [
                ("counter.tsx", InputRole::Component),
                ("app.json", InputRole::Runtime),
                ("styles.css", InputRole::Stylesheet),
            ]
            .into_iter()
            .map(|(name, role)| {
                (
                    path(name),
                    ProjectFile {
                        role,
                        dependencies: BTreeSet::new(),
                        license: Some("MIT".into()),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        },
        targets: vec![
            BuildTarget {
                kind: TargetKind::Native,
                triple: "x86_64-pc-windows-msvc".into(),
            },
            BuildTarget {
                kind: TargetKind::Web,
                triple: "wasm32-unknown-unknown".into(),
            },
        ],
        cargo: tool("MEARI_CARGO"),
        rustc: tool("MEARI_RUSTC"),
        trunk: Some(tool("MEARI_TRUNK")),
        wasm_bindgen: Some(tool("MEARI_BINDGEN")),
        tailwind: None,
    };
    let publication = execute(request()).unwrap();
    let native = publication
        .artifacts
        .iter()
        .find(|a| a.target.kind == TargetKind::Native)
        .unwrap();
    let executable = root
        .join("store/objects")
        .join(native.identity.as_str())
        .join("files/app.exe");
    let baseline = run(&executable, &["--snapshot"]);
    let changed = run(
        &executable,
        &[
            "--snapshot",
            "--action",
            "0/1",
            "--action",
            "0/1",
            "--action",
            "0/2",
        ],
    );
    assert_eq!(baseline["stamp"]["generation"], publication.generation);
    assert_eq!(
        baseline["state"]["values"]["count"]["value"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        changed["state"]["values"]["count"]["value"].as_f64(),
        Some(2.0)
    );
    assert_eq!(changed["state"]["values"]["label"]["value"], " ready🦀");
    assert_eq!(
        changed["stamp"]["graph_hash"],
        publication.graph_hash.as_str()
    );
    assert_ne!(baseline["state"], changed["state"]);
    assert_ne!(baseline["frame_sha256"], changed["frame_sha256"]);
    assert_eq!(changed["actions"].as_array().unwrap().len(), 3);
    let semantics = baseline["accessibility"].as_array().unwrap();
    assert!(
        semantics
            .iter()
            .any(|node| node["role"] == "Button" && node["label"] == "Increment")
    );
    assert!(semantics.iter().any(|node| node["label"] == "Name"));
    let disabled = run(
        &executable,
        &["--snapshot", "--action", "0/3", "--action", "0/1"],
    );
    assert_eq!(
        disabled["state"]["values"]["count"]["value"].as_f64(),
        Some(0.0)
    );
    assert_eq!(disabled["actions"][1]["changed"], false);
    assert!(
        disabled["accessibility"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["label"] == "Increment" && node["disabled"] == true)
    );
    let current = std::fs::read(root.join("store/current.json")).unwrap();
    std::fs::write(source.join("counter.tsx"), "@@@").unwrap();
    let failure: BuildFailure = execute(request()).unwrap_err();
    assert_eq!(failure.diagnostics[0].source, Some(path("counter.tsx")));
    assert!(failure.diagnostics[0].span.is_some());
    let request_path = root.join("invalid-build-request.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec_pretty(&request()).unwrap(),
    )
    .unwrap();
    let cli = Command::new(env!("CARGO_BIN_EXE_meari-builder"))
        .arg("build")
        .arg(&request_path)
        .output()
        .unwrap();
    assert_eq!(cli.status.code(), Some(1));
    let cli_failure: serde_json::Value = serde_json::from_slice(&cli.stderr).unwrap();
    assert_eq!(cli_failure["diagnostics"][0]["source"], "counter.tsx");
    assert!(!cli_failure["diagnostics"][0]["span"].is_null());
    assert_eq!(
        current,
        std::fs::read(root.join("store/current.json")).unwrap()
    );
    assert_eq!(baseline, run(&executable, &["--snapshot"]));
    std::fs::write(root.join("proof.json"),serde_json::to_vec_pretty(&serde_json::json!({"publication":publication,"baseline":baseline,"changed":changed,"disabled":disabled,"invalid_source":failure,"cli_invalid_source":cli_failure})).unwrap()).unwrap();
}
