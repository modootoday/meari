use super::project::{self, ProjectRequest};
use crate::infrastructure::{
    compiler::{CompilerRegistration, RustAppCompiler},
    files::ProjectInputs,
    tools::RegisteredTool,
};
use meari::{
    application::builder::*, domain::build::*, infrastructure::artifact_store::FsArtifactStore,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRegistration {
    pub path: PathBuf,
    pub sha256: ContentHash,
}
impl ToolRegistration {
    pub fn load(&self) -> Result<RegisteredTool, String> {
        RegisteredTool::register(&self.path, self.sha256.clone())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRequest {
    pub project_root: PathBuf,
    pub framework_root: PathBuf,
    pub work_root: PathBuf,
    pub target_dir: PathBuf,
    pub store_root: PathBuf,
    pub project: ProjectRequest,
    pub targets: Vec<BuildTarget>,
    pub cargo: ToolRegistration,
    pub rustc: ToolRegistration,
    pub trunk: Option<ToolRegistration>,
    pub wasm_bindgen: Option<ToolRegistration>,
    pub tailwind: Option<PathBuf>,
}
pub fn execute(request: BuildRequest) -> Result<BuildPublication, BuildFailure> {
    let inputs = ProjectInputs::new(&request.project_root)?;
    let project = request.project.load(&inputs)?;
    let config = project::configuration(&project.configuration, &project.bytes)?;
    let compiler = RustAppCompiler::new(CompilerRegistration {
        framework_root: request.framework_root,
        work_root: request.work_root,
        target_dir: request.target_dir,
        configuration: project.configuration.clone(),
        cargo: request.cargo.load()?,
        rustc: request.rustc.load()?,
        trunk: request.trunk.map(|tool| tool.load()).transpose()?,
        wasm_bindgen: request.wasm_bindgen.map(|tool| tool.load()).transpose()?,
        tailwind: request.tailwind,
    })?;
    let mut specifications = BTreeMap::new();
    for target in request.targets {
        let spec = compiler.specification(&target, &config)?;
        if specifications.insert(target, spec).is_some() {
            return Err("duplicate requested build target".into());
        }
    }
    let mut store = FsArtifactStore::open(&request.store_root)?;
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let mut coordinator = BuildCoordinator::resume(&store, ContentHash::of(&random))?;
    let ticket = coordinator.request(project.graph.clone(), specifications.clone())?;
    for target in specifications.keys() {
        let task = coordinator.task(&ticket, target)?;
        let bundle = compile_task(&task, &compiler, &project)?;
        coordinator.accept(&ticket, &bundle, &mut store)?;
    }
    coordinator.publish(&ticket, &mut store).map_err(Into::into)
}
