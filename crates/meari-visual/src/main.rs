//! `cargo xtask visual-test`: run every fixture through both renderers and write the report.
use meari_visual::{compare, fixtures, report};
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut out = PathBuf::from("target/meari-visual");
    let mut chrome = String::from("/usr/bin/google-chrome");
    let mut tolerance = compare::Tolerance::default();
    let mut only: Option<String> = None;
    let mut tailwind = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtime/tools/tailwind/tailwindcss-linux-x64"
    ));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().expect("--out DIR").into(),
            "--only" => only = args.next(),
            "--tailwind" => tailwind = args.next().expect("--tailwind PATH").into(),
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
    let mut fixtures = fixtures::load().expect("load fixtures");
    if let Some(only) = &only {
        fixtures.retain(|f| f.name.contains(only.as_str()));
    }
    let cx = report::Context {
        out: &out,
        chrome: &chrome,
        tailwind: &tailwind,
        tolerance,
    };
    let rows: Vec<_> = fixtures.iter().map(|f| report::run_one(f, &cx)).collect();
    let failed = report::write(&out, &rows, tolerance).expect("write report");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
