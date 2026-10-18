//! Explicit user-mediated selection; no filesystem reads or uploads are implied.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedFile {
    pub path: std::path::PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRequest {
    pub multiple: bool,
    pub accept: String,
}

pub trait FilePicker {
    /// None is cancellation; an empty selection explicitly clears the value.
    fn select(&mut self, request: &FileRequest) -> Result<Option<Vec<SelectedFile>>, String>;
}
