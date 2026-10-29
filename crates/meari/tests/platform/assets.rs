use meari::{
    application::assets::VerifiedAssets,
    domain::{
        assets::{AssetDescriptor, AssetManifest, MAX_ASSET_BYTES},
        build::{BuildPath, ContentHash, InputRole},
    },
};
use std::collections::BTreeMap;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
fn fixture() -> (AssetManifest, BTreeMap<ContentHash, Vec<u8>>) {
    let bytes = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M0 0L24 24\"/></svg>".to_vec();
    let hash = ContentHash::of(&bytes);
    let descriptor = AssetDescriptor {
        role: InputRole::Icon,
        content: hash.clone(),
        length: bytes.len() as u64,
        media_type: "image/svg+xml".into(),
        license: Some("MIT OR Apache-2.0".into()),
    };
    (
        AssetManifest {
            schema_version: 1,
            files: BTreeMap::from([(path("assets/icons/control.svg"), descriptor)]),
        },
        BTreeMap::from([(hash, bytes)]),
    )
}

#[test]
fn verified_assets_reject_changed_missing_extra_or_short_bytes() {
    let (manifest, blobs) = fixture();
    let hash = blobs.keys().next().unwrap().clone();
    assert!(VerifiedAssets::new(manifest.clone(), BTreeMap::new()).is_err());
    let mut extra = blobs.clone();
    extra.insert(ContentHash::of(b"unlisted"), b"unlisted".to_vec());
    assert!(VerifiedAssets::new(manifest.clone(), extra).is_err());
    let mut corrupt = blobs.clone();
    corrupt.get_mut(&hash).unwrap()[0] = b'!';
    assert!(VerifiedAssets::new(manifest.clone(), corrupt).is_err());
    let mut short = blobs.clone();
    short.get_mut(&hash).unwrap().pop();
    assert!(VerifiedAssets::new(manifest.clone(), short).is_err());
    let output = manifest
        .files
        .values()
        .next()
        .unwrap()
        .output_path()
        .unwrap();
    let assets = VerifiedAssets::new(manifest, blobs.clone()).unwrap();
    assert_eq!(
        assets.resource(&output).unwrap(),
        ("image/svg+xml", blobs[&hash].as_slice())
    );
    assert!(assets.resource(&path("assets/undeclared.svg")).is_none());
}

#[test]
fn aliases_share_content_while_manifest_identity_retains_source_and_license() {
    let (mut manifest, blobs) = fixture();
    let original = manifest.fingerprint().unwrap();
    let descriptor = manifest.files.values().next().unwrap().clone();
    manifest
        .files
        .insert(path("assets/icons/alias.svg"), descriptor.clone());
    assert_ne!(original, manifest.fingerprint().unwrap());
    let output = descriptor.output_path().unwrap();
    let assets = VerifiedAssets::new(manifest.clone(), blobs).unwrap();
    let clone = assets.clone();
    assert!(std::ptr::eq(
        assets.resource(&output).unwrap().1,
        clone.resource(&output).unwrap().1
    ));
    assert_eq!(assets.manifest().files.len(), 2);
    let before = manifest.fingerprint().unwrap();
    manifest.files.values_mut().next().unwrap().license = None;
    assert_ne!(before, manifest.fingerprint().unwrap());
}

#[test]
fn manifest_rejects_role_type_conflicts_case_collisions_and_declared_byte_exhaustion() {
    let (manifest, _) = fixture();
    let descriptor = manifest.files.values().next().unwrap().clone();
    let mut invalid = manifest.clone();
    invalid.schema_version = 2;
    assert!(invalid.validate().is_err());
    let mut invalid = manifest.clone();
    invalid.files.values_mut().next().unwrap().role = InputRole::Component;
    assert!(invalid.validate().is_err());
    let mut invalid = manifest.clone();
    invalid.files.values_mut().next().unwrap().media_type = "font/ttf".into();
    assert!(invalid.validate().is_err());
    let mut invalid = manifest.clone();
    invalid
        .files
        .insert(path("ASSETS/ICONS/CONTROL.SVG"), descriptor.clone());
    assert!(invalid.validate().is_err());
    let mut invalid = manifest.clone();
    let mut conflict = descriptor.clone();
    conflict.length += 1;
    invalid.files.insert(path("other.svg"), conflict);
    assert!(invalid.validate().is_err());
    let mut invalid = manifest;
    invalid.files.clear();
    for index in 0..5 {
        let mut large = descriptor.clone();
        large.content = ContentHash::of(&[index]);
        large.length = MAX_ASSET_BYTES;
        invalid.files.insert(path(&format!("{index}.svg")), large);
    }
    assert!(invalid.validate().is_err());
}
