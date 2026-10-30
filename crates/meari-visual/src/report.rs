//! Per-fixture run and summary report (report.md, index.html side-by-side sheet).
use crate::host::PixelGate;
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

pub const ADVISORY: &str = "ADVISORY (OS rendering difference)";

pub fn run_one(fixture: &Fixture, cx: &Context) -> Row {
    let run = || -> Result<compare::Diff, String> {
        let dir = cx.out.join(&fixture.name);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let css = style::resolve(fixture, cx.tailwind)?;
        std::fs::write(dir.join("style.css"), &css).map_err(|e| e.to_string())?;
        let html = web::html(&fixture.source, &style::chrome_generics(&css), fixture.dark)
            .map_err(|e| format!("web plan: {e}"))?;
        let html_path = std::fs::canonicalize(&dir)
            .map_err(|e| e.to_string())?
            .join("web.html");
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let web = web::screenshot(cx.chrome, &html_path, &dir.join("web.png"), &fixture.scroll)?;
        let native = native::render(&fixture.source, &css, fixture.dark, &fixture.scroll)
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

fn status(row: &Row, gate: PixelGate) -> &'static str {
    match (row.kind, &row.outcome, row.passed()) {
        (Kind::Supported, _, true) => "MATCH",
        (Kind::Supported, Ok(d), false) if gate == PixelGate::Advisory && d.layout_equal => {
            ADVISORY
        }
        (Kind::Supported, Ok(d), false) if gate == PixelGate::Advisory && !d.layout_equal => {
            "LAYOUT-MISMATCH"
        }
        (Kind::Supported, Ok(_), false) => "MISMATCH",
        (Kind::Supported, Err(_), _) => "ERROR",
        (Kind::Gap, _, true) => "GAP-NOW-MATCHES (move to supported)",
        (Kind::Gap, _, false) => "KNOWN-GAP",
    }
}

/// Whether a row fails the run under `gate` (known gaps and advisory rows never do).
pub fn fails(row: &Row, gate: PixelGate) -> bool {
    row.kind == Kind::Supported && !row.passed() && status(row, gate) != ADVISORY
}

/// Returns the number of failing supported fixtures (known gaps never fail the run).
pub fn write(
    out: &Path,
    rows: &[Row],
    tolerance: compare::Tolerance,
    gate: PixelGate,
) -> Result<usize, String> {
    let mut md = format!(
        "# meari visual test\n\n> Automated pixel comparison only — **not verified by a human eye**. Open `index.html` for native | web | diff sheets.\n\nviewport {}x{}, channel tolerance {}, default max differing ratio {:.3}%, gate {:?}\n\n| fixture | kind | result | differing px | ratio | limit | max channel Δ / error |\n|---|---|---|---|---|---|---|\n",
        crate::WIDTH,
        crate::HEIGHT,
        tolerance.channel,
        tolerance.max_ratio * 100.0,
        gate
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
        if fails(row, gate) {
            failed += 1;
        }
        let limit = match &row.reason {
            Some(r) => format!("{:.3}% ({r})", row.max_ratio * 100.0),
            None => format!("{:.3}%", row.max_ratio * 100.0),
        };
        let line = format!(
            "| {} | {kind} | {} | {px} | {ratio} | {limit} | {detail} |\n",
            row.name,
            status(row, gate)
        );
        print!("{line}");
        md.push_str(&line);
        html.push_str(&format!(
            "<h2>{} — {} ({ratio})</h2><p>{}</p><img src=\"{}/sheet.png\">",
            row.name,
            status(row, gate),
            detail.replace('<', "&lt;"),
            row.name
        ));
    }
    std::fs::write(out.join("report.md"), md).map_err(|e| e.to_string())?;
    std::fs::write(out.join("index.html"), html).map_err(|e| e.to_string())?;
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::{ADVISORY, Row, fails, status};
    use crate::{compare::Diff, fixtures::Kind, host::PixelGate};

    fn row(outcome: Result<(usize, bool), &str>) -> Row {
        Row {
            name: "f".into(),
            kind: Kind::Supported,
            max_ratio: 0.01,
            reason: None,
            outcome: outcome
                .map(|(differing, layout_equal)| Diff {
                    differing,
                    total: 100,
                    max_channel: 200,
                    image: Vec::new(),
                    layout_equal,
                })
                .map_err(str::to_owned),
        }
    }

    #[test]
    fn strict_gate_fails_every_mismatch() {
        let aa = row(Ok((5, true)));
        assert_eq!(status(&aa, PixelGate::Strict), "MISMATCH");
        assert!(fails(&aa, PixelGate::Strict));
        assert!(!fails(&row(Ok((1, true))), PixelGate::Strict));
    }

    #[test]
    fn advisory_gate_reports_aa_only_and_still_fails_layout_and_errors() {
        let aa = row(Ok((5, true)));
        assert_eq!(status(&aa, PixelGate::Advisory), ADVISORY);
        assert!(!fails(&aa, PixelGate::Advisory));
        let layout = row(Ok((5, false)));
        assert_eq!(status(&layout, PixelGate::Advisory), "LAYOUT-MISMATCH");
        assert!(fails(&layout, PixelGate::Advisory));
        let error = row(Err("render failed"));
        assert_eq!(status(&error, PixelGate::Advisory), "ERROR");
        assert!(fails(&error, PixelGate::Advisory));
    }
}
