//! Executable CSS-first Tailwind and direct CSS kitchen-sink fixtures.
use crate::{
    application::{self, Renderer, StylesheetRenderRequest},
    infrastructure::{
        artifacts, document::InteractiveDocument, stylesheet::DirectCssCompiler,
        tailwind::TailwindCompiler,
    },
};
use std::{fs, path::Path};

pub(super) fn run(executable: &Path, output: &Path) -> Result<(), String> {
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let compiler = TailwindCompiler::pinned(executable)?;
    let mut report = Vec::new();
    for (name, json, tailwind) in [
        (
            "direct-css",
            include_str!("../../fixtures/custom-css/raw.json"),
            false,
        ),
        (
            "tailwind-css",
            include_str!("../../fixtures/custom-css/tailwind.json"),
            true,
        ),
    ] {
        for width in [400, 800] {
            let mut request: StylesheetRenderRequest =
                serde_json::from_str(json).map_err(|e| e.to_string())?;
            request.viewport.width = width;
            let directory = output.join(format!("{name}-{width}"));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            let result = (|| {
                let (component, compiled) = if tailwind {
                    application::prepare_stylesheet(&request, &compiler)?
                } else {
                    application::prepare_stylesheet(&request, &DirectCssCompiler)?
                };
                fs::write(directory.join("compiled.css"), &compiled.css)
                    .map_err(|e| e.to_string())?;
                artifacts::json(&directory.join("stylesheet.json"), &compiled)?;
                let mut doc =
                    InteractiveDocument::new(component, &compiled.css, request.viewport.clone())?;
                artifacts::capture(&mut doc, &directory, "initial")?;
                let expectations: Vec<(&str, f32, f32)> = if tailwind {
                    vec![
                        ("themed", 120.0, 48.0),
                        ("applied", 80.0, 40.0),
                        ("responsive", if width >= 600 { 160.0 } else { 64.0 }, 32.0),
                    ]
                } else {
                    vec![("panel", if width >= 600 { 240.0 } else { 120.0 }, 64.0)]
                };
                for (id, w, h) in &expectations {
                    let bounds = doc.renderer.bounds(id)?;
                    if (bounds.width - w).abs() > 0.01 || (bounds.height - h).abs() > 0.01 {
                        return Err(format!(
                            "{id}: expected {w}x{h}, got {}x{}",
                            bounds.width, bounds.height
                        ));
                    }
                }
                Ok::<_, String>(expectations.len())
            })();
            report.push(serde_json::json!({"case":name,"width":width,"passed":result.is_ok(),"geometry_assertions":result.as_ref().ok(),"error":result.err()}));
            artifacts::json(&output.join("report.json"), &report)?;
        }
    }
    if report.iter().any(|row| row["passed"] != true) {
        Err(
            "custom CSS kitchen-sink geometry assertions failed; inspect styles-suite/report.json"
                .into(),
        )
    } else {
        Ok(())
    }
}
