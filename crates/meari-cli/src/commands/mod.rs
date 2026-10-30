//! Subcommand routing for the unified `meari` binary.
use std::process::ExitCode;

const USAGE: &str = "meari <command> [args]\n\
  convert <rust-to-tsx|tsx-to-rust|native-plan> [...]  TSX/Rust conversion (was meari-cli)\n\
  native <verify|show|styles> [...]                     native renderer tools (was meari-native)\n\
  build <build|serve> [...]                             app builder (was meari-builder)\n\
  fonts fetch [all|latin,hangul,kana,...]               download the pinned Noto fonts (default all)\n\
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
        "fonts" => fonts(&rest),
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

/// `meari fonts fetch [scripts]`: the one explicit download of the pinned, sha256-verified Noto
/// files into the font cache (`$MEARI_FONT_CACHE`, else the nearest `runtime/fonts`). Apps never
/// download at run time; without a cache they print one line pointing here.
fn fonts(args: &[String]) -> ExitCode {
    let ([sub] | [sub, _]) = args else {
        eprintln!("usage: meari fonts fetch [all|latin,hangul,kana,arabic,hebrew,devanagari,thai]");
        return ExitCode::from(2);
    };
    if sub != "fetch" {
        eprintln!("unknown fonts command `{sub}`; expected `fetch`");
        return ExitCode::from(2);
    }
    let list = args.get(1).map_or("all", String::as_str);
    let scripts: Vec<&str> = list.split(',').collect();
    let cache = meari_native::infrastructure::fonts::FontCache::default_location();
    match cache.ensure(&scripts) {
        Ok(files) => {
            for (pin, path) in files {
                println!("ok {} {} {}", pin.script, pin.family, path.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
