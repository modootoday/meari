use super::*;

#[test]
#[ignore = "actual resource Native + Web builds require registered tools and a reserved slot"]
fn real_asset_apps_ship_original_bytes_and_native_reports_delivery() {
    let framework = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let root = PathBuf::from(
        std::env::var_os("MEARI_ASSET_PROOF_ROOT")
            .expect("fresh private MEARI_ASSET_PROOF_ROOT required"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("project");
    std::fs::create_dir(&source).expect("fresh asset proof root required");
    std::fs::create_dir(source.join("styles")).unwrap();
    std::fs::create_dir(source.join("icons")).unwrap();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#ff0000" d="M0 0h16v16H0z"/></svg>"##;
    let image = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#0000ff" d="M0 0h16v16H0z"/></svg>"##;
    let css = "main{width:120px;height:100px;background-image:url('../icons/logo.svg');background-size:16px 16px}";
    std::fs::write(source.join("icons/logo.svg"), svg).unwrap();
    std::fs::write(source.join("icons/image.svg"), image).unwrap();
    std::fs::write(source.join("styles/app.css"), css).unwrap();
    std::fs::write(source.join("view.tsx"), "type Props={};export function View(props:Props){return <main aria-label=\"Asset proof\"><img src=\"./icons/image.svg\" alt=\"Blue source asset\" width=\"16\" height=\"16\"/></main>;}").unwrap();
    std::fs::write(source.join("app.json"), serde_json::to_vec(&serde_json::json!({"language":"tsx","title":"Asset proof","native_viewport":{"width":160,"height":120,"dark":false},"props":{},"stylesheet":{"mode":"css","entry":"styles/app.css","bridge_theme":false}})).unwrap()).unwrap();
    let request = || BuildRequest {
        project_root: source.clone(),
        framework_root: framework.clone(),
        work_root: root.join("work"),
        target_dir: PathBuf::from(
            std::env::var_os("MEARI_APP_TARGET_DIR").expect("cached MEARI_APP_TARGET_DIR required"),
        ),
        store_root: root.join("store"),
        project: ProjectRequest {
            entry: path("view.tsx"),
            configuration: path("app.json"),
            files: [
                ("view.tsx", InputRole::Component),
                ("app.json", InputRole::Runtime),
                ("styles/app.css", InputRole::Stylesheet),
                ("icons/logo.svg", InputRole::Icon),
                ("icons/image.svg", InputRole::Image),
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
    let resource = format!("assets/{}.svg", ContentHash::of(svg).as_str());
    let mut snapshot = None;
    for artifact in &publication.artifacts {
        let files = root
            .join("store/objects")
            .join(artifact.identity.as_str())
            .join("files");
        let deployed = if artifact.target.kind == TargetKind::Web {
            format!("web/{resource}")
        } else {
            resource.clone()
        };
        assert_eq!(std::fs::read(files.join(deployed)).unwrap(), svg);
        let image_resource = format!("assets/{}.svg", ContentHash::of(image).as_str());
        let image_deployed = if artifact.target.kind == TargetKind::Web {
            format!("web/{image_resource}")
        } else {
            image_resource.clone()
        };
        assert_eq!(std::fs::read(files.join(image_deployed)).unwrap(), image);
        assert_eq!(
            std::fs::read(files.join("source/original/styles/app.css")).unwrap(),
            css.as_bytes()
        );
        if artifact.target.kind == TargetKind::Native {
            let generated = std::fs::read_to_string(files.join("Cargo.toml")).unwrap();
            // meari-native reaches the vendored painter by path; no [patch] is needed.
            assert!(!generated.contains("[patch"));
            let lock = std::fs::read_to_string(files.join("build/Cargo.lock")).unwrap();
            let paint = lock
                .split("[[package]]")
                .filter(|block| block.lines().any(|line| line == "name = \"meari-paint\""))
                .collect::<Vec<_>>();
            assert_eq!(
                paint.len(),
                1,
                "generated Native must have one paint source"
            );
            assert!(
                !paint[0].contains("source ="),
                "generated Native fell back to a registry painter"
            );
            let frame = run(&files.join("app.exe"), &["--snapshot"]);
            assert_eq!(frame["assets"]["rejected"], 0);
            assert!(
                frame["assets"]["delivered"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .any(|key| key.ends_with(&resource))
            );
            assert_eq!(frame["rgba_bytes"], 160 * 120 * 4);
            use meari_native::{
                application::Renderer, infrastructure::document::InteractiveDocument,
            };
            let definition = serde_json::from_slice(
                &std::fs::read(files.join("runtime/application.json")).unwrap(),
            )
            .unwrap();
            let manifest = serde_json::from_slice(
                &std::fs::read(files.join("runtime/asset-manifest.json")).unwrap(),
            )
            .unwrap();
            let assets = meari::application::assets::VerifiedAssets::new(
                manifest,
                BTreeMap::from([
                    (ContentHash::of(svg), svg.to_vec()),
                    (ContentHash::of(image), image.to_vec()),
                ]),
            )
            .unwrap();
            let css = std::fs::read_to_string(files.join("styles.css")).unwrap();
            let mut document =
                InteractiveDocument::from_application_with_assets(&definition, &css, assets)
                    .unwrap();
            document.renderer.layout(&document.viewport, 0.0).unwrap();
            let pixels = document.renderer.rasterize(&document.viewport).unwrap();
            std::fs::write(root.join("native-frame.rgba"), &pixels).unwrap();
            std::fs::write(
                root.join("native-snapshot.json"),
                serde_json::to_vec_pretty(&frame).unwrap(),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(ContentHash::of(&pixels)).unwrap(),
                frame["frame_sha256"]
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|rgba| rgba[0] > 180 && rgba[1] < 90 && rgba[2] < 90)
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|rgba| rgba[0] < 90 && rgba[1] < 90 && rgba[2] > 180)
            );
            snapshot = Some(frame);
        }
    }
    assert!(snapshot.is_some());
    std::fs::write(root.join("proof.json"), serde_json::to_vec_pretty(&serde_json::json!({"publication":publication,"native":snapshot,"actual_browser":false,"visual_acceptance":false,"font_shaping":false})).unwrap()).unwrap();
}
