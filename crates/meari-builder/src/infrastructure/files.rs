use meari::{
    application::builder::BuildInputs,
    domain::build::{BuildPath, ContentHash},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub fn hash_file(path: &Path) -> Result<ContentHash, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    format!("{:x}", hash.finalize()).try_into()
}
pub struct ProjectInputs {
    root: PathBuf,
}
pub(super) fn is_link(metadata: &fs::Metadata) -> bool {
    super::platform::is_link(metadata)
}
impl ProjectInputs {
    pub fn new(root: &Path) -> Result<Self, String> {
        Ok(Self {
            root: fs::canonicalize(root).map_err(|error| error.to_string())?,
        })
    }
}
impl BuildInputs for ProjectInputs {
    fn read(&self, path: &BuildPath) -> Result<Vec<u8>, String> {
        let candidate = self.root.join(path.as_str());
        let resolved = fs::canonicalize(&candidate).map_err(|error| error.to_string())?;
        if !resolved.starts_with(&self.root) {
            return Err("project input escapes its root".into());
        }
        let mut current = self.root.clone();
        for part in path.as_str().split('/') {
            current.push(part);
            if is_link(&fs::symlink_metadata(&current).map_err(|error| error.to_string())?) {
                return Err("project input contains a link".into());
            }
        }
        let metadata = fs::metadata(&resolved).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 * 1024 {
            return Err("project input exceeds file budget".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(resolved)
            .map_err(|error| error.to_string())?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("project input grew beyond budget".into());
        }
        Ok(bytes)
    }
}
pub fn framework_hash(root: &Path) -> Result<ContentHash, String> {
    let mut paths = vec![PathBuf::from("Cargo.toml"), PathBuf::from("Cargo.lock")];
    for package in ["meari", "meari-native", "meari-paint"] {
        paths.push(PathBuf::from(format!("crates/{package}/Cargo.toml")));
        for folder in ["src", "fixtures", "assets"] {
            let base = PathBuf::from(format!("crates/{package}/{folder}"));
            if !root.join(&base).is_dir() {
                continue;
            }
            let mut pending = vec![base];
            while let Some(directory) = pending.pop() {
                for entry in
                    fs::read_dir(root.join(&directory)).map_err(|error| error.to_string())?
                {
                    let entry = entry.map_err(|error| error.to_string())?;
                    let relative = directory.join(entry.file_name());
                    let kind = entry.file_type().map_err(|error| error.to_string())?;
                    if is_link(
                        &fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?,
                    ) {
                        return Err("framework source contains a link".into());
                    }
                    if kind.is_dir() {
                        pending.push(relative);
                    } else if kind.is_file() {
                        paths.push(relative);
                    }
                }
            }
        }
    }
    paths.sort();
    let mut hash = Sha256::new();
    for path in paths {
        let name = path
            .to_str()
            .ok_or("framework path is not UTF-8")?
            .replace('\\', "/");
        let content = hash_file(&root.join(path))?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(content.as_str().as_bytes());
    }
    format!("{:x}", hash.finalize()).try_into()
}
