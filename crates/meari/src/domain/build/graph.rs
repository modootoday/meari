use super::{BuildPath, ContentHash, identity::unique_paths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputRole {
    Component,
    Stylesheet,
    Icon,
    Font,
    Image,
    Runtime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub role: InputRole,
    pub content: ContentHash,
    pub length: u64,
    pub dependencies: BTreeSet<BuildPath>,
    pub license: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildGraph {
    pub entry: BuildPath,
    pub files: BTreeMap<BuildPath, InputFile>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphDelta {
    pub changed: BTreeSet<BuildPath>,
    pub invalidated: BTreeSet<BuildPath>,
}

impl BuildGraph {
    pub fn validate(&self) -> Result<(), String> {
        unique_paths(self.files.keys())?;
        if self.files.len() > 16384
            || !self
                .files
                .get(&self.entry)
                .is_some_and(|file| file.role == InputRole::Component)
        {
            return Err("component graph entry and bounded input inventory required".into());
        }
        for file in self.files.values() {
            if file
                .license
                .as_ref()
                .is_some_and(|license| !super::identity::nonempty(license))
            {
                return Err("invalid observed license".into());
            }
            if file
                .dependencies
                .iter()
                .any(|path| !self.files.contains_key(path))
            {
                return Err("build graph dependency is missing".into());
            }
        }
        // Iterative topological elimination avoids recursion on untrusted graphs.
        let mut remaining: BTreeMap<_, _> = self
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.dependencies.len()))
            .collect();
        let mut dependents: BTreeMap<BuildPath, Vec<BuildPath>> = BTreeMap::new();
        for (path, file) in &self.files {
            for dependency in &file.dependencies {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(path.clone());
            }
        }
        let mut ready: BTreeSet<_> = remaining
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(path, _)| path.clone())
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(path) = ready.pop_first() {
            if !visited.insert(path.clone()) {
                continue;
            }
            for dependent in dependents.get(&path).into_iter().flatten() {
                let count = remaining.get_mut(dependent).expect("validated dependency");
                *count -= 1;
                if *count == 0 {
                    ready.insert(dependent.clone());
                }
            }
        }
        if visited.len() != self.files.len() {
            return Err("build dependency cycle".into());
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<ContentHash, String> {
        self.validate()?;
        Ok(ContentHash::of(
            &serde_json::to_vec(self).map_err(|error| error.to_string())?,
        ))
    }
    pub fn changes_from(&self, previous: &Self) -> Result<GraphDelta, String> {
        self.validate()?;
        previous.validate()?;
        let mut changed: BTreeSet<_> = self
            .files
            .keys()
            .chain(previous.files.keys())
            .filter(|path| self.files.get(*path) != previous.files.get(*path))
            .cloned()
            .collect();
        if self.entry != previous.entry {
            changed.insert(self.entry.clone());
            changed.insert(previous.entry.clone());
        }
        let mut dependents: BTreeMap<BuildPath, BTreeSet<BuildPath>> = BTreeMap::new();
        for (path, file) in self.files.iter().chain(previous.files.iter()) {
            for dependency in &file.dependencies {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .insert(path.clone());
            }
        }
        let mut pending = changed.clone();
        let mut invalidated = BTreeSet::new();
        while let Some(path) = pending.pop_first() {
            if !invalidated.insert(path.clone()) {
                continue;
            }
            if let Some(parents) = dependents.get(&path) {
                pending.extend(parents.iter().cloned());
            }
        }
        Ok(GraphDelta {
            changed,
            invalidated,
        })
    }
}
