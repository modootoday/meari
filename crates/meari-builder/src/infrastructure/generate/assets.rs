//! Host-specific transport of the same verified resource inventory.
use super::path;
use meari::{
    application::assets::VerifiedAssets,
    domain::build::{BuildPath, TargetKind},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

pub(super) fn files(
    assets: Option<&VerifiedAssets>,
    target: TargetKind,
) -> Result<BTreeMap<BuildPath, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let Some(assets) = assets else {
        files.insert(path("src/assets.rs"),match target{
            TargetKind::Native=>"pub(super) fn load()->Result<Option<meari::application::assets::VerifiedAssets>,String>{Ok(None)}\n",
            TargetKind::Web=>"pub(super) fn validate()->Result<(),String>{Ok(())}\n",
        }.as_bytes().to_vec());
        return Ok(files);
    };
    files.insert(
        path("asset-manifest.json"),
        serde_json::to_vec(assets.manifest()).map_err(|error| error.to_string())?,
    );
    let mut includes = String::new();
    let mut seen = BTreeSet::new();
    for descriptor in assets.manifest().files.values() {
        let output = descriptor.output_path()?;
        if !seen.insert(output.clone()) {
            continue;
        }
        let (_, bytes) = assets
            .resource(&output)
            .ok_or("generated resource missing")?;
        files.insert(output.clone(), bytes.to_vec());
        writeln!(includes,"(meari::domain::build::ContentHash::try_from({:?}.to_owned())?,include_bytes!({:?}).to_vec()),",
            descriptor.content.as_str(),format!("../{}",output.as_str())).map_err(|error|error.to_string())?;
    }
    let source=match target{
        TargetKind::Native=>format!(r#"pub(super) fn load()->Result<Option<meari::application::assets::VerifiedAssets>,String>{{
    let manifest:meari::domain::assets::AssetManifest=serde_json::from_str(include_str!("../asset-manifest.json")).map_err(|error|error.to_string())?;
    let blobs:std::collections::BTreeMap<meari::domain::build::ContentHash,Vec<u8>>=std::collections::BTreeMap::from([{includes}]);
    meari::application::assets::VerifiedAssets::new(manifest,blobs).map(Some)
}}
"#),
        TargetKind::Web=>r#"pub(super) fn validate()->Result<(),String>{
    let manifest:meari::domain::assets::AssetManifest=serde_json::from_str(include_str!("../asset-manifest.json")).map_err(|error|error.to_string())?;
    manifest.validate()
}
"#.into(),
    };
    files.insert(path("src/assets.rs"), source.into_bytes());
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meari::domain::{
        assets::{AssetDescriptor, AssetManifest},
        build::{ContentHash, InputRole},
    };

    #[test]
    fn both_targets_emit_identical_static_bytes_but_only_native_embeds_them() {
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec();
        let hash = ContentHash::of(&bytes);
        let descriptor = AssetDescriptor {
            role: InputRole::Icon,
            content: hash.clone(),
            length: bytes.len() as u64,
            media_type: "image/svg+xml".into(),
            license: Some("MIT".into()),
        };
        let output = descriptor.output_path().unwrap();
        let resources = VerifiedAssets::new(
            AssetManifest {
                schema_version: 1,
                files: BTreeMap::from([(path("logo.svg"), descriptor)]),
            },
            BTreeMap::from([(hash, bytes.clone())]),
        )
        .unwrap();
        let native = files(Some(&resources), TargetKind::Native).unwrap();
        let web = files(Some(&resources), TargetKind::Web).unwrap();
        assert_eq!(native[&output], bytes);
        assert_eq!(web[&output], bytes);
        assert_eq!(
            native[&path("asset-manifest.json")],
            web[&path("asset-manifest.json")]
        );
        assert!(
            std::str::from_utf8(&native[&path("src/assets.rs")])
                .unwrap()
                .contains("include_bytes!")
        );
        assert!(
            !std::str::from_utf8(&web[&path("src/assets.rs")])
                .unwrap()
                .contains("include_bytes!")
        );
    }
}
