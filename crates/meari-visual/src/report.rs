//! Per-fixture run and summary report (report.md + report.json-ish TSV).
use crate::{compare, fixtures::Fixture, native, web};
use std::path::Path;

pub struct Row {
    pub name: String,
    pub outcome: Result<compare::Diff, String>,
}

pub fn run_one(fixture: &Fixture, out: &Path, chrome: &str, tolerance: compare::Tolerance) -> Row {
    let run = || -> Result<compare::Diff, String> {
        let dir = out.join(&fixture.name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let native = native::render(&fixture.source, &fixture.css, fixture.dark)
            .map_err(|e| format!("native: {e}"))?;
        compare::write_png(&dir.join("native.png"), &native)?;
        let html = web::html(&fixture.source, &fixture.css, fixture.dark)
            .map_err(|e| format!("web plan: {e}"))?;
        let html_path = std::fs::canonicalize(&dir)
            .map_err(|e| e.to_string())?
            .join("web.html");
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let web = web::screenshot(chrome, &html_path, &dir.join("web.png"))?;
        let diff = compare::diff(&native, &web, tolerance)?;
        compare::write_png(&dir.join("diff.png"), &diff.image)?;
        Ok(diff)
    };
    Row {
        name: fixture.name.clone(),
        outcome: run(),
    }
}

/// Returns the number of failing fixtures.
pub fn write(out: &Path, rows: &[Row], tolerance: compare::Tolerance) -> Result<usize, String> {
    let mut md = format!(
        "# meari visual test\n\nviewport {}x{}, channel tolerance {}, max differing ratio {:.3}%\n\n| fixture | result | differing px | ratio | max channel Δ |\n|---|---|---|---|---|\n",
        crate::WIDTH,
        crate::HEIGHT,
        tolerance.channel,
        tolerance.max_ratio * 100.0
    );
    let mut failed = 0;
    for row in rows {
        let line = match &row.outcome {
            Ok(d) if d.ratio() <= tolerance.max_ratio => {
                format!(
                    "| {} | MATCH | {} | {:.3}% | {} |\n",
                    row.name,
                    d.differing,
                    d.ratio() * 100.0,
                    d.max_channel
                )
            }
            Ok(d) => {
                failed += 1;
                format!(
                    "| {} | MISMATCH | {} | {:.3}% | {} |\n",
                    row.name,
                    d.differing,
                    d.ratio() * 100.0,
                    d.max_channel
                )
            }
            Err(e) => {
                failed += 1;
                format!(
                    "| {} | ERROR | - | - | {} |\n",
                    row.name,
                    e.replace('|', "/")
                )
            }
        };
        print!("{line}");
        md.push_str(&line);
    }
    std::fs::write(out.join("report.md"), md).map_err(|e| e.to_string())?;
    Ok(failed)
}
