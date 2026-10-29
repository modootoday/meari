//! Meari development tasks.
//! Usage: `cargo xtask lint-structure [--strict]` | `cargo xtask visual-test [--out DIR] [...]` | `cargo xtask spec-check [...]` | `cargo xtask demo [FIXTURE] [...]`.

mod clean_cache;
mod spec_check;
mod structure;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("lint-structure") => {
            let strict = args.iter().any(|a| a == "--strict");
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
            let report = structure::scan(&root.join("crates"));
            structure::print(&report);
            if strict && !report.is_empty() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Some("visual-test") => {
            // Native (offscreen CPU) vs Web (headless Chrome) pixel comparison; see crates/meari-visual.
            run_package("meari-visual", &args[1..])
        }
        Some("spec-check") => spec_check::run(&args[1..]),
        Some("clean-cache") => clean_cache::run(
            &args[1..],
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."),
        ),
        Some("demo") => run_package("meari-demo", &args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask lint-structure [--strict] | cargo xtask visual-test [--out DIR] | cargo xtask spec-check [ARGS] | cargo xtask clean-cache [--max-gb N] [--older-days D] [--dry-run] | cargo xtask demo [FIXTURE] [--list] [--seconds N]"
            );
            ExitCode::from(2)
        }
    }
}

/// `cargo run -p <package> -- <args>`; used by visual-test and the side-by-side demo.
fn run_package(package: &str, args: &[String]) -> ExitCode {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = std::process::Command::new(cargo)
        .args(["run", "-q", "-p", package, "--"])
        .args(args)
        .status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
