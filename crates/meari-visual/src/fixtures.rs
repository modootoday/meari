//! Fixture discovery.
//!
//! `fixtures/supported/<name>.tsx` must match; `fixtures/gaps/<name>.tsx` records known Native gaps
//! (reported, never hidden, but not counted as regressions). Styling comes from `<name>.css` when
//! present, otherwise the component's Tailwind classes are compiled with the pinned standalone
//! Tailwind v4.3.3 binary. An optional `<name>.tolerance` holds `max_ratio=<f64>` plus a reason.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Supported,
    Gap,
}

pub struct Fixture {
    pub name: String,
    pub kind: Kind,
    pub source: String,
    pub css: Option<String>,
    pub dark: bool,
    pub tolerance: Option<(f64, String)>,
}

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

pub fn load() -> Result<Vec<Fixture>, String> {
    let mut all = load_kind(&dir().join("supported"), Kind::Supported)?;
    all.extend(load_kind(&dir().join("gaps"), Kind::Gap)?);
    Ok(all)
}

fn load_kind(dir: &Path, kind: Kind) -> Result<Vec<Fixture>, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "tsx").then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let read = |ext: &str| std::fs::read_to_string(dir.join(format!("{name}.{ext}")));
            let tolerance = read("tolerance").ok().map(parse_tolerance).transpose()?;
            Ok(Fixture {
                dark: name.starts_with("dark-"),
                source: read("tsx").map_err(|e| format!("{name}.tsx: {e}"))?,
                css: read("css").ok(),
                kind,
                tolerance,
                name,
            })
        })
        .collect()
}

fn parse_tolerance(text: String) -> Result<(f64, String), String> {
    let mut ratio = None;
    let mut reason = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("max_ratio=") {
            ratio = Some(v.trim().parse::<f64>().map_err(|e| e.to_string())?);
        } else if let Some(v) = line.strip_prefix("reason=") {
            reason = v.trim().to_owned();
        }
    }
    match ratio {
        Some(r) if !reason.is_empty() => Ok((r, reason)),
        _ => Err("tolerance file needs max_ratio= and reason=".into()),
    }
}
