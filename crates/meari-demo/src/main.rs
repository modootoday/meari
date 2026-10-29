//! `cargo xtask demo [FIXTURE]`: show one TSX fixture rendered by meari-native and by the Web
//! DOM plan in Chrome, side by side, and switch fixtures from the terminal.
//!
//! The same binary re-executes itself as `meari-demo native-window ...` so the native window can
//! be replaced on every fixture switch without restarting the controller.
mod catalog;
mod cli;
mod control;
mod launch;
mod screen;
mod session;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("native-window") => launch::native::run_window(&args[1..]),
        _ => cli::Options::parse(&args).and_then(control::run),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("meari-demo: {error}");
            ExitCode::FAILURE
        }
    }
}
