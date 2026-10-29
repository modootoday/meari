//! Native pane: this binary re-executed as `native-window`, hosting one meari-native window.
use super::process::Owned;
use crate::catalog::Prepared;
use crate::screen::layout::Rect;
use meari_native::application::WindowOptions;
use meari_native::domain::Viewport;
use meari_native::infrastructure::document::InteractiveDocument;
use std::process::{Command, Stdio};

pub fn title(name: &str) -> String {
    format!("Meari Native - {name}")
}

pub fn spawn(prepared: &Prepared, rect: Rect) -> Result<Owned, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = Command::new(exe)
        .arg("native-window")
        .arg(&prepared.name)
        .arg(&prepared.source)
        .arg(&prepared.css)
        .arg(rect.width.to_string())
        .arg(rect.height.to_string())
        .arg(if prepared.dark { "dark" } else { "light" })
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("start native window: {e}"))?;
    Ok(Owned::new("native", child))
}

/// Child mode: `native-window NAME SOURCE.tsx STYLE.css WIDTH HEIGHT light|dark`.
pub fn run_window(args: &[String]) -> Result<(), String> {
    let [name, source, css, width, height, scheme] = args else {
        return Err("native-window NAME SOURCE CSS WIDTH HEIGHT light|dark".into());
    };
    let read = |p: &String| std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"));
    let component = meari::parse(&read(source)?, meari::application::Language::Tsx)
        .map_err(|d| format!("parse: {}", d.message))?;
    let size = |v: &String| v.parse::<u32>().map_err(|_| format!("bad size {v}"));
    let viewport = Viewport {
        width: size(width)?,
        height: size(height)?,
        dark: scheme == "dark",
    };
    let document = InteractiveDocument::new(component, &read(css)?, viewport)?;
    let options = WindowOptions { title: title(name) };
    meari_native::infrastructure::window::show_with_options(document, None, options)
}
