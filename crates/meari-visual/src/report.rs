//! Per-fixture run and summary report (report.md, index.html side-by-side sheet).
use crate::render::{native, style, web};
use crate::{compare, fixtures::Fixture, fixtures::Kind, sheet};
use std::path::Path;

pub struct Row {
    pub name: String,
    pub kind: Kind,
    pub max_ratio: f64,
    pub reason: Option<String>,
    pub outcome: Result<compare::Diff, String>,
}

impl Row {
    fn passed(&self) -> bool {
        matches!(&self.outcome, Ok(d) if d.ratio() <= self.max_ratio)
    }
}

pub struct Context<'a> {
    pub out: &'a Path,
    pub chrome: &'a str,
    pub tailwind: &'a Path,
    pub tolerance: compare::Tolerance,
}

pub fn run_one(fixture: &Fixture, cx: &Context) -> Row {
    let run = || -> Result<compare::Diff, String> {
        let dir = cx.out.join(&fixture.name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let css = style::resolve(fixture, cx.tailwind)?;
        std::fs::write(dir.join("style.css"), &css).map_err(|e| e.to_string())?;
        let html =
            web::html(&fixture.source, &css, fixture.dark).map_err(|e| format!("web plan: {e}"))?;
        let html_path = std::fs::canonicalize(&dir)
            .map_err(|e| e.to_string())?
            .join("web.html");
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let web = web::screenshot(cx.chrome, &html_path, &dir.join("web.png"))?;
        let native = native::render(&fixture.source, &css, fixture.dark)
            .map_err(|e| format!("native: {e}"))?;
        compare::write_png(&dir.join("native.png"), &native)?;
        let diff = compare::diff(&native, &web, cx.tolerance)?;
        compare::write_png(&dir.join("diff.png"), &diff.image)?;
        sheet::write(&dir.join("sheet.png"), &[&native, &web, &diff.image])?;
        Ok(diff)
    };
    Row {
        name: fixture.name.clone(),
        kind: fixture.kind,
        max_ratio: fixture
            .tolerance
            .as_ref()
            .map_or(cx.tolerance.max_ratio, |t| t.0),
        reason: fixture.tolerance.as_ref().map(|t| t.1.clone()),
        outcome: run(),
    }
}

fn status(row: &Row) -> &'static str {
    match (row.kind, &row.outcome, row.passed()) {
        (Kind::Supported, _, true) => "MATCH",
        (Kind::Supported, Ok(_), false) => "MISMATCH",
        (Kind::Supported, Err(_), _) => "ERROR",
        (Kind::Gap, _, true) => "GAP-NOW-MATCHES (move to supported)",
        (Kind::Gap, _, false) => "KNOWN-GAP",
    }
}

/// Returns the number of failing supported fixtures (known gaps never fail the run).
pub fn write(out: &Path, rows: &[Row], tolerance: compare::Tolerance) -> Result<usize, String> {
    let mut md = format!(
        "# meari visual test\n\n> Automated pixel comparison only — **not verified by a human eye**. Open `index.html` for native | web | diff sheets.\n\nviewport {}x{}, channel tolerance {}, default max differing ratio {:.3}%\n\n| fixture | kind | result | differing px | ratio | limit | max channel Δ / error |\n|---|---|---|---|---|---|---|\n",
        crate::WIDTH,
        crate::HEIGHT,
        tolerance.channel,
        tolerance.max_ratio * 100.0
    );
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>meari visual</title><style>body{font:14px sans-serif;margin:16px}img{border:1px solid #ccc;max-width:100%}h2{margin:24px 0 4px}</style><h1>meari visual test</h1><p><b>Automated pixel comparison only, not verified by a human eye.</b> Each sheet: native | web | diff (magenta = differs).</p>",
    );
    let mut failed = 0;
    for row in rows {
        let kind = if row.kind == Kind::Gap {
            "gap"
        } else {
            "supported"
        };
        let (px, ratio, detail) = match &row.outcome {
            Ok(d) => (
                d.differing.to_string(),
                format!("{:.3}%", d.ratio() * 100.0),
                d.max_channel.to_string(),
            ),
            Err(e) => (
                "-".into(),
                "-".into(),
                e.replace('|', "/").replace('\n', " "),
            ),
        };
        if row.kind == Kind::Supported && !row.passed() {
            failed += 1;
        }
        let limit = match &row.reason {
            Some(r) => format!("{:.3}% ({r})", row.max_ratio * 100.0),
            None => format!("{:.3}%", row.max_ratio * 100.0),
        };
        let line = format!(
            "| {} | {kind} | {} | {px} | {ratio} | {limit} | {detail} |\n",
            row.name,
            status(row)
        );
        print!("{line}");
        md.push_str(&line);
        html.push_str(&format!(
            "<h2>{} — {} ({ratio})</h2><p>{}</p><img src=\"{}/sheet.png\">",
            row.name,
            status(row),
            detail.replace('<', "&lt;"),
            row.name
        ));
    }
    std::fs::write(out.join("report.md"), md).map_err(|e| e.to_string())?;
    std::fs::write(out.join("index.html"), html).map_err(|e| e.to_string())?;
    Ok(failed)
}
