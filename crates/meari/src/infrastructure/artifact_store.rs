//! Immutable local build bundles. Compiler invocation and runtime activation are separate ports.
use crate::{
    application::builder::{ArtifactStore, BuildBundle},
    domain::build::*,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_MANIFEST: u64 = 16 * 1024 * 1024;
const MAX_ARTIFACT: u64 = 2 * 1024 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn io(error: std::io::Error) -> String {
    error.to_string()
}
fn nonce() -> Result<String, String> {
    let sequence = SEQUENCE
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| "artifact sequence exhausted")?;
    Ok(format!("{}-{sequence}", std::process::id()))
}
fn normal(path: &Path, directory: bool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err("artifact path has an unexpected file type/link".into());
    }
    Ok(())
}
fn directories(root: &Path, relative: &str, create: bool) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    normal(&path, true)?;
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        if part == "." || part == ".." || part.contains(['\\', ':']) {
            return Err("unsafe artifact directory".into());
        }
        path.push(part);
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io(error)),
            }
        }
        normal(&path, true)?;
    }
    Ok(path)
}
fn read_small(path: &Path) -> Result<Vec<u8>, String> {
    normal(path, false)?;
    if fs::metadata(path).map_err(io)?.len() > MAX_MANIFEST {
        return Err("artifact metadata exceeds size budget".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("artifact metadata grew beyond budget".into());
    }
    Ok(bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
fn verify_file(path: &Path, expected: &OutputFile) -> Result<(), String> {
    normal(path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).map_err(io)?.permissions().mode();
        if mode & 0o7000 != 0
            || (expected.role == OutputRole::NativeExecutable && mode & 0o111 == 0)
        {
            return Err("artifact executable permissions differ from its role".into());
        }
    }
    if expected.length > MAX_ARTIFACT || fs::metadata(path).map_err(io)?.len() != expected.length {
        return Err("artifact file length/budget mismatch".into());
    }
    let mut reader = File::open(path).map_err(io)?;
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 32768];
    loop {
        let count = reader.read(&mut buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .ok_or("artifact length overflow")?;
        if length > expected.length {
            return Err("artifact file grew during read".into());
        }
        hash.update(&buffer[..count]);
    }
    if length != expected.length || format!("{:x}", hash.finalize()) != expected.content.as_str() {
        return Err("artifact file hash mismatch".into());
    }
    Ok(())
}
fn load_manifest(root: &Path, artifact: &ArtifactRef) -> Result<BuildManifest, String> {
    let object = directories(
        root,
        &format!("objects/{}", artifact.identity.as_str()),
        false,
    )?;
    for entry in fs::read_dir(&object).map_err(io)? {
        if !["manifest.json", "files"].contains(
            &entry
                .map_err(io)?
                .file_name()
                .to_str()
                .ok_or("non-UTF8 artifact entry")?,
        ) {
            return Err("artifact contains an unlisted root entry".into());
        }
    }
    let manifest: BuildManifest =
        serde_json::from_slice(&read_small(&object.join("manifest.json"))?)
            .map_err(|error| error.to_string())?;
    if manifest.target != artifact.target || manifest.identity()? != artifact.identity {
        return Err("artifact manifest identity mismatch".into());
    }
    let files = directories(&object, "files", false)?;
    let mut pending = vec![files.clone()];
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(io)? {
            let path = entry.map_err(io)?.path();
            let metadata = fs::symlink_metadata(&path).map_err(io)?;
            if metadata.file_type().is_symlink() {
                return Err("artifact contains a symbolic link".into());
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(&files)
                .map_err(|error| error.to_string())?
                .components()
                .map(|part| {
                    part.as_os_str()
                        .to_str()
                        .ok_or("artifact path is not UTF-8")
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("/");
            let relative = BuildPath::try_from(relative)?;
            let expected = manifest
                .outputs
                .get(&relative)
                .ok_or("artifact contains an unlisted file")?;
            total = total
                .checked_add(expected.length)
                .ok_or("artifact size overflow")?;
            if total > MAX_ARTIFACT {
                return Err("artifact bundle exceeds size budget".into());
            }
            verify_file(&path, expected)?;
            seen.insert(relative);
        }
    }
    if seen.len() != manifest.outputs.len() {
        return Err("artifact output disappeared".into());
    }
    Ok(manifest)
}
fn current(root: &Path) -> Result<Option<BuildPublication>, String> {
    let path = root.join("current.json");
    if !path.try_exists().map_err(io)? {
        return Ok(None);
    }
    let publication: BuildPublication =
        serde_json::from_slice(&read_small(&path)?).map_err(|error| error.to_string())?;
    publication.validate()?;
    for artifact in &publication.artifacts {
        let manifest = load_manifest(root, artifact)?;
        if manifest.session != publication.session
            || manifest.generation != publication.generation
            || manifest.graph_hash != publication.graph_hash
        {
            return Err("publication points to a different build revision".into());
        }
    }
    Ok(Some(publication))
}

pub struct FsArtifactStore {
    root: PathBuf,
    _writer: File,
}
impl FsArtifactStore {
    pub fn open(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(io)?;
        normal(root, true)?;
        let root = fs::canonicalize(root).map_err(io)?;
        let lock = root.join(".writer.lock");
        if lock.try_exists().map_err(io)? {
            normal(&lock, false)?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock)
            .map_err(io)?;
        writer
            .try_lock()
            .map_err(|error| format!("artifact writer is already owned: {error}"))?;
        Ok(Self {
            root,
            _writer: writer,
        })
    }
    /// Hosts may read the last successful immutable bundle without taking the writer lock.
    pub fn read_publication(root: &Path) -> Result<Option<BuildPublication>, String> {
        normal(root, true)?;
        current(&fs::canonicalize(root).map_err(io)?)
    }

    /// Read an immutable runtime definition while the builder retains its writer lock.
    pub fn read_application(
        root: &Path,
        artifact: &ArtifactRef,
    ) -> Result<crate::application::app_bundle::ApplicationDefinition, String> {
        normal(root, true)?;
        let root = fs::canonicalize(root).map_err(io)?;
        let manifest = load_manifest(&root, artifact)?;
        let path = BuildPath::try_from("runtime/application.json".to_owned())?;
        let output = manifest
            .outputs
            .get(&path)
            .ok_or("runtime application is missing")?;
        let directory = directories(
            &root,
            &format!("objects/{}/files/runtime", artifact.identity.as_str()),
            false,
        )?;
        let bytes = read_small(&directory.join("application.json"))?;
        if bytes.len() as u64 != output.length || ContentHash::of(&bytes) != output.content {
            return Err("runtime application changed during read".into());
        }
        let definition: crate::application::app_bundle::ApplicationDefinition =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        definition.validate_manifest(&manifest)?;
        Ok(definition)
    }
}
impl ArtifactStore for FsArtifactStore {
    fn stage(&mut self, bundle: &BuildBundle) -> Result<ArtifactRef, String> {
        bundle.validate()?;
        let manifest = serde_json::to_vec(&bundle.manifest).map_err(|error| error.to_string())?;
        let total = bundle
            .manifest
            .outputs
            .values()
            .try_fold(0u64, |sum, file| {
                sum.checked_add(file.length).ok_or("artifact size overflow")
            })?;
        if total > MAX_ARTIFACT || manifest.len() as u64 > MAX_MANIFEST {
            return Err("artifact exceeds store budget".into());
        }
        let artifact = ArtifactRef {
            target: bundle.manifest.target.clone(),
            identity: bundle.manifest.identity()?,
        };
        let objects = directories(&self.root, "objects", true)?;
        let destination = objects.join(artifact.identity.as_str());
        if destination.try_exists().map_err(io)? {
            load_manifest(&self.root, &artifact)?;
            return Ok(artifact);
        }
        let staging = directories(&self.root, &format!("staging/{}", nonce()?), true)?;
        directories(&staging, "files", true)?;
        for (path, bytes) in &bundle.files {
            let (parent, name) = path
                .as_str()
                .rsplit_once('/')
                .unwrap_or(("", path.as_str()));
            let directory = directories(&staging, &format!("files/{parent}"), true)?;
            write_new(&directory.join(name), bytes)?;
            #[cfg(unix)]
            if bundle.manifest.outputs[path].role == OutputRole::NativeExecutable {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(directory.join(name), fs::Permissions::from_mode(0o755))
                    .map_err(io)?;
            }
        }
        write_new(&staging.join("manifest.json"), &manifest)?;
        fs::rename(&staging, &destination).map_err(io)?;
        load_manifest(&self.root, &artifact)?;
        Ok(artifact)
    }
    fn publish(&mut self, publication: &BuildPublication) -> Result<(), String> {
        publication.validate()?;
        if let Some(previous) = current(&self.root)? {
            if previous == *publication {
                return Ok(());
            }
            if previous.generation >= publication.generation {
                return Err("publication would replace a newer build".into());
            }
        }
        for artifact in &publication.artifacts {
            let manifest = load_manifest(&self.root, artifact)?;
            if manifest.session != publication.session
                || manifest.generation != publication.generation
                || manifest.graph_hash != publication.graph_hash
            {
                return Err("publication revision mismatch".into());
            }
        }
        let bytes = serde_json::to_vec(publication).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("publication exceeds size budget".into());
        }
        let temporary = self.root.join(format!("current-{}.tmp", nonce()?));
        write_new(&temporary, &bytes)?;
        fs::rename(temporary, self.root.join("current.json")).map_err(io)
    }
    fn current(&self) -> Result<Option<BuildPublication>, String> {
        current(&self.root)
    }
    fn manifest(&self, artifact: &ArtifactRef) -> Result<BuildManifest, String> {
        load_manifest(&self.root, artifact)
    }
}
