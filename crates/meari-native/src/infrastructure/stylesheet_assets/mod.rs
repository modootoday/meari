//! Preserve stylesheet origins while translating declared local resource URLs.
use meari::domain::{
    Diagnostic, Span, assets::AssetManifest, build::BuildPath,
    design::stylesheet::StylesheetProject,
};
mod urls;

#[derive(Clone, Debug, serde::Serialize)]
pub struct CssAssetReference {
    pub file: BuildPath,
    pub source: BuildPath,
    pub output: BuildPath,
    pub original_url: String,
    pub url: String,
    pub original_span: Span,
    pub generated_span: Span,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CssAssetError {
    pub file: String,
    pub diagnostic: Diagnostic,
}
impl std::fmt::Display for CssAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}..{}: {}",
            self.file,
            self.diagnostic.span.start,
            self.diagnostic.span.end,
            self.diagnostic.message
        )
    }
}
impl std::error::Error for CssAssetError {}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AssetStylesheet {
    pub project: StylesheetProject,
    pub references: Vec<CssAssetReference>,
}

/// This rewrites resource references, not stylesheet imports or the original input.
/// Format decoding and resource readiness are separate host-adapter operations.
pub fn rewrite(
    project: &StylesheetProject,
    assets: &AssetManifest,
) -> Result<AssetStylesheet, CssAssetError> {
    let fail = |message| CssAssetError {
        file: project.entry.clone(),
        diagnostic: Diagnostic::unsupported(Span::default(), message),
    };
    project.validate().map_err(fail)?;
    assets.validate().map_err(fail)?;
    if project.files.len() > 16384 {
        return Err(fail("stylesheet file budget exceeded".into()));
    }
    let mut total = 0usize;
    for css in project.files.values() {
        total = total
            .checked_add(css.len())
            .ok_or_else(|| fail("stylesheet byte budget overflow".into()))?;
        if css.len() > 16 * 1024 * 1024 || total > 256 * 1024 * 1024 {
            return Err(fail("stylesheet byte budget exceeded".into()));
        }
    }
    let mut rewritten = project.clone();
    let mut references = Vec::new();
    for (name, css) in &project.files {
        let name = BuildPath::try_from(name.clone()).map_err(|message| CssAssetError {
            file: name.clone(),
            diagnostic: Diagnostic::unsupported(Span::default(), message),
        })?;
        let mut found = urls::scan(&name, css, assets)?;
        if references.len().saturating_add(found.len()) > 65536 {
            return Err(fail("stylesheet resource reference budget exceeded".into()));
        }
        let mut output = String::with_capacity(css.len());
        let mut cursor = 0;
        for reference in &mut found {
            output.push_str(&css[cursor..reference.original_span.start]);
            let start = output.len();
            output.push_str("url(");
            cssparser::serialize_string(&reference.url, &mut output)
                .map_err(|_| fail("could not serialize resource URL".into()))?;
            output.push(')');
            reference.generated_span = Span {
                start,
                end: output.len(),
            };
            cursor = reference.original_span.end;
        }
        output.push_str(&css[cursor..]);
        rewritten.files.insert(name.as_str().to_owned(), output);
        references.extend(found);
    }
    Ok(AssetStylesheet {
        project: rewritten,
        references,
    })
}

#[cfg(test)]
mod tests;
