//! X11 desktop queries and window placement through `xdotool` (Linux first; other platforms are
//! not wired yet and fall back to the window manager's placement).
use super::layout::Rect;
use std::process::Command;
use std::time::{Duration, Instant};

pub fn display_size() -> (u32, u32) {
    let parsed = Command::new("xdotool")
        .arg("getdisplaygeometry")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            let mut it = s.split_whitespace().map(|v| v.parse::<u32>().ok());
            Some((it.next()??, it.next()??))
        });
    parsed.unwrap_or((1280, 800))
}

/// Wait for a visible window whose title is exactly `title`, then move and resize it.
pub fn place(title: &str, rect: Rect, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let found = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name"])
            .arg(format!("^{}$", regex_escape(title)))
            .output()
            .map_err(|e| format!("xdotool: {e}"))?;
        let ids = String::from_utf8_lossy(&found.stdout).to_string();
        if let Some(id) = ids.lines().last() {
            let run = |args: &[&str]| Command::new("xdotool").args(args).status();
            let (x, y) = (rect.x.to_string(), rect.y.to_string());
            let (w, h) = (rect.width.to_string(), rect.height.to_string());
            run(&["windowsize", id, &w, &h]).map_err(|e| e.to_string())?;
            run(&["windowmove", id, &x, &y]).map_err(|e| e.to_string())?;
            return Ok(());
        }
        if started.elapsed() > timeout {
            return Err(format!("window {title:?} did not appear"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn regex_escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let special = "\\.^$|?*+()[]{}".contains(c);
            special
                .then_some('\\')
                .into_iter()
                .chain(std::iter::once(c))
        })
        .collect()
}
