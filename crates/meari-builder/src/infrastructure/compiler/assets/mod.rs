//! Declared input resources and per-file CSS provenance, shared by both target factories.
use meari::{
    application::{assets::VerifiedAssets, builder::BuildFailure},
    domain::{
        assets::{AssetDescriptor, AssetManifest},
        build::*,
        design::stylesheet::StylesheetProject,
    },
};
use meari_native::infrastructure::stylesheet_assets::{self, CssAssetReference};
use std::collections::BTreeMap;
mod markup;
pub(super) use markup::rewrite_components;

pub(super) struct Assets {
    pub verified: Option<VerifiedAssets>,
    pub project: StylesheetProject,
    pub references: Vec<CssAssetReference>,
    pub originals: BTreeMap<BuildPath, Vec<u8>>,
    pub mappings: Vec<BuildMapping>,
}

pub(super) fn prepare(
    graph: &BuildGraph,
    inputs: &BTreeMap<BuildPath, Vec<u8>>,
    project: StylesheetProject,
) -> Result<Assets, BuildFailure> {
    let mut manifest = AssetManifest {
        schema_version: 1,
        files: BTreeMap::new(),
    };
    let mut blobs = BTreeMap::new();
    let mut originals = BTreeMap::new();
    let mut mappings = Vec::new();
    for source in project.files.keys() {
        let path = BuildPath::try_from(source.clone())?;
        if graph
            .files
            .get(&path)
            .is_none_or(|input| input.role != InputRole::Stylesheet)
        {
            return Err(BuildFailure::file(
                path,
                "stylesheet is not declared in the input graph".into(),
            ));
        }
    }
    for (path, input) in &graph.files {
        if input.role == InputRole::Stylesheet {
            let bytes = inputs.get(path).ok_or("stylesheet input missing")?;
            if input.length != bytes.len() as u64
                || input.content != ContentHash::of(bytes)
                || project.files.get(path.as_str()).map(String::as_bytes) != Some(bytes.as_slice())
            {
                return Err(BuildFailure::file(
                    path.clone(),
                    "stylesheet project differs from original input graph".into(),
                ));
            }
            let original = BuildPath::try_from(format!("source/original/{}", path.as_str()))?;
            let span = meari::domain::Span {
                start: 0,
                end: bytes.len(),
            };
            mappings.push(BuildMapping {
                source: path.clone(),
                generated: original.clone(),
                source_span: span,
                generated_span: span,
            });
            originals.insert(original, bytes.clone());
        }
        if !matches!(
            input.role,
            InputRole::Icon | InputRole::Font | InputRole::Image
        ) {
            continue;
        }
        let bytes = inputs
            .get(path)
            .ok_or_else(|| BuildFailure::file(path.clone(), "asset bytes missing".into()))?;
        if bytes.len() as u64 != input.length || ContentHash::of(bytes) != input.content {
            return Err(BuildFailure::file(
                path.clone(),
                "asset bytes differ from input graph".into(),
            ));
        }
        let media_type =
            media_type(path).map_err(|message| BuildFailure::file(path.clone(), message))?;
        let descriptor = AssetDescriptor {
            role: input.role.clone(),
            content: input.content.clone(),
            length: input.length,
            media_type: media_type.into(),
            license: input.license.clone(),
        };
        descriptor
            .validate()
            .map_err(|message| BuildFailure::file(path.clone(), message))?;
        manifest.files.insert(path.clone(), descriptor);
        blobs
            .entry(input.content.clone())
            .or_insert_with(|| bytes.clone());
    }
    let output =
        stylesheet_assets::rewrite(&project, &manifest).map_err(
            |error| match BuildPath::try_from(error.file.clone()) {
                Ok(path) => BuildFailure::source(path, error.diagnostic),
                Err(_) => BuildFailure::from(error.to_string()),
            },
        )?;
    let verified = if manifest.files.is_empty() {
        None
    } else {
        Some(VerifiedAssets::new(manifest, blobs)?)
    };
    for (file, css) in &output.project.files {
        originals.insert(
            BuildPath::try_from(format!("source/styles/{file}"))?,
            css.as_bytes().to_vec(),
        );
    }
    for reference in &output.references {
        mappings.push(BuildMapping {
            source: reference.file.clone(),
            generated: BuildPath::try_from(format!("source/styles/{}", reference.file.as_str()))?,
            source_span: reference.original_span,
            generated_span: reference.generated_span,
        });
    }
    Ok(Assets {
        verified,
        project: output.project,
        references: output.references,
        originals,
        mappings,
    })
}

fn media_type(path: &BuildPath) -> Result<&'static str, String> {
    match path
        .as_str()
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => Ok("image/svg+xml"),
        Some("png") => Ok("image/png"),
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("webp") => Ok("image/webp"),
        Some("gif") => Ok("image/gif"),
        Some("ttf") => Ok("font/ttf"),
        Some("otf") => Ok("font/otf"),
        Some("woff") => Ok("font/woff"),
        Some("woff2") => Ok("font/woff2"),
        _ => Err("asset extension has no registered media type".into()),
    }
}

#[cfg(test)]
mod tests;
