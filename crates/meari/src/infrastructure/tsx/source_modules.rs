//! Inspect local declarations without deleting imports or executing a JavaScript module.
use super::{err, location};
use crate::application::source_modules::LocalComponentImport;
use crate::domain::{Diagnostic, Span};
use oxc_allocator::Allocator;
use oxc_ast::ast::{ImportDeclarationSpecifier, ImportOrExportKind, ModuleExportName, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::BTreeSet;

pub(crate) fn local_imports(source: &str) -> Result<Vec<LocalComponentImport>, Diagnostic> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Diagnostic::unsupported(
            Span::default(),
            "component module exceeds source budget",
        ));
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(Diagnostic {
            code: "syntax".into(),
            message: error.to_string(),
            span: error
                .labels
                .first()
                .map_or_else(Span::default, |label| Span {
                    start: label.offset() as usize,
                    end: (label.offset() + label.len()) as usize,
                }),
        });
    }
    let mut bindings = BTreeSet::new();
    let mut result = Vec::new();
    for statement in &parsed.program.body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        let module = import.source.value.as_str();
        if !module.starts_with("./") && !module.starts_with("../") {
            continue;
        }
        if import.phase.is_some()
            || import.with_clause.is_some()
            || import.import_kind != ImportOrExportKind::Value
        {
            return Err(err(
                import.as_ref(),
                "plain local component value imports required",
            ));
        }
        let specifiers = import
            .specifiers
            .as_ref()
            .filter(|specifiers| !specifiers.is_empty())
            .ok_or_else(|| err(import.as_ref(), "local side-effect imports are unsupported"))?;
        for specifier in specifiers {
            let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
                return Err(err(
                    import.as_ref(),
                    "local components require named imports",
                ));
            };
            let ModuleExportName::IdentifierName(exported) = &specifier.imported else {
                return Err(err(
                    import.as_ref(),
                    "local component export must be an identifier",
                ));
            };
            if specifier.import_kind != ImportOrExportKind::Value
                || !bindings.insert(specifier.local.name.to_string())
            {
                return Err(err(
                    import.as_ref(),
                    "type-only or duplicate local component binding",
                ));
            }
            result.push(LocalComponentImport {
                source: module.into(),
                exported: exported.name.to_string(),
                local: specifier.local.name.to_string(),
                span: location(import.as_ref()),
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_source_named_export_alias_and_original_utf8_import_range() {
        let source =
            "// 한글\nimport { WindowChrome as Chrome } from './components/window-chrome';\n";
        let imports = local_imports(source).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].source, "./components/window-chrome");
        assert_eq!(imports[0].exported, "WindowChrome");
        assert_eq!(imports[0].local, "Chrome");
        assert_eq!(
            &source[imports[0].span.start..imports[0].span.end],
            "import { WindowChrome as Chrome } from './components/window-chrome';"
        );
    }

    #[test]
    fn rejects_local_side_effect_default_namespace_and_type_only_imports() {
        for source in [
            "import './style.css';",
            "import Chrome from './chrome';",
            "import * as Chrome from './chrome';",
            "import type { Chrome } from './chrome';",
            "import { type Chrome } from './chrome';",
        ] {
            let error = local_imports(source).unwrap_err();
            assert!(error.span.end > error.span.start, "{source}");
            assert!(error.span.end <= source.len(), "{source}");
        }
    }
}
