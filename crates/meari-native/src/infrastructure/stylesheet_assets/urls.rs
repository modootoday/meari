use super::{CssAssetError, CssAssetReference};
use cssparser::{Parser, Token};
use meari::domain::{Diagnostic, Span, assets::AssetManifest, build::BuildPath};

struct Scanner<'a> {
    file: &'a BuildPath,
    assets: &'a AssetManifest,
    references: Vec<CssAssetReference>,
}
impl Scanner<'_> {
    fn error(&self, span: Span, message: impl Into<String>) -> CssAssetError {
        CssAssetError {
            file: self.file.as_str().to_owned(),
            diagnostic: Diagnostic::unsupported(span, message),
        }
    }
    fn record(&mut self, url: String, span: Span) -> Result<(), CssAssetError> {
        if self.references.len() >= 65536 {
            return Err(self.error(span, "CSS resource reference budget exceeded"));
        }
        let (source, suffix) =
            resolve(self.file, &url).map_err(|message| self.error(span, message))?;
        let descriptor = self.assets.files.get(&source).ok_or_else(|| {
            self.error(
                span,
                format!("CSS asset is not declared: {}", source.as_str()),
            )
        })?;
        let output = descriptor
            .output_path()
            .map_err(|message| self.error(span, message))?;
        self.references.push(CssAssetReference {
            file: self.file.clone(),
            source,
            url: format!("/{}{suffix}", output.as_str()),
            output,
            original_url: url,
            original_span: span,
            generated_span: Span::default(),
        });
        Ok(())
    }
    fn visit(
        &mut self,
        parser: &mut Parser<'_>,
        depth: usize,
        mut import: bool,
        literal_images: bool,
    ) -> Result<(), CssAssetError> {
        if depth > 128 {
            return Err(self.error(Span::default(), "CSS block depth exceeded"));
        }
        while !parser.is_exhausted() {
            let start = parser.position().byte_index();
            let token = parser
                .next_including_whitespace_and_comments()
                .map_err(|error| {
                    self.error(Span { start, end: start }, format!("CSS token: {error:?}"))
                })?
                .clone();
            let span = Span {
                start,
                end: parser.position().byte_index(),
            };
            match token {
                Token::AtKeyword(name)
                    if name.eq_ignore_ascii_case("import")
                        || name.eq_ignore_ascii_case("reference") =>
                {
                    import = true
                }
                Token::Semicolon => import = false,
                Token::BadString(_)
                | Token::BadUrl(_)
                | Token::CloseCurlyBracket
                | Token::CloseSquareBracket
                | Token::CloseParenthesis => {
                    return Err(self.error(span, "malformed CSS token/block"));
                }
                Token::UnquotedUrl(url) if !import => self.record(url.to_string(), span)?,
                Token::QuotedString(url) if !import && literal_images => {
                    self.record(url.to_string(), span)?
                }
                Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                    let url = parser
                        .parse_nested_block(|p| {
                            let value = p.expect_string_cloned()?.to_string();
                            p.expect_exhausted()?;
                            Ok::<_, cssparser::ParseError<()>>(value)
                        })
                        .map_err(|error| self.error(span, format!("CSS url: {error:?}")))?;
                    if !import {
                        self.record(
                            url,
                            Span {
                                start,
                                end: parser.position().byte_index(),
                            },
                        )?;
                    }
                }
                Token::CurlyBracketBlock if import => {
                    return Err(self.error(span, "stylesheet import must end before a rule block"));
                }
                Token::Function(name) => {
                    let images = name.eq_ignore_ascii_case("image-set")
                        || name.eq_ignore_ascii_case("-webkit-image-set");
                    parser
                        .parse_nested_block(|p| {
                            self.visit(p, depth + 1, import, images)
                                .map_err(cssparser::ParseError::<CssAssetError>::custom)
                        })
                        .map_err(|error| match error.kind {
                            cssparser::ParseErrorKind::Custom(error) => error,
                            other => self.error(span, format!("CSS function: {other:?}")),
                        })?;
                }
                Token::CurlyBracketBlock | Token::SquareBracketBlock | Token::ParenthesisBlock => {
                    parser
                        .parse_nested_block(|p| {
                            self.visit(p, depth + 1, import, false)
                                .map_err(cssparser::ParseError::<CssAssetError>::custom)
                        })
                        .map_err(|error| match error.kind {
                            cssparser::ParseErrorKind::Custom(error) => error,
                            other => self.error(span, format!("CSS block: {other:?}")),
                        })?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub(super) fn scan(
    file: &BuildPath,
    css: &str,
    assets: &AssetManifest,
) -> Result<Vec<CssAssetReference>, CssAssetError> {
    let mut scanner = Scanner {
        file,
        assets,
        references: Vec::new(),
    };
    scanner.visit(&mut Parser::new(css), 0, false, false)?;
    Ok(scanner.references)
}

fn resolve(file: &BuildPath, url: &str) -> Result<(BuildPath, String), String> {
    if url.is_empty() || url.len() > 8192 || url.chars().any(char::is_control) {
        return Err("CSS resource URL exceeds its literal budget".into());
    }
    let split = url.find(['?', '#']).unwrap_or(url.len());
    let (path, suffix) = url.split_at(split);
    if suffix.contains('#') {
        return Err(
            "CSS resource fragments require a dedicated SVG/document reference adapter".into(),
        );
    }
    if path.contains([':', '\\', '%']) || path.starts_with("//") {
        return Err(
            "CSS resource scheme, encoded path or external authority is not registered".into(),
        );
    }
    if path.is_empty() {
        return Err("CSS resource requires a declared file path".into());
    }
    let mut parts = if path.starts_with('/') {
        vec![]
    } else {
        let mut parts = file.as_str().split('/').collect::<Vec<_>>();
        parts.pop();
        parts
    };
    for part in path.strip_prefix('/').unwrap_or(path).split('/') {
        match part {
            "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or("CSS resource escapes declared project root")?;
            }
            "" => return Err("CSS resource contains an empty path segment".into()),
            part => parts.push(part),
        }
    }
    let source = BuildPath::try_from(parts.join("/"))?;
    Ok((source, suffix.to_owned()))
}
