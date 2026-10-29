//! Fixture discovery: `fixtures/<name>.tsx` + `fixtures/<name>.css` (+ optional `.dark` marker).
use std::path::PathBuf;

pub struct Fixture {
    pub name: String,
    pub source: String,
    pub css: String,
    pub dark: bool,
}

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn load() -> Result<Vec<Fixture>, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir())
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "tsx").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let read = |ext: &str| {
                std::fs::read_to_string(dir().join(format!("{name}.{ext}")))
                    .map_err(|e| format!("{name}.{ext}: {e}"))
            };
            Ok(Fixture {
                dark: name.starts_with("dark-"),
                source: read("tsx")?,
                css: read("css")?,
                name,
            })
        })
        .collect()
}
