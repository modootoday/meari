//! Explicit AST-span edits, preserving JSX text, string literals and runtime expressions.
use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Function, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

pub(super) fn adapt(code: &str) -> Result<String, String> {
    if code.trim_start().starts_with('<') {
        return Ok(format!(
            "type Props={{}};export function KitchenExample(props:Props){{return ({code});}}"
        ));
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, code, SourceType::tsx()).parse();
    if !parsed.diagnostics.is_empty() {
        return Ok(code.into());
    }
    let mut edits: Vec<(usize, usize, String)> = vec![];
    let mut needs_props = false;
    let mut has_props = false;
    fn parameters(
        function: &Function<'_>,
        edits: &mut Vec<(usize, usize, String)>,
        needs_props: &mut bool,
    ) {
        if function.params.items.is_empty() && function.params.rest.is_none() {
            edits.push((
                function.params.span.start as usize,
                function.params.span.end as usize,
                "(props: Props)".into(),
            ));
            *needs_props = true;
        }
    }
    for statement in &parsed.program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
                if let Some(relative) = import.source.value.strip_prefix("@/styles/")
                    && let Some((style, module)) = relative.split_once("/ui/")
                    && ["base-", "aria-", "radix-"]
                        .iter()
                        .any(|prefix| style.starts_with(prefix))
                {
                    edits.push((
                        import.source.span.start as usize,
                        import.source.span.end as usize,
                        serde_json::to_string(&format!("@/components/ui/{module}"))
                            .map_err(|e| e.to_string())?,
                    ));
                }
            }
            Statement::TSTypeAliasDeclaration(alias) if alias.id.name == "Props" => {
                has_props = true
            }
            Statement::ExportDeclaration(export) => {
                if let Declaration::FunctionDeclaration(function) = &export.declaration {
                    parameters(function, &mut edits, &mut needs_props);
                }
            }
            Statement::ExportDefaultDeclaration(export) => {
                if let ExportDefaultDeclarationKind::FunctionDeclaration(function) =
                    &export.declaration
                {
                    edits.push((
                        export.span.start as usize,
                        function.span.start as usize,
                        "export ".into(),
                    ));
                    parameters(function, &mut edits, &mut needs_props);
                }
            }
            _ => {}
        }
    }
    if needs_props && !has_props {
        let position = parsed
            .program
            .directives
            .last()
            .map_or(0, |directive| directive.span.end as usize);
        edits.push((position, position, "\ntype Props = {};\n".into()));
    }
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    let mut result = code.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn edits_only_import_and_function_syntax_not_visible_source_examples() {
        let code = r#"import { Button } from "@/styles/base-nova/ui/button";
export default function Example() { return <Button title="export default function Text()">{"@/styles/base-nova/ui/button"}</Button>; }"#;
        let adapted = super::adapt(code).unwrap();
        assert!(adapted.contains("export function Example(props: Props)"));
        assert!(adapted.contains("title=\"export default function Text()\""));
        assert!(adapted.contains("{\"@/styles/base-nova/ui/button\"}"));
        assert!(
            meari::parse(&adapted, meari::application::Language::Tsx).is_ok(),
            "{adapted}"
        );
    }
    #[test]
    fn directives_hooks_and_invalid_reference_snippets_are_preserved() {
        let code = "\"use client\"; export default function Example(){const [x,setX]=useState(false);return <div onClick={()=>setX(!x)}>{x}</div>}";
        let adapted = super::adapt(code).unwrap();
        assert!(adapted.starts_with("\"use client\";"));
        assert!(adapted.contains("const [x,setX]=useState(false)"));
        assert!(adapted.contains("onClick={()=>setX(!x)}"));
        assert!(meari::parse(&adapted, meari::application::Language::Tsx).is_err());
        let invalid = "export const columns = [";
        assert_eq!(super::adapt(invalid).unwrap(), invalid);
    }
}
