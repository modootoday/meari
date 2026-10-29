//! `cargo xtask spec-check`: runs the specfold document checks (`spec check`).
//!
//! The `spec` binary comes from `cargo install specfold` or a release archive;
//! `MEARI_SPEC_BIN` overrides its path. Extra arguments are passed through.
use std::process::{Command, ExitCode};

const SPEC_BIN_ENV: &str = "MEARI_SPEC_BIN";

pub fn run(args: &[String]) -> ExitCode {
    let bin = std::env::var(SPEC_BIN_ENV).unwrap_or_else(|_| "spec".to_owned());
    match Command::new(&bin).arg("check").args(args).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("spec check failed ({status}); run `spec check` for details");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!(
                "cannot run `{bin}` ({error}); install specfold (`cargo install specfold`) or set {SPEC_BIN_ENV}"
            );
            ExitCode::FAILURE
        }
    }
}
