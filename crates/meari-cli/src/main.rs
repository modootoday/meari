//! Unified `meari` binary. Subcommands delegate to each crate's presentation layer;
//! legacy binaries (`meari-cli`, `meari-native`, `meari-builder`) stay as compatible aliases.
mod commands;

fn main() -> std::process::ExitCode {
    commands::dispatch(std::env::args().skip(1).collect())
}
