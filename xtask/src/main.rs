//! Meari development tasks.
//! Usage: `cargo xtask lint-structure [--strict]` | `cargo xtask visual-test [--out DIR] [...]` | `cargo xtask spec-check [...]`.

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
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let status = std::process::Command::new(cargo)
                .args(["run", "-q", "-p", "meari-visual", "--"])
                .args(&args[1..])
                .status();
            match status {
                Ok(s) if s.success() => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            }
        }
        Some("spec-check") => spec_check::run(&args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask lint-structure [--strict] | cargo xtask visual-test [--out DIR] | cargo xtask spec-check [ARGS]"
            );
            ExitCode::from(2)
        }
    }
}
