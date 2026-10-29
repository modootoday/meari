//! Target-independent build preparation and publication. Runtime frame ACKs remain HMR's job.
use crate::domain::build::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, serde::Serialize)]
pub struct BuildFailure {
    pub message: String,
    pub diagnostics: Vec<BuildDiagnostic>,
}
impl BuildFailure {
    pub fn source(path: BuildPath, diagnostic: crate::domain::Diagnostic) -> Self {
        Self {
            message: diagnostic.message.clone(),
            diagnostics: vec![BuildDiagnostic {
                severity: BuildSeverity::Error,
                message: diagnostic.message,
                source: Some(path),
                span: Some(diagnostic.span),
                code: Some(diagnostic.code),
            }],
        }
    }
    pub fn file(path: BuildPath, message: String) -> Self {
        Self {
            message: message.clone(),
            diagnostics: vec![BuildDiagnostic {
                severity: BuildSeverity::Error,
                message,
                source: Some(path),
                span: None,
                code: Some("build-input".into()),
            }],
        }
    }
}
impl From<String> for BuildFailure {
    fn from(message: String) -> Self {
        Self {
            message: message.clone(),
            diagnostics: vec![BuildDiagnostic {
                severity: BuildSeverity::Error,
                message,
                source: None,
                span: None,
                code: Some("build".into()),
            }],
        }
    }
}
impl From<&str> for BuildFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl std::fmt::Display for BuildFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for BuildFailure {}

#[derive(Clone, Debug)]
pub struct BuildBundle {
    pub manifest: BuildManifest,
    pub files: BTreeMap<BuildPath, Vec<u8>>,
}
impl BuildBundle {
    /// Load the original, unbound component used by both runtime hosts.
    pub fn application_definition(
        &self,
    ) -> Result<super::app_bundle::ApplicationDefinition, String> {
        self.validate()?;
        if self.files.contains_key(&BuildPath::try_from(
            "runtime/module-application.json".to_owned(),
        )?) {
            return Err("module application requires module_application_definition; single-source HMR is unsupported".into());
        }
        let path = BuildPath::try_from("runtime/application.json".to_owned())?;
        let bytes = self
            .files
            .get(&path)
            .ok_or("runtime application is missing")?;
        let definition: super::app_bundle::ApplicationDefinition =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        definition.validate_manifest(&self.manifest)?;
        Ok(definition)
    }

    pub fn hmr_artifact(&self) -> Result<super::hmr::Artifact, String> {
        let definition = self.application_definition()?;
        Ok(super::hmr::Artifact {
            source_hash: self.manifest.graph_hash.as_str().into(),
            artifact_id: self.manifest.identity()?.as_str().into(),
            state_schema: definition.source_schema()?.fingerprint()?.as_str().into(),
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.manifest.validate()?;
        if self.files.len() != self.manifest.outputs.len() {
            return Err("output inventory differs from manifest".into());
        }
        for (path, output) in &self.manifest.outputs {
            let bytes = self.files.get(path).ok_or("manifest output is missing")?;
            if bytes.len() as u64 != output.length || ContentHash::of(bytes) != output.content {
                return Err("output length/hash differs from manifest".into());
            }
        }
        Ok(())
    }
}
pub trait ArtifactStore {
    fn stage(&mut self, bundle: &BuildBundle) -> Result<ArtifactRef, String>;
    fn publish(&mut self, publication: &BuildPublication) -> Result<(), String>;
    fn current(&self) -> Result<Option<BuildPublication>, String>;
    fn manifest(&self, artifact: &ArtifactRef) -> Result<BuildManifest, String>;
}
pub trait BuildInputs {
    fn read(&self, path: &BuildPath) -> Result<Vec<u8>, String>;
}
pub trait TargetCompiler {
    fn compile(
        &self,
        task: &BuildTask,
        inputs: &BTreeMap<BuildPath, Vec<u8>>,
    ) -> Result<BuildBundle, BuildFailure>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildTicket {
    pub session: ContentHash,
    pub generation: u64,
    pub graph_hash: ContentHash,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetBuildSpec {
    pub available_capabilities: BTreeSet<String>,
    pub toolchain: BuildToolchain,
}
#[derive(Clone, Debug)]
pub struct BuildTask {
    pub ticket: BuildTicket,
    pub graph: BuildGraph,
    pub target: BuildTarget,
    pub available_capabilities: BTreeSet<String>,
    pub toolchain: BuildToolchain,
    pub invalidated: BTreeSet<BuildPath>,
}
pub fn compile_task(
    task: &BuildTask,
    compiler: &dyn TargetCompiler,
    inputs: &dyn BuildInputs,
) -> Result<BuildBundle, BuildFailure> {
    if task.ticket.generation == 0 || task.ticket.graph_hash != task.graph.fingerprint()? {
        return Err("invalid build task identity".into());
    }
    task.target.validate()?;
    let mut loaded = BTreeMap::new();
    for (path, input) in &task.graph.files {
        let bytes = inputs
            .read(path)
            .map_err(|message| BuildFailure::file(path.clone(), message))?;
        if bytes.len() as u64 != input.length || ContentHash::of(&bytes) != input.content {
            return Err(BuildFailure::file(
                path.clone(),
                "source input changed after build request".into(),
            ));
        }
        loaded.insert(path.clone(), bytes);
    }
    let bundle = compiler.compile(task, &loaded)?;
    validate_result(task, &bundle)?;
    Ok(bundle)
}
fn validate_result(task: &BuildTask, bundle: &BuildBundle) -> Result<(), String> {
    bundle.validate()?;
    if bundle.manifest.available_capabilities != task.available_capabilities
        || bundle.manifest.session != task.ticket.session
        || bundle.manifest.toolchain != task.toolchain
        || bundle.manifest.generation != task.ticket.generation
        || bundle.manifest.graph_hash != task.ticket.graph_hash
        || bundle.manifest.graph != task.graph
        || bundle.manifest.target != task.target
        || !bundle
            .manifest
            .required_capabilities
            .is_subset(&task.available_capabilities)
    {
        return Err("compiler result does not match task/capabilities".into());
    }
    Ok(())
}
struct Pending {
    ticket: BuildTicket,
    graph: BuildGraph,
    targets: BTreeMap<BuildTarget, TargetBuildSpec>,
    artifacts: BTreeMap<BuildTarget, ArtifactRef>,
    invalidated: BTreeSet<BuildPath>,
}
pub struct BuildCoordinator {
    session: ContentHash,
    sequence: u64,
    pending: Option<Pending>,
    last_good: Option<BuildPublication>,
    last_graph: Option<BuildGraph>,
    last_specs: BTreeMap<BuildTarget, TargetBuildSpec>,
}
impl BuildCoordinator {
    pub fn new(session: ContentHash) -> Self {
        Self {
            session,
            sequence: 0,
            pending: None,
            last_good: None,
            last_graph: None,
            last_specs: BTreeMap::new(),
        }
    }
    pub fn resume(store: &dyn ArtifactStore, session: ContentHash) -> Result<Self, String> {
        let mut coordinator = Self::new(session);
        if let Some(publication) = store.current()? {
            publication.validate()?;
            if publication.session == coordinator.session {
                return Err("resumed builder requires a fresh host session".into());
            }
            let mut graph = None;
            for artifact in &publication.artifacts {
                let manifest = store.manifest(artifact)?;
                manifest.validate()?;
                if manifest.session != publication.session
                    || manifest.generation != publication.generation
                    || manifest.graph_hash != publication.graph_hash
                    || manifest.target != artifact.target
                    || manifest.identity()? != artifact.identity
                {
                    return Err("stored publication/manifest mismatch".into());
                }
                coordinator.last_specs.insert(
                    manifest.target.clone(),
                    TargetBuildSpec {
                        available_capabilities: manifest.available_capabilities,
                        toolchain: manifest.toolchain,
                    },
                );
                graph = Some(manifest.graph);
            }
            coordinator.sequence = publication.generation;
            coordinator.last_graph = graph;
            coordinator.last_good = Some(publication);
        }
        Ok(coordinator)
    }
    pub fn request(
        &mut self,
        graph: BuildGraph,
        targets: BTreeMap<BuildTarget, TargetBuildSpec>,
    ) -> Result<BuildTicket, String> {
        let graph_hash = graph.fingerprint()?;
        if targets.is_empty() || targets.len() > 16 {
            return Err("one to sixteen build targets required".into());
        }
        for target in targets.keys() {
            target.validate()?;
        }
        let generation = self
            .sequence
            .checked_add(1)
            .ok_or("build generation exhausted")?;
        let mut invalidated = match &self.last_graph {
            Some(previous) => graph.changes_from(previous)?.invalidated,
            None => graph.files.keys().cloned().collect(),
        };
        if targets
            .iter()
            .any(|(target, spec)| self.last_specs.get(target) != Some(spec))
        {
            invalidated.extend(graph.files.keys().cloned());
        }
        let ticket = BuildTicket {
            session: self.session.clone(),
            generation,
            graph_hash,
        };
        self.pending = Some(Pending {
            ticket: ticket.clone(),
            graph,
            targets,
            artifacts: BTreeMap::new(),
            invalidated,
        });
        self.sequence = generation;
        Ok(ticket)
    }
    fn pending(&self, ticket: &BuildTicket) -> Result<&Pending, String> {
        self.pending
            .as_ref()
            .filter(|pending| pending.ticket == *ticket)
            .ok_or_else(|| "stale or cancelled build ticket".into())
    }
    pub fn task(&self, ticket: &BuildTicket, target: &BuildTarget) -> Result<BuildTask, String> {
        let pending = self.pending(ticket)?;
        let spec = pending
            .targets
            .get(target)
            .ok_or("target was not requested")?;
        Ok(BuildTask {
            ticket: ticket.clone(),
            graph: pending.graph.clone(),
            target: target.clone(),
            available_capabilities: spec.available_capabilities.clone(),
            toolchain: spec.toolchain.clone(),
            invalidated: pending.invalidated.clone(),
        })
    }
    pub fn accept(
        &mut self,
        ticket: &BuildTicket,
        bundle: &BuildBundle,
        store: &mut dyn ArtifactStore,
    ) -> Result<ArtifactRef, String> {
        let pending = self.pending(ticket)?;
        if pending.artifacts.contains_key(&bundle.manifest.target) {
            return Err("target already prepared".into());
        }
        validate_result(&self.task(ticket, &bundle.manifest.target)?, bundle)?;
        let expected = ArtifactRef {
            target: bundle.manifest.target.clone(),
            identity: bundle.manifest.identity()?,
        };
        let stored = store.stage(bundle)?;
        if stored != expected {
            return Err("store returned the wrong artifact identity".into());
        }
        self.pending
            .as_mut()
            .expect("pending build retained")
            .artifacts
            .insert(stored.target.clone(), stored.clone());
        Ok(stored)
    }
    pub fn publish(
        &mut self,
        ticket: &BuildTicket,
        store: &mut dyn ArtifactStore,
    ) -> Result<BuildPublication, String> {
        let pending = self.pending(ticket)?;
        if pending.artifacts.len() != pending.targets.len() {
            return Err("all requested targets must be prepared".into());
        }
        let publication = BuildPublication {
            session: ticket.session.clone(),
            generation: ticket.generation,
            graph_hash: ticket.graph_hash.clone(),
            artifacts: pending.artifacts.values().cloned().collect(),
        };
        publication.validate()?;
        store.publish(&publication)?;
        let graph = pending.graph.clone();
        let specs = pending.targets.clone();
        self.last_graph = Some(graph);
        self.last_specs = specs;
        self.last_good = Some(publication.clone());
        self.pending = None;
        Ok(publication)
    }
    pub fn cancel(&mut self, ticket: &BuildTicket) -> Result<(), String> {
        self.pending(ticket)?;
        self.pending = None;
        Ok(())
    }
    pub fn last_good(&self) -> Option<&BuildPublication> {
        self.last_good.as_ref()
    }
}
