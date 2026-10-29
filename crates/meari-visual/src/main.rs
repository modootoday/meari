//! Visual regression harness: one TSX fixture set, two renderers, one pixel diff.
//!
//! Native: `meari-native` InteractiveDocument -> blitz layout -> vello CPU raster (offscreen, no window).
//! Web: the same portable DOM plan the WASM `WebDocument` adapter mounts (`bind_with_scope` ->
//! `StateProgram::project` -> `dom::prepare`), serialized to HTML and screenshotted by headless Chrome.
mod compare;
mod fixtures;
mod native;
mod report;
mod web;

use std::path::PathBuf;

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 320;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut out = PathBuf::from("target/meari-visual");
    let mut chrome = String::from("/usr/bin/google-chrome");
    let mut tolerance = compare::Tolerance::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().expect("--out DIR").into(),
            "--chrome" => chrome = args.next().expect("--chrome PATH"),
            "--channel-tolerance" => {
                tolerance.channel = args.next().and_then(|v| v.parse().ok()).expect("u8")
            }
            "--max-ratio" => {
                tolerance.max_ratio = args.next().and_then(|v| v.parse().ok()).expect("f64")
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    std::fs::create_dir_all(&out).expect("create output dir");
    let fixtures = fixtures::load().expect("load fixtures");
    let mut rows = Vec::new();
    for fixture in &fixtures {
        rows.push(report::run_one(fixture, &out, &chrome, tolerance));
    }
    let failed = report::write(&out, &rows, tolerance).expect("write report");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
