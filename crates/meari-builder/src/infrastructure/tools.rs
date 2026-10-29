use super::files::hash_file;
use meari::domain::build::{ContentHash, ProviderPin};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug)]
pub struct RegisteredTool {
    pub path: PathBuf,
    pub pin: ProviderPin,
}
impl RegisteredTool {
    /// Trusted host registration, never editable app-source configuration.
    pub fn register(path: &Path, expected: ContentHash) -> Result<Self, String> {
        let path = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
        if hash_file(&path)? != expected {
            return Err("tool hash differs from registration".into());
        }
        let output = Command::new(&path)
            .arg("--version")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err("tool version probe failed".into());
        }
        let version = String::from_utf8(output.stdout)
            .map_err(|error| error.to_string())?
            .trim()
            .to_owned();
        if version.is_empty() {
            return Err("tool version probe is empty".into());
        }
        Ok(Self {
            path,
            pin: ProviderPin {
                version: version.clone(),
                revision: version,
                content: expected,
            },
        })
    }
    pub fn verify(&self) -> Result<(), String> {
        if hash_file(&self.path)? != self.pin.content {
            return Err("registered tool changed".into());
        }
        Ok(())
    }
}
pub fn rustc_metadata(tool: &RegisteredTool) -> Result<(String, String, String), String> {
    tool.verify()?;
    let output = Command::new(&tool.path)
        .arg("-vV")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("rustc metadata probe failed".into());
    }
    let text = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::to_owned)
            .ok_or_else(|| format!("rustc metadata missing {name}"))
    };
    Ok((
        field("release: ")?,
        field("commit-hash: ")?,
        field("host: ")?,
    ))
}
