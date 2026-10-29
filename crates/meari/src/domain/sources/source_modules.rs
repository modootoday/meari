//! Declaration identity for source components, separate from mounted instances.
use super::super::{
    Component, Span,
    build::{BuildGraph, BuildPath, ContentHash, InputFile, InputRole},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentReference {
    pub module: BuildPath,
    pub export: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentImport {
    pub target: ComponentReference,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceComponentModule {
    pub content: ContentHash,
    pub source_bytes: u64,
    pub component: Component,
    /// Local import binding -> resolved declaration, retaining its original source range.
    pub imports: BTreeMap<String, ComponentImport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceModuleGraph {
    pub entry: ComponentReference,
    pub modules: BTreeMap<BuildPath, SourceComponentModule>,
}

impl SourceModuleGraph {
    /// Checks declaration graph integrity, not runtime behavior or full template semantics.
    pub fn validate(&self) -> Result<(), String> {
        let graph = BuildGraph {
            entry: self.entry.module.clone(),
            files: self
                .modules
                .iter()
                .map(|(path, module)| {
                    (
                        path.clone(),
                        InputFile {
                            role: InputRole::Component,
                            content: module.content.clone(),
                            length: module.source_bytes,
                            dependencies: module
                                .imports
                                .values()
                                .map(|import| import.target.module.clone())
                                .collect(),
                            license: None,
                        },
                    )
                })
                .collect(),
        };
        graph.validate()?;
        self.definition(&self.entry)?;
        let mut total_imports = 0usize;
        for module in self.modules.values() {
            total_imports = total_imports
                .checked_add(module.imports.len())
                .ok_or("source import budget overflow")?;
            if total_imports > 65536 {
                return Err("source import budget exceeded".into());
            }
            for (local, import) in &module.imports {
                if local.is_empty() || local == &module.component.name {
                    return Err("source import binding conflicts with its component".into());
                }
                if import.span.start > import.span.end
                    || import.span.end as u64 > module.source_bytes
                {
                    return Err("source import range is outside its module".into());
                }
                self.definition(&import.target)?;
            }
        }
        Ok(())
    }

    pub fn definition(
        &self,
        target: &ComponentReference,
    ) -> Result<&SourceComponentModule, String> {
        let module = self
            .modules
            .get(&target.module)
            .ok_or("source component module is missing")?;
        if module.component.name != target.export {
            return Err("named source component export is missing".into());
        }
        Ok(module)
    }

    pub fn fingerprint(&self) -> Result<ContentHash, String> {
        self.validate()?;
        Ok(ContentHash::of(
            &serde_json::to_vec(self).map_err(|error| error.to_string())?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(value: &str) -> BuildPath {
        value.to_owned().try_into().unwrap()
    }
    fn reference(file: &str, export: &str) -> ComponentReference {
        ComponentReference {
            module: path(file),
            export: export.into(),
        }
    }
    fn module(name: &str) -> SourceComponentModule {
        let source =
            format!("type Props = {{}}; export function {name}(props: Props) {{ return <div/>; }}");
        SourceComponentModule {
            content: ContentHash::of(source.as_bytes()),
            source_bytes: source.len() as u64,
            component: crate::parse(&source, crate::application::Language::Tsx).unwrap(),
            imports: BTreeMap::new(),
        }
    }
    fn graph() -> SourceModuleGraph {
        let mut viewing = module("Viewing");
        viewing.imports.insert(
            "WindowChrome".into(),
            ComponentImport {
                target: reference("components/window-chrome.tsx", "WindowChrome"),
                span: Span { start: 0, end: 8 },
            },
        );
        SourceModuleGraph {
            entry: reference("viewing.tsx", "Viewing"),
            modules: BTreeMap::from([
                (path("viewing.tsx"), viewing),
                (path("components/window-chrome.tsx"), module("WindowChrome")),
            ]),
        }
    }

    #[test]
    fn graph_retains_named_export_and_import_origin_and_fingerprints_changes() {
        let mut graph = graph();
        graph.validate().unwrap();
        assert_eq!(
            graph.definition(&graph.entry).unwrap().component.name,
            "Viewing"
        );
        let before = graph.fingerprint().unwrap();
        graph
            .modules
            .get_mut(&path("components/window-chrome.tsx"))
            .unwrap()
            .content = ContentHash::of(b"changed source");
        assert_ne!(graph.fingerprint().unwrap(), before);
        let json = serde_json::to_vec(&graph).unwrap();
        assert_eq!(
            serde_json::from_slice::<SourceModuleGraph>(&json).unwrap(),
            graph
        );
    }

    #[test]
    fn graph_rejects_missing_exports_cycles_and_out_of_source_import_ranges() {
        let mut missing = graph();
        missing.entry.export = "Missing".into();
        assert!(missing.validate().unwrap_err().contains("export"));
        let mut cycle = graph();
        cycle
            .modules
            .get_mut(&path("components/window-chrome.tsx"))
            .unwrap()
            .imports
            .insert(
                "Viewing".into(),
                ComponentImport {
                    target: reference("viewing.tsx", "Viewing"),
                    span: Span { start: 0, end: 8 },
                },
            );
        assert!(cycle.validate().unwrap_err().contains("cycle"));
        let mut range = graph();
        range
            .modules
            .get_mut(&path("viewing.tsx"))
            .unwrap()
            .imports
            .get_mut("WindowChrome")
            .unwrap()
            .span
            .end = usize::MAX;
        assert!(range.validate().unwrap_err().contains("range"));
    }
}
