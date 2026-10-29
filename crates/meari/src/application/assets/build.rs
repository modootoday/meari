use super::VerifiedAssets;
use crate::{
    application::builder::BuildBundle,
    domain::{
        assets::AssetManifest,
        build::{BuildPath, ContentHash, InputRole, OutputRole, TargetKind},
    },
};
use std::collections::{BTreeMap, BTreeSet};

impl BuildBundle {
    /// Load actual target resource bytes and verify their declared source provenance.
    pub fn verified_assets(&self) -> Result<Option<VerifiedAssets>, String> {
        self.validate()?;
        let declared = self
            .manifest
            .graph
            .files
            .iter()
            .filter(|(_, input)| {
                matches!(
                    input.role,
                    InputRole::Icon | InputRole::Font | InputRole::Image
                )
            })
            .map(|(path, input)| (path.clone(), input))
            .collect::<BTreeMap<_, _>>();
        let path = BuildPath::try_from("runtime/asset-manifest.json".to_owned())?;
        let Some(bytes) = self.files.get(&path) else {
            return if declared.is_empty() {
                Ok(None)
            } else {
                Err("bundle resource manifest missing".into())
            };
        };
        let manifest: AssetManifest =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        manifest.validate()?;
        if let Some(bytes) = self
            .files
            .get(&BuildPath::try_from("asset-manifest.json".to_owned())?)
        {
            let compiled: AssetManifest =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            if compiled != manifest {
                return Err("compiled and runtime asset manifests differ".into());
            }
        }
        if manifest.files.len() != declared.len() {
            return Err("bundle asset source inventory differs from input graph".into());
        }
        let mut blobs = BTreeMap::<ContentHash, Vec<u8>>::new();
        let mut deployed_paths = BTreeSet::new();
        for (source, descriptor) in &manifest.files {
            let input = declared
                .get(source)
                .ok_or("bundle asset has no declared source")?;
            if input.role != descriptor.role
                || input.content != descriptor.content
                || input.length != descriptor.length
                || input.license != descriptor.license
            {
                return Err("bundle asset identity differs from original input graph".into());
            }
            let logical = descriptor.output_path()?;
            let deployed = match self.manifest.target.kind {
                TargetKind::Native => logical,
                TargetKind::Web => BuildPath::try_from(format!("web/{}", logical.as_str()))?,
            };
            let output = self
                .manifest
                .outputs
                .get(&deployed)
                .ok_or("bundle deployed asset missing")?;
            if output.role != OutputRole::Asset
                || output.content != descriptor.content
                || output.length != descriptor.length
            {
                return Err(
                    "bundle deployed asset identity differs from its resource manifest".into(),
                );
            }
            let bytes = self
                .files
                .get(&deployed)
                .ok_or("bundle deployed asset bytes missing")?;
            blobs
                .entry(descriptor.content.clone())
                .or_insert_with(|| bytes.clone());
            deployed_paths.insert(deployed);
        }
        let prefix = match self.manifest.target.kind {
            TargetKind::Native => "assets/",
            TargetKind::Web => "web/assets/",
        };
        if self
            .manifest
            .outputs
            .keys()
            .any(|path| path.as_str().starts_with(prefix) && !deployed_paths.contains(path))
        {
            return Err("bundle contains an unlisted deployed resource".into());
        }
        VerifiedAssets::new(manifest, blobs).map(Some)
    }
}
