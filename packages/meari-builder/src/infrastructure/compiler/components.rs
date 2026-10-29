//! Compile declared component inputs while retaining module identity and original source.
use crate::application::project::AppConfiguration;
use meari::{
    application::{
        Language,
        app_bundle::{AppBuildStamp, ApplicationDefinition},
        builder::{BuildFailure, BuildTask},
        module_app_bundle::ModuleApplicationDefinition,
    },
    domain::{Component, Span, build::*},
};
use std::collections::BTreeMap;

pub(super) struct Components {
    pub definition: ApplicationDefinition,
    pub modules: Option<ModuleApplicationDefinition>,
    pub projected: Component,
    pub files: BTreeMap<BuildPath, Vec<u8>>,
    pub mappings: Vec<BuildMapping>,
}

impl Components {
    pub fn validate_web(&self) -> Result<(), BuildFailure> {
        if let Some(modules) = &self.modules {
            for (path, module) in &modules.modules.modules {
                meari::application::dom::validate_source(&module.component)
                    .map_err(|message| BuildFailure::file(path.clone(), message))?;
            }
            let program = modules.program("builder-web-preflight".into(), 1)?;
            let projection = program
                .project()
                .map_err(|error| BuildFailure::source(error.file, error.diagnostic))?;
            if let Some(path) = projection.input_paths().next() {
                let origin = projection
                    .origins
                    .get(path)
                    .ok_or("module input origin missing")?;
                return Err(BuildFailure::source(
                    origin.module.clone(),
                    meari::domain::Diagnostic::unsupported(
                        origin.span,
                        "Web module inputs require the typed property/listener adapter",
                    ),
                ));
            }
        } else {
            meari::application::dom::validate_source(&self.definition.component)?;
            let program = meari::application::state_program::StateProgram::mount(
                self.projected.clone(),
                "builder-web-preflight".into(),
                1,
            )?;
            meari::application::dom::prepare(&program.project()?.component)?;
            return Ok(());
        }
        meari::application::dom::prepare(&self.projected)?;
        Ok(())
    }
}

pub(super) fn prepare(
    task: &BuildTask,
    inputs: &BTreeMap<BuildPath, Vec<u8>>,
    config: &AppConfiguration,
) -> Result<Components, BuildFailure> {
    let mut sources = BTreeMap::new();
    for (path, input) in &task.graph.files {
        if input.role == InputRole::Component {
            let bytes = inputs.get(path).ok_or("component input missing")?;
            if ContentHash::of(bytes) != input.content || bytes.len() as u64 != input.length {
                return Err(BuildFailure::file(
                    path.clone(),
                    "component differs from graph inventory".into(),
                ));
            }
            let source = std::str::from_utf8(bytes)
                .map_err(|error| BuildFailure::file(path.clone(), error.to_string()))?;
            sources.insert(path.clone(), source.to_owned());
        }
    }
    let source = sources
        .get(&task.graph.entry)
        .ok_or("component entry missing")?;
    // TSX always uses the module frontend, including a one-file graph with React imports.
    // Rust retains its established single-source parser until its module frontend exists.
    let graph = if matches!(config.language, Language::Tsx) {
        Some(
            meari::parse_module_graph(&task.graph.entry, &sources)
                .map_err(|error| BuildFailure::source(error.file, error.diagnostic))?,
        )
    } else {
        if sources.len() != 1 {
            return Err("Rust component modules are not supported".into());
        }
        None
    };
    let component = match &graph {
        Some(graph) => graph.definition(&graph.entry)?.component.clone(),
        None => meari::parse(source, config.language)
            .map_err(|error| BuildFailure::source(task.graph.entry.clone(), error))?,
    };
    let definition = ApplicationDefinition {
        component,
        props: config.props.clone(),
        title: config.title.clone(),
        native_viewport: config.native_viewport.clone(),
        stamp: AppBuildStamp {
            session: task.ticket.session.clone(),
            generation: task.ticket.generation,
            graph_hash: task.ticket.graph_hash.clone(),
            target: task.target.clone(),
        },
    };
    let modules = graph
        .filter(|graph| {
            graph.modules.len() > 1
                || graph
                    .modules
                    .values()
                    .any(|module| !module.imports.is_empty())
        })
        .map(|modules| ModuleApplicationDefinition {
            schema_version: 1,
            application: definition.clone(),
            modules,
        });
    let projected = if let Some(modules) = &modules {
        modules.validate()?;
        modules
            .program("builder-preflight".into(), 1)?
            .project()
            .map_err(|error| BuildFailure::source(error.file, error.diagnostic))?
            .component
            .clone()
    } else {
        definition.validate()?;
        definition.bound_component()?
    };
    let mut files = BTreeMap::new();
    let mut mappings = Vec::new();
    for (path, source) in &sources {
        let copied = BuildPath::try_from(format!("source/original/{}", path.as_str()))?;
        files.insert(copied.clone(), source.as_bytes().to_vec());
        // This is a source-preservation map, not a claim that expanded runtime nodes
        // retain the coordinates of a single flattened input.
        mappings.push(BuildMapping {
            source: path.clone(),
            generated: copied,
            source_span: Span {
                start: 0,
                end: source.len(),
            },
            generated_span: Span {
                start: 0,
                end: source.len(),
            },
        });
    }
    if modules.is_none() {
        let output = meari::emit(&definition.component, Language::Rust)
            .map_err(|error| BuildFailure::source(task.graph.entry.clone(), error))?;
        let converted = BuildPath::try_from("source/converted.rs".to_owned())?;
        mappings.extend(output.mappings.into_iter().map(|mapping| BuildMapping {
            source: task.graph.entry.clone(),
            generated: converted.clone(),
            source_span: mapping.source,
            generated_span: mapping.generated,
        }));
        files.insert(converted, output.code.into_bytes());
    }
    Ok(Components {
        definition,
        modules,
        projected,
        files,
        mappings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use meari::application::builder::BuildTicket;
    use meari::domain::NodeKind;
    use std::collections::BTreeSet;

    fn path(value: &str) -> BuildPath {
        value.to_owned().try_into().unwrap()
    }
    fn fixture() -> (BuildTask, BTreeMap<BuildPath, Vec<u8>>, AppConfiguration) {
        let sources = BTreeMap::from([
            (path("view.tsx"), b"import { Counter } from './counter'; type Props={caption:string}; export function View(props:Props){return <main><Counter caption={props.caption}/><Counter caption={props.caption}/></main>;}".to_vec()),
            (path("counter.tsx"), b"import {useState} from 'react'; type Props={caption:string}; export function Counter(props:Props){const [count,setCount]=useState(0);return <button onClick={()=>setCount(previous=>previous+1)}>{props.caption}{count}</button>;}".to_vec()),
        ]);
        let graph = BuildGraph {
            entry: path("view.tsx"),
            files: sources
                .iter()
                .map(|(path, source)| {
                    (
                        path.clone(),
                        InputFile {
                            role: InputRole::Component,
                            content: ContentHash::of(source),
                            length: source.len() as u64,
                            dependencies: BTreeSet::new(),
                            license: None,
                        },
                    )
                })
                .collect(),
        };
        let task = BuildTask {
            ticket: BuildTicket {
                session: ContentHash::of(b"session"),
                generation: 1,
                graph_hash: graph.fingerprint().unwrap(),
            },
            graph,
            target: BuildTarget {
                kind: TargetKind::Native,
                triple: "x86_64-pc-windows-msvc".into(),
            },
            available_capabilities: BTreeSet::new(),
            invalidated: BTreeSet::new(),
            toolchain: BuildToolchain {
                rustc_release: "test".into(),
                rustc_commit: "test".into(),
                cargo_release: "test".into(),
                providers: BTreeMap::new(),
                settings: BTreeMap::new(),
            },
        };
        let config = serde_json::from_value(serde_json::json!({"language":"tsx","title":"Graph proof","native_viewport":{"width":640,"height":480,"dark":false},"props":{"caption":{"String":"Counter"}},"stylesheet":{"mode":"css","entry":"styles.css","bridge_theme":false}})).unwrap();
        (task, sources, config)
    }

    #[test]
    fn graph_bundle_retains_original_sources_and_independent_child_state() {
        let (task, sources, config) = fixture();
        let compiled = prepare(&task, &sources, &config).unwrap();
        compiled.validate_web().unwrap();
        let definition = compiled.modules.as_ref().unwrap();
        assert_eq!(definition.modules.modules.len(), 2);
        assert!(matches!(
            compiled.projected.root.kind,
            NodeKind::Element { .. }
        ));
        assert_eq!(compiled.mappings.len(), 2);
        for (path, source) in &sources {
            assert_eq!(
                compiled.files[&self::path(&format!("source/original/{}", path.as_str()))],
                *source
            );
        }
        assert!(!compiled.files.contains_key(&path("source/converted.rs")));
        let mut program = definition.program("test".into(), 1).unwrap();
        let before = program.project().unwrap().clone();
        assert_eq!(before.instances.len(), 3);
        let event = before.event_paths().next().unwrap().clone();
        program.activate(&event).unwrap();
        let after = program.project().unwrap();
        assert_ne!(before.instances, after.instances);
        assert_eq!(after.instances.len(), 3);
    }

    #[test]
    fn generated_factories_and_bundle_loader_preserve_graph_and_reject_root_only_hmr() {
        use meari::application::builder::BuildBundle;
        let (mut task, sources, config) = fixture();
        task.toolchain.providers.insert(
            "test-only".into(),
            ProviderPin {
                version: "1".into(),
                revision: "fixture".into(),
                content: ContentHash::of(b"fixture"),
            },
        );
        let compiled = prepare(&task, &sources, &config).unwrap();
        let modules = compiled.modules.as_ref().unwrap();
        let framework = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let (_, mut files) = crate::infrastructure::generate::project(
            framework,
            &compiled.definition,
            Some(modules),
            "button{color:red}",
            None,
        )
        .unwrap();
        let factory = std::str::from_utf8(&files[&path("src/application.rs")]).unwrap();
        assert!(factory.contains("InteractiveDocument::from_module_application"));
        let mut web = modules.clone();
        web.application.stamp.target = BuildTarget {
            kind: TargetKind::Web,
            triple: "wasm32-unknown-unknown".into(),
        };
        let (_, web_files) = crate::infrastructure::generate::project(
            framework,
            &web.application,
            Some(&web),
            "",
            None,
        )
        .unwrap();
        assert!(
            std::str::from_utf8(&web_files[&path("src/application.rs")])
                .unwrap()
                .contains("WebDocument::mount_module_application")
        );
        files.insert(
            path("runtime/application.json"),
            serde_json::to_vec(&compiled.definition).unwrap(),
        );
        files.insert(
            path("runtime/module-application.json"),
            serde_json::to_vec(modules).unwrap(),
        );
        files.extend(compiled.files.clone());
        // Synthetic bytes exercise metadata integrity only, never executable acceptance.
        files.insert(path("test-only.exe"), b"not an executable".to_vec());
        let outputs = files
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    OutputFile {
                        role: if path.as_str() == "test-only.exe" {
                            OutputRole::NativeExecutable
                        } else if path.as_str().starts_with("source/original/") {
                            OutputRole::GeneratedSource
                        } else {
                            OutputRole::Asset
                        },
                        content: ContentHash::of(bytes),
                        length: bytes.len() as u64,
                    },
                )
            })
            .collect();
        let mut bundle = BuildBundle {
            files,
            manifest: BuildManifest {
                schema_version: 1,
                session: task.ticket.session,
                generation: task.ticket.generation,
                graph_hash: task.ticket.graph_hash,
                graph: task.graph,
                target: task.target,
                toolchain: task.toolchain,
                outputs,
                source_maps: compiled.mappings.clone(),
                available_capabilities: BTreeSet::new(),
                required_capabilities: BTreeSet::new(),
                diagnostics: vec![],
            },
        };
        assert_eq!(
            bundle
                .module_application_definition()
                .unwrap()
                .unwrap()
                .modules,
            modules.modules
        );
        assert!(
            bundle
                .application_definition()
                .unwrap_err()
                .contains("module_application_definition")
        );
        assert!(bundle.hmr_artifact().is_err());
        let mut mismatch = modules.clone();
        mismatch
            .modules
            .modules
            .get_mut(&path("counter.tsx"))
            .unwrap()
            .content = ContentHash::of(b"other source");
        let bytes = serde_json::to_vec(&mismatch).unwrap();
        let output = bundle
            .manifest
            .outputs
            .get_mut(&path("runtime/module-application.json"))
            .unwrap();
        output.content = ContentHash::of(&bytes);
        output.length = bytes.len() as u64;
        bundle
            .files
            .insert(path("runtime/module-application.json"), bytes);
        assert!(
            bundle
                .module_application_definition()
                .unwrap_err()
                .contains("source identity")
        );
    }

    #[test]
    fn module_failure_keeps_original_importer_span_and_inventory_hash_is_enforced() {
        let (task, mut sources, config) = fixture();
        sources.insert(path("counter.tsx"), b"changed".to_vec());
        let failure = prepare(&task, &sources, &config).err().unwrap();
        assert_eq!(failure.diagnostics[0].source, Some(path("counter.tsx")));
        let (mut task, mut sources, config) = fixture();
        task.graph.files.remove(&path("counter.tsx"));
        sources.remove(&path("counter.tsx"));
        let failure = prepare(&task, &sources, &config).err().unwrap();
        assert_eq!(failure.diagnostics[0].source, Some(path("view.tsx")));
        assert_eq!(failure.diagnostics[0].span.unwrap().start, 0);
        assert!(failure.diagnostics[0].span.unwrap().end > 0);
    }
}
