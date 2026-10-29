//! Web pane: a Chrome `--app` window on the served page, with its own throwaway profile.
use super::process::Owned;
use crate::screen::layout::Rect;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn spawn(chrome: &str, url: &str, rect: Rect, profile: &Path) -> Result<Owned, String> {
    let child = Command::new(chrome)
        .arg(format!("--app={url}"))
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--window-position={},{}", rect.x, rect.y))
        .arg(format!("--window-size={},{}", rect.width, rect.height))
        .args([
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--password-store=basic",
            "--renderer-process-limit=1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("start {chrome}: {e}"))?;
    Ok(Owned::new("chrome", child))
}
