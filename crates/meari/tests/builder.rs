#![cfg(not(target_arch = "wasm32"))]
use meari::{
    application::{Language, builder::*},
    domain::build::*,
};
use std::collections::{BTreeMap, BTreeSet};
#[path = "builder/runtime.rs"]
mod runtime;
#[path = "builder/store.rs"]
mod store;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn source() -> &'static str {
    "type Props={};export function Demo(props:Props){return <p>한글</p>;}"
}
fn graph() -> BuildGraph {
    BuildGraph {
        entry: path("src/view.tsx"),
        files: BTreeMap::from([(
            path("src/view.tsx"),
            InputFile {
                role: InputRole::Component,
                content: ContentHash::of(source().as_bytes()),
                length: source().len() as u64,
                dependencies: BTreeSet::new(),
                license: Some("MIT".into()),
            },
        )]),
    }
}
fn target(kind: TargetKind) -> BuildTarget {
    BuildTarget {
        kind,
        triple: match kind {
            TargetKind::Native => "x86_64-pc-windows-msvc",
            TargetKind::Web => "wasm32-unknown-unknown",
        }
        .into(),
    }
}
fn spec() -> TargetBuildSpec {
    TargetBuildSpec {
        available_capabilities: BTreeSet::from(["dom".into()]),
        toolchain: BuildToolchain {
            rustc_release: "fixture-rust".into(),
            rustc_commit: "fixture-commit".into(),
            cargo_release: "fixture-cargo".into(),
            providers: BTreeMap::from([(
                "fixture-backend".into(),
                ProviderPin {
                    version: "1".into(),
                    revision: "fixed".into(),
                    content: ContentHash::of(b"fixture backend"),
                },
            )]),
            settings: BTreeMap::from([("profile".into(), "debug".into())]),
        },
    }
}
fn targets() -> BTreeMap<BuildTarget, TargetBuildSpec> {
    BTreeMap::from([
        (target(TargetKind::Native), spec()),
        (target(TargetKind::Web), spec()),
    ])
}
struct Inputs;
impl BuildInputs for Inputs {
    fn read(&self, _: &BuildPath) -> Result<Vec<u8>, String> {
        Ok(source().as_bytes().to_vec())
    }
}
// Binary payloads are test fixtures, not completed Native/Web compiler adapters.
struct FixtureCompiler;
impl TargetCompiler for FixtureCompiler {
    fn compile(
        &self,
        task: &BuildTask,
        inputs: &BTreeMap<BuildPath, Vec<u8>>,
    ) -> Result<BuildBundle, BuildFailure> {
        let output = meari::convert(
            std::str::from_utf8(&inputs[&task.graph.entry]).map_err(|error| error.to_string())?,
            Language::Tsx,
            Language::Rust,
        )
        .map_err(|error| BuildFailure::source(task.graph.entry.clone(), error))?;
        let generated = path("source/entry.rs");
        let mut files = BTreeMap::from([(generated.clone(), output.code.into_bytes())]);
        let mut roles = BTreeMap::from([(generated.clone(), OutputRole::GeneratedSource)]);
        match task.target.kind {
            TargetKind::Native => {
                files.insert(path("host.bin"), b"fixture library payload".to_vec());
                roles.insert(path("host.bin"), OutputRole::NativeLibrary);
            }
            TargetKind::Web => {
                files.insert(path("app.wasm"), b"\0asm\x01\0\0\0".to_vec());
                roles.insert(path("app.wasm"), OutputRole::WasmModule);
                files.insert(path("app.module"), b"opaque fixture module".to_vec());
                roles.insert(path("app.module"), OutputRole::WebModule);
            }
        }
        let outputs = files
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    OutputFile {
                        role: roles[path],
                        content: ContentHash::of(bytes),
                        length: bytes.len() as u64,
                    },
                )
            })
            .collect();
        let source_maps = output
            .mappings
            .into_iter()
            .map(|mapping| BuildMapping {
                source: task.graph.entry.clone(),
                generated: generated.clone(),
                source_span: mapping.source,
                generated_span: mapping.generated,
            })
            .collect();
        Ok(BuildBundle {
            manifest: BuildManifest {
                schema_version: 1,
                session: task.ticket.session.clone(),
                generation: task.ticket.generation,
                graph_hash: task.ticket.graph_hash.clone(),
                graph: task.graph.clone(),
                target: task.target.clone(),
                toolchain: task.toolchain.clone(),
                outputs,
                source_maps,
                available_capabilities: task.available_capabilities.clone(),
                required_capabilities: BTreeSet::from(["dom".into()]),
                diagnostics: Vec::new(),
            },
            files,
        })
    }
}
fn bundle(builder: &BuildCoordinator, ticket: &BuildTicket, target: &BuildTarget) -> BuildBundle {
    compile_task(
        &builder.task(ticket, target).unwrap(),
        &FixtureCompiler,
        &Inputs,
    )
    .unwrap()
}

#[derive(Default)]
struct MemoryStore {
    bundles: BTreeMap<ContentHash, BuildManifest>,
    publication: Option<BuildPublication>,
    fail_stage: bool,
    fail_publish: bool,
}
impl ArtifactStore for MemoryStore {
    fn stage(&mut self, bundle: &BuildBundle) -> Result<ArtifactRef, String> {
        if self.fail_stage {
            return Err("injected stage failure".into());
        }
        bundle.validate()?;
        let identity = bundle.manifest.identity()?;
        self.bundles
            .insert(identity.clone(), bundle.manifest.clone());
        Ok(ArtifactRef {
            identity,
            target: bundle.manifest.target.clone(),
        })
    }
    fn publish(&mut self, publication: &BuildPublication) -> Result<(), String> {
        if self.fail_publish {
            return Err("injected publication failure".into());
        }
        self.publication = Some(publication.clone());
        Ok(())
    }
    fn current(&self) -> Result<Option<BuildPublication>, String> {
        Ok(self.publication.clone())
    }
    fn manifest(&self, artifact: &ArtifactRef) -> Result<BuildManifest, String> {
        self.bundles
            .get(&artifact.identity)
            .cloned()
            .ok_or("missing fixture artifact".into())
    }
}

#[test]
fn graph_invalidation_covers_diamond_changes_deletion_and_cycles() {
    let mut previous = graph();
    let entry = previous.entry.clone();
    for name in ["left.tsx", "right.tsx", "tokens.css", "unused.svg"] {
        previous.files.insert(
            path(name),
            InputFile {
                role: InputRole::Stylesheet,
                content: ContentHash::of(name.as_bytes()),
                length: name.len() as u64,
                dependencies: BTreeSet::new(),
                license: None,
            },
        );
    }
    previous.files.get_mut(&entry).unwrap().dependencies =
        BTreeSet::from([path("left.tsx"), path("right.tsx")]);
    for name in ["left.tsx", "right.tsx"] {
        previous
            .files
            .get_mut(&path(name))
            .unwrap()
            .dependencies
            .insert(path("tokens.css"));
    }
    let mut next = previous.clone();
    next.files.get_mut(&path("tokens.css")).unwrap().content = ContentHash::of(b"changed");
    let delta = next.changes_from(&previous).unwrap();
    assert_eq!(delta.changed, BTreeSet::from([path("tokens.css")]));
    assert_eq!(delta.invalidated.len(), 4);
    assert!(!delta.invalidated.contains(&path("unused.svg")));
    next.files.remove(&path("right.tsx"));
    next.files
        .get_mut(&entry)
        .unwrap()
        .dependencies
        .remove(&path("right.tsx"));
    assert!(
        next.changes_from(&previous)
            .unwrap()
            .invalidated
            .contains(&path("right.tsx"))
    );
    next.files
        .get_mut(&path("tokens.css"))
        .unwrap()
        .dependencies
        .insert(entry);
    assert!(next.validate().is_err());
    let reordered = BuildGraph {
        entry: previous.entry.clone(),
        files: previous
            .files
            .iter()
            .rev()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    };
    assert_eq!(
        previous.fingerprint().unwrap(),
        reordered.fingerprint().unwrap()
    );
}

#[test]
fn manifests_paths_capabilities_and_source_maps_fail_closed() {
    assert_eq!(
        ContentHash::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    for invalid in [
        "../escape",
        "/absolute",
        "C:/escape",
        "a\\b",
        "a//b",
        "AUX.txt",
        "a/con/file",
        "a/trailing.",
        "a/trailing ",
        "COM9.exe",
    ] {
        assert!(
            BuildPath::try_from(invalid.to_owned()).is_err(),
            "{invalid}"
        );
    }
    assert!(ContentHash::try_from("ABC".to_owned()).is_err());
    assert!(serde_json::from_str::<BuildPath>("\"../escape\"").is_err());
    let mut builder = BuildCoordinator::new(ContentHash::of(b"session"));
    let ticket = builder.request(graph(), targets()).unwrap();
    let good = bundle(&builder, &ticket, &target(TargetKind::Web));
    good.validate().unwrap();
    let mut bad = good.clone();
    bad.manifest.source_maps[0].source_span.end = usize::MAX;
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.manifest.target = target(TargetKind::Native);
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.manifest.required_capabilities.insert("missing".into());
    assert!(bad.validate().is_err());
    let mut bad = good.clone();
    bad.manifest.toolchain.rustc_release = "different".into();
    assert!(
        builder
            .accept(&ticket, &bad, &mut MemoryStore::default())
            .is_err()
    );
    let mut bad = good;
    bad.files.get_mut(&path("app.wasm")).unwrap().push(0);
    assert!(bad.validate().is_err());
    let mut bad_graph = graph();
    bad_graph.files.insert(
        path("SRC/view.tsx"),
        bad_graph.files[&bad_graph.entry].clone(),
    );
    assert!(bad_graph.validate().is_err());
}

#[test]
fn stale_cancel_and_store_failures_never_replace_last_good() {
    let mut builder = BuildCoordinator::new(ContentHash::of(b"session-one"));
    let mut store = MemoryStore::default();
    let first = builder.request(graph(), targets()).unwrap();
    let native = bundle(&builder, &first, &target(TargetKind::Native));
    builder.accept(&first, &native, &mut store).unwrap();
    assert!(builder.publish(&first, &mut store).is_err());
    assert!(store.current().unwrap().is_none());
    let web = bundle(&builder, &first, &target(TargetKind::Web));
    builder.accept(&first, &web, &mut store).unwrap();
    store.fail_publish = true;
    assert!(builder.publish(&first, &mut store).is_err());
    assert!(builder.last_good().is_none());
    store.fail_publish = false;
    let published = builder.publish(&first, &mut store).unwrap();
    let second = builder.request(graph(), targets()).unwrap();
    assert!(
        builder
            .task(&second, &target(TargetKind::Web))
            .unwrap()
            .invalidated
            .is_empty()
    );
    let stale = bundle(&builder, &second, &target(TargetKind::Native));
    let third = builder.request(graph(), targets()).unwrap();
    assert!(builder.accept(&second, &stale, &mut store).is_err());
    assert!(builder.cancel(&second).is_err());
    let good = bundle(&builder, &third, &target(TargetKind::Native));
    store.fail_stage = true;
    assert!(builder.accept(&third, &good, &mut store).is_err());
    builder.cancel(&third).unwrap();
    assert!(builder.accept(&third, &good, &mut store).is_err());
    assert_eq!(builder.last_good(), Some(&published));
    assert_eq!(store.current().unwrap(), Some(published.clone()));
    let mut resumed = BuildCoordinator::resume(&store, ContentHash::of(b"session-two")).unwrap();
    let next = resumed.request(graph(), targets()).unwrap();
    assert_eq!(next.generation, second.generation);
    assert_ne!(next.session, second.session);
    assert!(resumed.accept(&second, &stale, &mut store).is_err());
    let mut updated = targets();
    updated
        .get_mut(&target(TargetKind::Web))
        .unwrap()
        .toolchain
        .settings
        .insert("profile".into(), "release".into());
    let next = resumed.request(graph(), updated).unwrap();
    assert_eq!(
        resumed
            .task(&next, &target(TargetKind::Web))
            .unwrap()
            .invalidated
            .len(),
        1
    );
}
