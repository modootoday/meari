//! `cargo xtask visual-test`: run every fixture through both renderers and write the report.
use meari_visual::{compare, fixtures, report};
use std::path::PathBuf;

fn main() {
    // Unmodified Chrome resolves generics to host Arial/Times substitutes; compare like for like.
    meari_native::infrastructure::engine::generic_fonts::use_chrome_comparison_profile();
    let mut args = std::env::args().skip(1);
    let repo_root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut out = PathBuf::from("target/meari-visual");
    let mut chrome = meari_visual::host::default_chrome();
    let mut tolerance = compare::Tolerance::default();
    let mut only: Option<String> = None;
    let mut gate = meari_visual::host::PixelGate::current();
    let mut tailwind = meari_visual::host::default_tailwind(&PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../.."
    )));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().expect("--out DIR").into(),
            "--only" => only = args.next(),
            "--gate" => {
                gate = args
                    .next()
                    .and_then(|v| meari_visual::host::PixelGate::parse(&v))
                    .expect("--gate strict|advisory")
            }
            "--fetch-fonts" => {
                let list = args.next().unwrap_or_else(|| "all".into());
                let scripts: Vec<&str> = list.split(',').collect();
                match meari_visual::fonts::ensure(&scripts, &repo_root) {
                    Ok(files) => {
                        for (pin, path) in files {
                            println!("ok {} {} {}", pin.script, pin.family, path.display());
                        }
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
            }
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
    let cache_root = repo_root.join("runtime/fonts");
    // SAFETY: single-threaded start-up, before any font context reads the cache location.
    unsafe { std::env::set_var(meari_native::infrastructure::fonts::CACHE_ENV, &cache_root) };
    let pinned = meari_visual::fonts::ensure(meari_visual::fonts::VISUAL_TEST_SCRIPTS, &repo_root)
        .and_then(|files| meari_visual::render::style::use_pinned_fonts(&files))
        .and_then(|()| meari_visual::render::style::use_canonical_faces(&repo_root));
    if let Err(e) = pinned {
        eprintln!("pinned fonts: {e}");
        std::process::exit(1);
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
    let failed = report::write(&out, &rows, tolerance, gate).expect("write report");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
