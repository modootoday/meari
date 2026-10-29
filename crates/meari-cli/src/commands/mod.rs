//! Subcommand routing for the unified `meari` binary.
use std::process::ExitCode;

const USAGE: &str = "meari <command> [args]\n\
  convert <rust-to-tsx|tsx-to-rust|native-plan> [...]  TSX/Rust conversion (was meari-cli)\n\
  native <verify|show|styles> [...]                     native renderer tools (was meari-native)\n\
  build <build|serve> [...]                             app builder (was meari-builder)\n\
  --version | --help\n\
Pass `--help` after a command group for its own usage.";

pub fn dispatch(args: Vec<String>) -> ExitCode {
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest = rest.to_vec();
    match command.as_str() {
        "convert" => meari::presentation::cli::run_with(rest),
        "native" => meari_native::presentation::cli::run_with(rest),
        "build" => meari_builder::presentation::run_with(rest),
        "--version" => {
            println!("meari {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "--help" | "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command `{other}`\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
