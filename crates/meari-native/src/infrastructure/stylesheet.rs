//! CSS token/dependency adapter. Original files are never rewritten.
use crate::application::{CompiledStylesheet, StylesheetProjectCompiler};
use cssparser::{Parser, Token};
use meari::domain::design::{
    stylesheet::{StylesheetMode, StylesheetProject},
    theme::Theme,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Debug)]
pub(super) struct Import {
    pub start: usize,
    pub end: usize,
    pub target: String,
    pub conditional: bool,
}

fn target(parser: &mut Parser<'_>) -> Result<String, String> {
    match parser
        .next()
        .map_err(|e| format!("CSS import: {e:?}"))?
        .clone()
    {
        Token::QuotedString(s) | Token::UnquotedUrl(s) => Ok(s.to_string()),
        Token::Function(s) if s.eq_ignore_ascii_case("url") => parser
            .parse_nested_block(|p| {
                p.expect_string_cloned()
                    .map(|s| s.to_string())
                    .map_err(Into::<cssparser::ParseError<()>>::into)
            })
            .map_err(|e| format!("CSS url: {e:?}")),
        _ => Err("CSS import/reference requires a literal URL".into()),
    }
}

pub(super) fn imports(css: &str, mode: StylesheetMode) -> Result<Vec<Import>, String> {
    fn visit(
        parser: &mut Parser<'_>,
        mode: StylesheetMode,
        found: &mut Vec<Import>,
    ) -> Result<(), String> {
        while !parser.is_exhausted() {
            let start = parser.position();
            let token = parser
                .next_including_whitespace_and_comments()
                .map_err(|e| format!("CSS token: {e:?}"))?
                .clone();
            match token {
                Token::BadString(_)
                | Token::BadUrl(_)
                | Token::CloseCurlyBracket
                | Token::CloseSquareBracket
                | Token::CloseParenthesis => return Err("malformed CSS token/block".into()),
                Token::AtKeyword(name) => {
                    let name = name.to_ascii_lowercase();
                    if matches!(name.as_str(), "plugin" | "config") {
                        return Err(format!(
                            "@{name} executes JavaScript; requires a separate trusted-project adapter"
                        ));
                    }
                    if name == "source" {
                        let mut token = parser
                            .next()
                            .map_err(|e| format!("@source: {e:?}"))?
                            .clone();
                        if matches!(&token, Token::Ident(s) if s.eq_ignore_ascii_case("not")) {
                            token = parser
                                .next()
                                .map_err(|e| format!("@source not: {e:?}"))?
                                .clone();
                        }
                        if !matches!(&token, Token::Function(s) if s.eq_ignore_ascii_case("inline"))
                        {
                            return Err("@source filesystem scanning requires a trusted-project adapter; use inline()".into());
                        }
                    }
                    if mode == StylesheetMode::Css
                        && matches!(
                            name.as_str(),
                            "theme"
                                | "utility"
                                | "custom-variant"
                                | "variant"
                                | "apply"
                                | "reference"
                                | "source"
                        )
                    {
                        return Err(format!("@{name} requires Tailwind mode"));
                    }
                    if matches!(name.as_str(), "import" | "reference") {
                        let target = target(parser)?;
                        let tail = parser.position();
                        let mut tail_end = tail;
                        while !parser.is_exhausted() {
                            let before = parser.position();
                            let token = parser
                                .next_including_whitespace_and_comments()
                                .map_err(|e| format!("CSS import tail: {e:?}"))?
                                .clone();
                            if token == Token::Semicolon {
                                tail_end = before;
                                break;
                            }
                            if let Token::Function(name) = token
                                && name.eq_ignore_ascii_case("source")
                            {
                                parser
                                    .parse_nested_block(|p| {
                                        p.expect_ident_matching("none")?;
                                        p.expect_exhausted()?;
                                        Ok::<_, cssparser::ParseError<()>>(())
                                    })
                                    .map_err(|_| {
                                        "import source() must be source(none) in a portable bundle"
                                            .to_string()
                                    })?;
                            }
                            tail_end = parser.position();
                        }
                        found.push(Import {
                            start: start.byte_index(),
                            end: parser.position().byte_index(),
                            target,
                            conditional: !parser.slice(tail..tail_end).trim().is_empty(),
                        });
                    }
                }
                Token::CurlyBracketBlock
                | Token::SquareBracketBlock
                | Token::ParenthesisBlock
                | Token::Function(_) => {
                    parser
                        .parse_nested_block(|p| {
                            visit(p, mode, found).map_err(cssparser::ParseError::<String>::custom)
                        })
                        .map_err(|e| format!("CSS block: {e:?}"))?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut parser = Parser::new(css);
    let mut found = vec![];
    visit(&mut parser, mode, &mut found)?;
    Ok(found)
}

pub(super) fn resolve(file: &str, target: &str) -> Result<String, String> {
    if target.contains([':', '\\', '?', '#', '\0']) || target.starts_with('/') {
        return Err(format!(
            "external/absolute stylesheet dependency is not in portable bundle: {target}"
        ));
    }
    let mut parts: Vec<_> = file.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("stylesheet dependency escapes bundle".into());
                }
            }
            "" => return Err("empty stylesheet dependency path segment".into()),
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

pub(super) fn validate(project: &StylesheetProject) -> Result<(), String> {
    project.validate()?;
    fn visit(
        project: &StylesheetProject,
        file: &str,
        stack: &mut BTreeSet<String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if stack.len() >= 128 || !stack.insert(file.into()) {
            return Err(format!("stylesheet dependency cycle/depth: {file}"));
        }
        let css = project
            .files
            .get(file)
            .ok_or_else(|| format!("missing stylesheet file: {file}"))?;
        for import in imports(css, project.mode).map_err(|e| format!("{file}: {e}"))? {
            if project.mode == StylesheetMode::Tailwind
                && matches!(
                    import.target.as_str(),
                    "tailwindcss"
                        | "tailwindcss/index.css"
                        | "tailwindcss/theme.css"
                        | "tailwindcss/preflight.css"
                        | "tailwindcss/utilities.css"
                )
            {
                continue;
            }
            let target = resolve(file, &import.target)?;
            if !seen.contains(&target) {
                visit(project, &target, stack, seen)?;
            }
        }
        stack.remove(file);
        seen.insert(file.into());
        Ok(())
    }
    let mut seen = BTreeSet::new();
    for file in project.files.keys() {
        visit(project, file, &mut BTreeSet::new(), &mut seen)?;
    }
    Ok(())
}

pub(super) fn receipt(
    project: &StylesheetProject,
    css: String,
    compiler: &str,
) -> CompiledStylesheet {
    CompiledStylesheet {
        source_sha256: project
            .files
            .iter()
            .map(|(name, source)| {
                (
                    name.clone(),
                    format!("{:x}", Sha256::digest(source.as_bytes())),
                )
            })
            .collect(),
        css_sha256: format!("{:x}", Sha256::digest(css.as_bytes())),
        css,
        original: project.clone(),
        compiler: compiler.into(),
        native_full_acceptance: false,
    }
}

pub struct DirectCssCompiler;
impl StylesheetProjectCompiler for DirectCssCompiler {
    fn compile_project(
        &self,
        project: &StylesheetProject,
        _classes: &str,
        _theme: &Theme,
    ) -> Result<CompiledStylesheet, String> {
        if project.mode != StylesheetMode::Css {
            return Err("direct CSS compiler requires css mode".into());
        }
        validate(project)?;
        fn bundle(project: &StylesheetProject, file: &str) -> Result<String, String> {
            let source = &project.files[file];
            let mut result = String::new();
            let mut position = 0;
            for import in imports(source, project.mode)? {
                if import.conditional {
                    return Err(
                        "conditional/layered direct CSS imports need Tailwind bundling mode".into(),
                    );
                }
                result.push_str(&source[position..import.start]);
                result.push_str(&bundle(project, &resolve(file, &import.target)?)?);
                result.push('\n');
                position = import.end;
            }
            result.push_str(&source[position..]);
            Ok(result)
        }
        Ok(receipt(
            project,
            bundle(project, &project.entry)?,
            "direct-css/cssparser-0.38.0",
        ))
    }
}
