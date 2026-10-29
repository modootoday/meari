use super::*;

#[test]
#[ignore = "actual graph Native + Web builds; requires registered tools and a reserved build slot"]
fn real_module_apps_render_independent_children_and_keep_last_good() {
    let framework = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let root = PathBuf::from(
        std::env::var_os("MEARI_GRAPH_PROOF_ROOT")
            .expect("set fresh private MEARI_GRAPH_PROOF_ROOT"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("project");
    std::fs::create_dir(&source).expect("fresh proof root required");
    std::fs::write(source.join("view.tsx"), "import {Counter} from './counter';type Props={caption:string};export function View(props:Props){return <main><h1>{props.caption}</h1><Counter/><Counter/></main>;}").unwrap();
    std::fs::write(source.join("counter.tsx"), "import {useState} from 'react';type Props={};export function Counter(props:Props){const [count,setCount]=useState(0);return <section><button onClick={()=>setCount(previous=>previous+1)}>Increment</button><p>{count}</p></section>;}").unwrap();
    std::fs::write(source.join("styles.css"), "main{padding:16px} section{height:100px} button{display:block;width:140px;height:36px;background:#123456;color:white}").unwrap();
    std::fs::write(source.join("app.json"), serde_json::to_vec(&serde_json::json!({"language":"tsx","title":"Meari graph proof","native_viewport":{"width":640,"height":480,"dark":false},"props":{"caption":{"String":"Independent children"}},"stylesheet":{"mode":"css","entry":"styles.css","bridge_theme":false}})).unwrap()).unwrap();
    let request = || BuildRequest {
        project_root: source.clone(),
        framework_root: framework.clone(),
        work_root: root.join("work"),
        target_dir: PathBuf::from(
            std::env::var_os("MEARI_APP_TARGET_DIR").expect("set cached MEARI_APP_TARGET_DIR"),
        ),
        store_root: root.join("store"),
        project: ProjectRequest {
            entry: path("view.tsx"),
            configuration: path("app.json"),
            files: [
                ("view.tsx", InputRole::Component),
                ("counter.tsx", InputRole::Component),
                ("styles.css", InputRole::Stylesheet),
                ("app.json", InputRole::Runtime),
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
            .collect(),
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
        .find(|artifact| artifact.target.kind == TargetKind::Native)
        .unwrap();
    let files = root
        .join("store/objects")
        .join(native.identity.as_str())
        .join("files");
    let executable = files.join("app.exe");
    let baseline = run(&executable, &["--snapshot"]);
    let changed = run(
        &executable,
        &[
            "--snapshot",
            "--action",
            "0/1/0",
            "--action",
            "0/1/0",
            "--action",
            "0/2/0",
        ],
    );
    assert!(baseline["state"].is_null());
    assert_eq!(baseline["module_states"].as_object().unwrap().len(), 3);
    assert_eq!(
        baseline["module_states"]["root@0/1"]["values"]["count"]["value"].as_f64(),
        Some(0.0)
    );
    assert_eq!(
        changed["module_states"]["root@0/1"]["values"]["count"]["value"].as_f64(),
        Some(2.0)
    );
    assert_eq!(
        changed["module_states"]["root@0/2"]["values"]["count"]["value"].as_f64(),
        Some(1.0)
    );
    assert_ne!(baseline["frame_sha256"], changed["frame_sha256"]);
    assert_eq!(baseline["rgba_bytes"], 640 * 480 * 4);
    assert_eq!(
        baseline["accessibility"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["role"] == "Button" && node["label"] == "Increment")
            .count(),
        2
    );
    assert_eq!(
        std::fs::read(files.join("source/original/counter.tsx")).unwrap(),
        std::fs::read(source.join("counter.tsx")).unwrap()
    );
    let current = std::fs::read(root.join("store/current.json")).unwrap();
    std::fs::write(source.join("counter.tsx"), "@@@").unwrap();
    let failure = execute(request()).unwrap_err();
    assert_eq!(failure.diagnostics[0].source, Some(path("counter.tsx")));
    assert!(failure.diagnostics[0].span.is_some());
    assert_eq!(
        current,
        std::fs::read(root.join("store/current.json")).unwrap()
    );
    assert_eq!(baseline, run(&executable, &["--snapshot"]));
    std::fs::write(root.join("proof.json"),serde_json::to_vec_pretty(&serde_json::json!({"publication":publication,"baseline":baseline,"changed":changed,"invalid_child":failure,"actual_browser":false,"visual_acceptance":false})).unwrap()).unwrap();
}
