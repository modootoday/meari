//! Local component resolution over a declared inventory, without filesystem access.
use crate::domain::{
    Component, Diagnostic, Span,
    build::{BuildPath, ContentHash},
    source_modules::{
        ComponentImport, ComponentReference, SourceComponentModule, SourceModuleGraph,
    },
};
use std::collections::{BTreeMap, BTreeSet};
pub mod child_placement;
pub mod keyed_reconciliation;
mod node_slots;
mod program;
pub use node_slots::NodeSlotFactory;
pub use program::{
    HookModuleFailure, HookModulePublicationFailure, HookModulePublicationProblem, ModuleOrigin,
    ModuleProgram, ModuleProjection, PreparedHookModule,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalComponentImport {
    pub source: String,
    pub exported: String,
    pub local: String,
    pub span: Span,
}

pub struct ParsedSourceModule {
    pub component: Component,
    pub imports: Vec<LocalComponentImport>,
}

pub trait ModuleFrontend {
    fn parse_module(&self, source: &str) -> Result<ParsedSourceModule, Diagnostic>;
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ModuleDiagnostic {
    pub file: BuildPath,
    pub diagnostic: Diagnostic,
}

pub fn parse_graph(
    frontend: &dyn ModuleFrontend,
    entry: &BuildPath,
    sources: &BTreeMap<BuildPath, String>,
) -> Result<SourceModuleGraph, ModuleDiagnostic> {
    let fail = |file: &BuildPath, diagnostic| ModuleDiagnostic {
        file: file.clone(),
        diagnostic,
    };
    let graph_error =
        |message: &str| fail(entry, Diagnostic::unsupported(Span::default(), message));
    if sources.is_empty() || sources.len() > 16384 || !sources.contains_key(entry) {
        return Err(graph_error(
            "bounded component inventory and declared entry required",
        ));
    }
    let mut total = 0usize;
    for source in sources.values() {
        total = total
            .checked_add(source.len())
            .ok_or_else(|| graph_error("module source byte budget overflow"))?;
        if source.len() > 16 * 1024 * 1024 || total > 256 * 1024 * 1024 {
            return Err(graph_error("module source byte budget exceeded"));
        }
    }
    let inventory = sources.keys().cloned().collect::<BTreeSet<_>>();
    let mut modules = BTreeMap::new();
    for (path, source) in sources {
        let parsed = frontend
            .parse_module(source)
            .map_err(|diagnostic| fail(path, diagnostic))?;
        let mut imports = BTreeMap::new();
        for import in parsed.imports {
            let target = resolve_local_module(path, &import.source, import.span, &inventory)
                .map_err(|diagnostic| fail(path, diagnostic))?;
            if imports
                .insert(
                    import.local,
                    ComponentImport {
                        target: ComponentReference {
                            module: target,
                            export: import.exported,
                        },
                        span: import.span,
                    },
                )
                .is_some()
            {
                return Err(fail(
                    path,
                    Diagnostic::unsupported(import.span, "duplicate local component binding"),
                ));
            }
        }
        modules.insert(
            path.clone(),
            SourceComponentModule {
                content: ContentHash::of(source.as_bytes()),
                source_bytes: source.len() as u64,
                component: parsed.component,
                imports,
            },
        );
    }
    let export = modules
        .get(entry)
        .ok_or_else(|| graph_error("component entry disappeared"))?
        .component
        .name
        .clone();
    let graph = SourceModuleGraph {
        entry: ComponentReference {
            module: entry.clone(),
            export,
        },
        modules,
    };
    for (file, module) in &graph.modules {
        for import in module.imports.values() {
            graph
                .definition(&import.target)
                .map_err(|message| fail(file, Diagnostic::unsupported(import.span, message)))?;
        }
    }
    graph.validate().map_err(|message| graph_error(&message))?;
    Ok(graph)
}

pub fn resolve_local_module(
    importer: &BuildPath,
    specifier: &str,
    span: Span,
    inventory: &BTreeSet<BuildPath>,
) -> Result<BuildPath, Diagnostic> {
    let invalid = |message: &str| Diagnostic::unsupported(span, message);
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return Err(invalid("local component import must be relative"));
    }
    if specifier.contains(['\\', ':', '?', '#']) || specifier.chars().any(char::is_control) {
        return Err(invalid("local component import has an invalid path"));
    }
    let mut parts = importer.as_str().split('/').collect::<Vec<_>>();
    parts.pop();
    for part in specifier.split('/') {
        match part {
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(invalid("local component import escapes the project root"));
                }
            }
            "" => return Err(invalid("local component import has an empty path segment")),
            value => parts.push(value),
        }
    }
    let normalized = parts.join("/");
    let target: BuildPath = normalized
        .clone()
        .try_into()
        .map_err(|_| invalid("local component import is not a portable path"))?;
    let file = parts
        .last()
        .ok_or_else(|| invalid("local component import has no file"))?;
    let candidates = if file.ends_with(".tsx") {
        vec![target]
    } else if file.contains('.') {
        return Err(invalid(
            "local component modules currently require TSX source",
        ));
    } else {
        [
            format!("{normalized}.tsx"),
            format!("{normalized}/index.tsx"),
        ]
        .into_iter()
        .map(|path| {
            path.try_into()
                .map_err(|_| invalid("invalid component module candidate"))
        })
        .collect::<Result<Vec<BuildPath>, _>>()?
    };
    let mut matches = candidates
        .into_iter()
        .filter(|path| inventory.contains(path));
    let found = matches
        .next()
        .ok_or_else(|| invalid("local component module is absent from the declared inventory"))?;
    if matches.next().is_some() {
        return Err(invalid(
            "local component module is ambiguous; use an explicit TSX path",
        ));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(value: &str) -> BuildPath {
        value.to_owned().try_into().unwrap()
    }

    #[test]
    fn resolves_declared_relative_modules_and_rejects_ambiguous_files() {
        let mut files = BTreeSet::from([
            path("components/window-chrome.tsx"),
            path("pages/viewing.tsx"),
        ]);
        let span = Span { start: 8, end: 42 };
        assert_eq!(
            resolve_local_module(
                &path("pages/viewing.tsx"),
                "../components/window-chrome",
                span,
                &files
            )
            .unwrap(),
            path("components/window-chrome.tsx")
        );
        files.insert(path("components/window-chrome/index.tsx"));
        let error = resolve_local_module(
            &path("pages/viewing.tsx"),
            "../components/window-chrome",
            span,
            &files,
        )
        .unwrap_err();
        assert!(error.message.contains("ambiguous"));
        assert_eq!(error.span, span);
        assert_eq!(
            resolve_local_module(
                &path("pages/viewing.tsx"),
                "../components/window-chrome.tsx",
                span,
                &files
            )
            .unwrap(),
            path("components/window-chrome.tsx")
        );
    }

    #[test]
    fn rejects_escape_unlisted_nonportable_and_external_imports() {
        let files = BTreeSet::from([path("components/window-chrome.tsx")]);
        let importer = path("viewing.tsx");
        let span = Span { start: 0, end: 58 };
        for specifier in [
            "../components/window-chrome",
            "./missing",
            "./components//window-chrome",
            "./components/window-chrome?raw",
            "./components/window-chrome#export",
            "./components\\window-chrome",
            "https://example.com/module.tsx",
            "./components/window-chrome.js",
            "./CON.tsx",
        ] {
            let error = resolve_local_module(&importer, specifier, span, &files).unwrap_err();
            assert_eq!(error.span, span, "{specifier}");
        }
        assert_eq!(
            resolve_local_module(&importer, "./components/window-chrome", span, &files).unwrap(),
            path("components/window-chrome.tsx")
        );
    }
}
