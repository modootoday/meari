//! Verify immutable resource integrity once, before either host receives the bytes.
//! Media decoding and embedded-reference policy remain adapter responsibilities.
use crate::domain::{
    assets::AssetManifest,
    build::{BuildPath, ContentHash},
};
use std::{collections::BTreeMap, sync::Arc};
mod build;

#[derive(Clone)]
pub struct VerifiedAssets {
    manifest: AssetManifest,
    resources: BTreeMap<BuildPath, (String, Arc<[u8]>)>,
}
impl VerifiedAssets {
    pub fn new(
        manifest: AssetManifest,
        blobs: BTreeMap<ContentHash, Vec<u8>>,
    ) -> Result<Self, String> {
        manifest.validate()?;
        let expected = manifest
            .files
            .values()
            .map(|descriptor| (descriptor.content.clone(), descriptor.length))
            .collect::<BTreeMap<_, _>>();
        if expected.keys().ne(blobs.keys()) {
            return Err("asset blob inventory differs from its manifest".into());
        }
        let mut verified = BTreeMap::<ContentHash, Arc<[u8]>>::new();
        for (hash, bytes) in blobs {
            let length = expected.get(&hash).ok_or("asset descriptor missing")?;
            if bytes.len() as u64 != *length || ContentHash::of(&bytes) != hash {
                return Err("asset bytes differ from their declared hash or length".into());
            }
            verified.insert(hash, bytes.into());
        }
        let mut resources = BTreeMap::new();
        for descriptor in manifest.files.values() {
            resources.insert(
                descriptor.output_path()?,
                (
                    descriptor.media_type.clone(),
                    Arc::clone(
                        verified
                            .get(&descriptor.content)
                            .ok_or("verified asset bytes missing")?,
                    ),
                ),
            );
        }
        Ok(Self {
            manifest,
            resources,
        })
    }
    pub fn manifest(&self) -> &AssetManifest {
        &self.manifest
    }
    pub fn resource(&self, path: &BuildPath) -> Option<(&str, &[u8])> {
        self.resources
            .get(path)
            .map(|(media_type, bytes)| (media_type.as_str(), bytes.as_ref()))
    }
}
