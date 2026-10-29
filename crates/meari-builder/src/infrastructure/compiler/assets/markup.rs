//! Lower original markup resource URLs while retaining declaration identity and spans.
use super::super::components::Components;
use meari::{
    application::builder::BuildFailure,
    domain::{
        Component, Diagnostic, Node, NodeKind, Span, Value,
        assets::AssetManifest,
        build::{BuildPath, InputRole},
    },
};

#[derive(serde::Serialize)]
pub(crate) struct MarkupAssetReference {
    file: BuildPath,
    span: Span,
    attribute: String,
    original_url: String,
    source: BuildPath,
    url: String,
}
fn resolve(
    file: &BuildPath,
    url: &str,
    manifest: &AssetManifest,
) -> Result<(BuildPath, String), String> {
    if url.is_empty()
        || url.contains(['\\', '%', '#'])
        || url.chars().any(char::is_control)
        || url.starts_with("//")
        || url.contains(':')
    {
        return Err("markup asset URL requires a declared local resource without encoded paths or fragments".into());
    }
    let (resource, query) = url
        .split_once('?')
        .map_or((url, None), |(resource, query)| (resource, Some(query)));
    if let Some((source, descriptor)) = manifest.files.iter().find(|(_, descriptor)| {
        descriptor
            .output_path()
            .is_ok_and(|path| resource.trim_start_matches('/') == path.as_str())
    }) {
        if !matches!(descriptor.role, InputRole::Icon | InputRole::Image) {
            return Err("markup image URL has a non-image role".into());
        }
        return Ok((source.clone(), url.to_owned()));
    }
    let mut parts = if resource.starts_with('/') {
        Vec::new()
    } else {
        file.as_str().split('/').collect::<Vec<_>>()
    };
    if !resource.starts_with('/') {
        parts.pop();
    }
    for part in resource.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or("markup asset escapes its project root")?;
            }
            part => parts.push(part),
        }
    }
    let source = BuildPath::try_from(parts.join("/"))?;
    let descriptor = manifest
        .files
        .get(&source)
        .ok_or("markup asset is not declared in the input graph")?;
    if !matches!(descriptor.role, InputRole::Icon | InputRole::Image) {
        return Err("markup image URL has a non-image role".into());
    }
    let mut output = format!("/{}", descriptor.output_path()?.as_str());
    if let Some(query) = query {
        output.push('?');
        output.push_str(query);
    }
    Ok((source, output))
}
fn visit(
    component: &mut Component,
    file: &BuildPath,
    manifest: &AssetManifest,
    references: &mut Vec<MarkupAssetReference>,
) -> Result<(), BuildFailure> {
    fn walk(
        node: &mut Node,
        file: &BuildPath,
        manifest: &AssetManifest,
        references: &mut Vec<MarkupAssetReference>,
    ) -> Result<(), BuildFailure> {
        if let NodeKind::Text(Value::Repeat(repeat)) = &mut node.kind {
            walk(&mut repeat.template, file, manifest, references)?;
        }
        if let NodeKind::Element {
            attributes,
            children,
            tag,
        } = &mut node.kind
        {
            for attribute in attributes {
                if !matches!(attribute.name.as_str(), "src" | "poster")
                    || !tag.starts_with(|ch: char| ch.is_ascii_lowercase())
                {
                    continue;
                }
                if let Value::String(url) = &mut attribute.value {
                    let (source, output) = resolve(file, url, manifest).map_err(|message| {
                        BuildFailure::source(
                            file.clone(),
                            Diagnostic::unsupported(node.span, message),
                        )
                    })?;
                    references.push(MarkupAssetReference {
                        file: file.clone(),
                        span: node.span,
                        attribute: attribute.name.clone(),
                        original_url: url.clone(),
                        source,
                        url: output.clone(),
                    });
                    *url = output;
                }
            }
            for child in children {
                walk(child, file, manifest, references)?;
            }
        }
        Ok(())
    }
    walk(&mut component.root, file, manifest, references)
}

pub(in super::super) fn rewrite_components(
    components: &mut Components,
    entry: &BuildPath,
    manifest: &AssetManifest,
) -> Result<Vec<MarkupAssetReference>, BuildFailure> {
    let mut references = Vec::new();
    if let Some(modules) = &mut components.modules {
        for (file, module) in &mut modules.modules.modules {
            visit(&mut module.component, file, manifest, &mut references)?;
        }
        modules.application.component = modules
            .modules
            .definition(&modules.modules.entry)?
            .component
            .clone();
        components.definition = modules.application.clone();
        components.projected = modules
            .program("builder-asset-preflight".into(), 1)?
            .project()
            .map_err(|error| BuildFailure::source(error.file, error.diagnostic))?
            .component
            .clone();
    } else {
        visit(
            &mut components.definition.component,
            entry,
            manifest,
            &mut references,
        )?;
        components.projected = components.definition.bound_component()?;
    }
    Ok(references)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meari::domain::{assets::AssetDescriptor, build::ContentHash};
    use std::collections::BTreeMap;
    #[test]
    fn resolves_original_module_relative_image_and_rejects_escape_missing_and_wrong_role() {
        let file = BuildPath::try_from("views/card.tsx".to_owned()).unwrap();
        let source = BuildPath::try_from("icons/logo.svg".to_owned()).unwrap();
        let descriptor = AssetDescriptor {
            role: InputRole::Icon,
            content: ContentHash::of(b"svg"),
            length: 3,
            media_type: "image/svg+xml".into(),
            license: Some("MIT".into()),
        };
        let expected = format!("/{}?v=1", descriptor.output_path().unwrap().as_str());
        let manifest = AssetManifest {
            schema_version: 1,
            files: BTreeMap::from([(source.clone(), descriptor)]),
        };
        assert_eq!(
            resolve(&file, "../icons/logo.svg?v=1", &manifest).unwrap(),
            (source, expected)
        );
        for url in [
            "../../outside.svg",
            "../icons/missing.svg",
            "https://example.com/logo.svg",
            "data:image/svg+xml,x",
            "../icons/%6cogo.svg",
            "../icons/logo.svg#part",
        ] {
            assert!(resolve(&file, url, &manifest).is_err(), "{url}");
        }
    }
}
