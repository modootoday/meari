use meari::{
    application::{
        Language,
        builder::{BuildFailure, BuildInputs},
    },
    domain::{
        Value,
        build::*,
        design::{
            stylesheet::{StylesheetMode, StylesheetProject},
            theme::Theme,
        },
        viewport::Viewport,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub role: InputRole,
    pub dependencies: BTreeSet<BuildPath>,
    pub license: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRequest {
    pub entry: BuildPath,
    pub configuration: BuildPath,
    pub files: BTreeMap<BuildPath, ProjectFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleConfiguration {
    pub mode: StylesheetMode,
    pub entry: BuildPath,
    pub bridge_theme: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfiguration {
    pub language: Language,
    pub title: String,
    pub native_viewport: Viewport,
    pub props: BTreeMap<String, Value>,
    pub stylesheet: StyleConfiguration,
    #[serde(default)]
    pub theme: Theme,
}
pub struct LoadedProject {
    pub graph: BuildGraph,
    pub bytes: BTreeMap<BuildPath, Vec<u8>>,
    pub configuration: BuildPath,
}
impl BuildInputs for LoadedProject {
    fn read(&self, path: &BuildPath) -> Result<Vec<u8>, String> {
        self.bytes
            .get(path)
            .cloned()
            .ok_or_else(|| "project input is missing".into())
    }
}
impl ProjectRequest {
    pub fn load(&self, inputs: &dyn BuildInputs) -> Result<LoadedProject, BuildFailure> {
        if self.files.is_empty() || self.files.len() > 16384 {
            return Err("project file inventory exceeds budget".into());
        }
        if self
            .files
            .get(&self.configuration)
            .is_none_or(|file| file.role != InputRole::Runtime)
        {
            return Err("project configuration must be a declared runtime input".into());
        }
        let mut files = BTreeMap::new();
        let mut bytes = BTreeMap::new();
        let mut total = 0usize;
        for (path, file) in &self.files {
            let content = inputs
                .read(path)
                .map_err(|message| BuildFailure::file(path.clone(), message))?;
            total = total
                .checked_add(content.len())
                .ok_or("project byte budget overflow")?;
            if total > 256 * 1024 * 1024 {
                return Err("project exceeds byte budget".into());
            }
            files.insert(
                path.clone(),
                InputFile {
                    role: file.role.clone(),
                    content: ContentHash::of(&content),
                    length: content.len() as u64,
                    dependencies: file.dependencies.clone(),
                    license: file.license.clone(),
                },
            );
            bytes.insert(path.clone(), content);
        }
        let dependencies = files
            .keys()
            .filter(|path| **path != self.entry)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(entry) = files.get_mut(&self.entry) {
            entry.dependencies.extend(dependencies);
        }
        let graph = BuildGraph {
            entry: self.entry.clone(),
            files,
        };
        graph.validate()?;
        Ok(LoadedProject {
            graph,
            bytes,
            configuration: self.configuration.clone(),
        })
    }
}
pub fn configuration(
    path: &BuildPath,
    inputs: &BTreeMap<BuildPath, Vec<u8>>,
) -> Result<AppConfiguration, BuildFailure> {
    serde_json::from_slice(inputs.get(path).ok_or("app configuration is missing")?)
        .map_err(|error| BuildFailure::file(path.clone(), error.to_string()))
}
pub fn stylesheet(
    graph: &BuildGraph,
    inputs: &BTreeMap<BuildPath, Vec<u8>>,
    config: &StyleConfiguration,
) -> Result<StylesheetProject, BuildFailure> {
    let mut files = BTreeMap::new();
    for (path, file) in &graph.files {
        if file.role == InputRole::Stylesheet {
            files.insert(
                path.as_str().to_owned(),
                String::from_utf8(inputs.get(path).ok_or("stylesheet input missing")?.clone())
                    .map_err(|error| BuildFailure::file(path.clone(), error.to_string()))?,
            );
        }
    }
    let project = StylesheetProject {
        mode: config.mode,
        entry: config.entry.as_str().into(),
        files,
        bridge_theme: config.bridge_theme,
    };
    project.validate()?;
    Ok(project)
}
