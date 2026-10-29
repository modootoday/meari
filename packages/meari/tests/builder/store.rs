use super::*;
use meari::infrastructure::artifact_store::FsArtifactStore;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "meari-build-store-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn prepare(
    builder: &mut BuildCoordinator,
    store: &mut dyn ArtifactStore,
) -> (BuildTicket, ArtifactRef) {
    let native = target(TargetKind::Native);
    let ticket = builder
        .request(graph(), BTreeMap::from([(native.clone(), spec())]))
        .unwrap();
    let artifact = builder
        .accept(&ticket, &bundle(builder, &ticket, &native), store)
        .unwrap();
    (ticket, artifact)
}

#[test]
fn runtime_reader_works_with_live_writer_and_rejects_corrupt_or_wrong_stamp() {
    use meari::application::app_bundle::{AppBuildStamp, ApplicationDefinition};
    let root = Scratch::new();
    let mut store = FsArtifactStore::open(&root.0).unwrap();
    let mut builder = BuildCoordinator::new(ContentHash::of(b"runtime-reader"));
    let ticket = builder.request(graph(), targets()).unwrap();
    let mut bundle = bundle(&builder, &ticket, &target(TargetKind::Native));
    let mut definition = ApplicationDefinition {
        component: meari::parse(source(), Language::Tsx).unwrap(),
        props: BTreeMap::new(),
        title: "Stored runtime".into(),
        native_viewport: meari::domain::viewport::Viewport {
            width: 640,
            height: 480,
            dark: false,
        },
        stamp: AppBuildStamp {
            session: ticket.session.clone(),
            generation: ticket.generation,
            graph_hash: ticket.graph_hash.clone(),
            target: target(TargetKind::Native),
        },
    };
    super::runtime::attach(&mut bundle, &definition);
    let artifact = store.stage(&bundle).unwrap();
    assert_eq!(
        FsArtifactStore::read_application(&root.0, &artifact)
            .unwrap()
            .title,
        definition.title
    );
    let file = root
        .0
        .join("objects")
        .join(artifact.identity.as_str())
        .join("files/runtime/application.json");
    fs::write(&file, b"{}").unwrap();
    assert!(FsArtifactStore::read_application(&root.0, &artifact).is_err());
    definition.stamp.generation += 1;
    super::runtime::attach(&mut bundle, &definition);
    let stale = store.stage(&bundle).unwrap();
    assert!(FsArtifactStore::read_application(&root.0, &stale).is_err());
}

#[test]
fn immutable_artifacts_have_one_writer_and_resume_last_good_after_reopen() {
    let root = Scratch::new();
    let mut store = FsArtifactStore::open(&root.0).unwrap();
    assert!(FsArtifactStore::open(&root.0).is_err());
    let mut builder = BuildCoordinator::new(ContentHash::of(b"first-process"));
    let (ticket, artifact) = prepare(&mut builder, &mut store);
    assert_eq!(
        store
            .stage(&bundle(&builder, &ticket, &target(TargetKind::Native)))
            .unwrap(),
        artifact
    );
    let published = builder.publish(&ticket, &mut store).unwrap();
    assert_eq!(
        FsArtifactStore::read_publication(&root.0).unwrap(),
        Some(published.clone())
    );
    let old_path = root
        .0
        .join("objects")
        .join(artifact.identity.as_str())
        .join("files/host.bin");
    let bytes = fs::read(&old_path).unwrap();
    let (new_ticket, new_artifact) = prepare(&mut builder, &mut store);
    assert_ne!(artifact.identity, new_artifact.identity);
    builder.cancel(&new_ticket).unwrap();
    assert_eq!(store.current().unwrap(), Some(published.clone()));
    assert_eq!(fs::read(&old_path).unwrap(), bytes);
    drop(store);
    let mut reopened = FsArtifactStore::open(&root.0).unwrap();
    let mut resumed =
        BuildCoordinator::resume(&reopened, ContentHash::of(b"second-process")).unwrap();
    assert_eq!(resumed.last_good(), Some(&published));
    let (ticket, _) = prepare(&mut resumed, &mut reopened);
    assert!(ticket.generation > published.generation);
    resumed.publish(&ticket, &mut reopened).unwrap();
    assert_eq!(fs::read(old_path).unwrap(), bytes);
}

#[test]
fn modified_missing_or_unlisted_artifact_files_are_rejected() {
    for mutation in ["modified", "missing", "unlisted", "unlisted-root"] {
        let root = Scratch::new();
        let mut store = FsArtifactStore::open(&root.0).unwrap();
        let mut builder = BuildCoordinator::new(ContentHash::of(mutation.as_bytes()));
        let (ticket, artifact) = prepare(&mut builder, &mut store);
        builder.publish(&ticket, &mut store).unwrap();
        let object = root.0.join("objects").join(artifact.identity.as_str());
        match mutation {
            "modified" => fs::write(object.join("files/host.bin"), b"tampered").unwrap(),
            "missing" => fs::remove_file(object.join("files/host.bin")).unwrap(),
            "unlisted" => fs::write(object.join("files/extra.bin"), b"extra").unwrap(),
            _ => fs::write(object.join("extra.bin"), b"extra").unwrap(),
        }
        assert!(store.current().is_err(), "{mutation}");
        assert!(store.manifest(&artifact).is_err(), "{mutation}");
    }
}

#[cfg(windows)]
#[test]
fn windows_locked_pointer_failure_retains_success_and_old_binary_is_never_overwritten() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = Scratch::new();
    let mut store = FsArtifactStore::open(&root.0).unwrap();
    let mut builder = BuildCoordinator::new(ContentHash::of(b"windows-lock"));
    let (ticket, artifact) = prepare(&mut builder, &mut store);
    let first = builder.publish(&ticket, &mut store).unwrap();
    let old_path = root
        .0
        .join("objects")
        .join(artifact.identity.as_str())
        .join("files/host.bin");
    let old_binary = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&old_path)
        .unwrap();
    let pointer = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(root.0.join("current.json"))
        .unwrap();
    let (ticket, _) = prepare(&mut builder, &mut store);
    assert!(builder.publish(&ticket, &mut store).is_err());
    assert_eq!(builder.last_good(), Some(&first));
    assert_eq!(store.current().unwrap(), Some(first));
    drop(pointer);
    builder.publish(&ticket, &mut store).unwrap();
    assert_eq!(fs::read(&old_path).unwrap(), b"fixture library payload");
    drop(old_binary);
}
