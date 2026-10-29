use super::*;
use crate::domain::design::icons::catalog::MAPPINGS;
fn license() -> Vec<u8> {
    include_bytes!("../../../../assets/icons/catalogs/lucide/LICENSE").to_vec()
}
fn lock(role: IconRole, bytes: &[u8]) -> CatalogLock {
    CatalogLock {
        version: LUCIDE.version.into(),
        revision: LUCIDE.revision.into(),
        entries: vec![IconLock {
            role,
            source_sha256: ContentHash::of(bytes),
        }],
    }
}
#[test]
fn proposed_roles_are_unique_and_keep_package_ownership_explicit() {
    let roles: std::collections::BTreeSet<_> = MAPPINGS
        .iter()
        .map(|mapping| mapping.semantic_name)
        .collect();
    assert_eq!(roles.len(), MAPPINGS.len());
    assert_eq!(LUCIDE.react_package, "lucide-react");
    assert_eq!(LUCIDE.static_package, "lucide-static");
    assert_eq!(LUCIDE.revision.len(), 40);
}
#[test]
fn content_check_does_not_claim_fixture_geometry_is_official() {
    let role = IconRole::ActionSearch;
    let bytes = b"synthetic content fixture, not imported upstream SVG";
    let catalog = ContentVerifiedCatalog::verify(
        lock(role, bytes),
        BTreeMap::from([(role, bytes.to_vec())]),
        license(),
    )
    .unwrap();
    assert_eq!(catalog.resolve(role).unwrap().bytes(), bytes);
    assert_eq!(
        catalog.resolve(IconRole::MediaPlay).err(),
        Some(CatalogError::NotImported(IconRole::MediaPlay))
    );
}
#[test]
fn changed_bytes_and_missing_bytes_fail_closed() {
    let role = IconRole::ActionSearch;
    assert!(matches!(
        ContentVerifiedCatalog::verify(
            lock(role, b"a"),
            BTreeMap::from([(role, b"b".to_vec())]),
            license()
        ),
        Err(CatalogError::HashMismatch(_))
    ));
    assert!(matches!(
        ContentVerifiedCatalog::verify(lock(role, b"a"), BTreeMap::new(), license()),
        Err(CatalogError::MissingAsset(_))
    ));
}
#[test]
fn changed_license_and_revision_are_rejected() {
    let role = IconRole::ActionSearch;
    let mut pin = lock(role, b"a");
    pin.revision = "main".into();
    assert!(matches!(
        ContentVerifiedCatalog::verify(pin, BTreeMap::new(), license()),
        Err(CatalogError::PinMismatch)
    ));
    assert!(matches!(
        ContentVerifiedCatalog::verify(lock(role, b"a"), BTreeMap::new(), b"wrong".to_vec()),
        Err(CatalogError::LicenseMismatch)
    ));
}
