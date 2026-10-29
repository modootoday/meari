//! Meari development tasks. Usage: `cargo xtask lint-structure [--strict]`.

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
        _ => {
            eprintln!("usage: cargo xtask lint-structure [--strict]");
            ExitCode::from(2)
        }
    }
}
