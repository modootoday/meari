use crate as application;
use crate::{
    application::Language,
    domain::{Diagnostic, Span},
};
use std::{
    io::{self, Read},
    process::ExitCode,
};

fn execute(args: Vec<String>) -> Result<(), Diagnostic> {
    if args.as_slice() == ["--help"] {
        println!(
            "meari-cli <rust-to-tsx|tsx-to-rust> [--json]\nmeari-cli native-plan < request.json\nConversion schema2 includes code, mappings and tailwind-shadcn-v1 native capability diagnostics. native-plan returns a typed plan, not a rendered window.\nErrors: JSON on stderr, exit 1. See README.md."
        );
        return Ok(());
    }
    let error = |message: &str| Diagnostic {
        code: "input".into(),
        message: message.into(),
        span: Span::default(),
    };
    if args.as_slice() == ["native-plan"] {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| error(&e.to_string()))?;
        let request = serde_json::from_str(&input).map_err(|e| error(&e.to_string()))?;
        let plan = application::prepare_native(&request)?;
        println!(
            "{}",
            serde_json::to_string(&plan).map_err(|e| error(&e.to_string()))?
        );
        return Ok(());
    }
    let (from, to) = match args.first().map(String::as_str) {
        Some("rust-to-tsx") => (Language::Rust, Language::Tsx),
        Some("tsx-to-rust") => (Language::Tsx, Language::Rust),
        _ => return Err(error("expected rust-to-tsx or tsx-to-rust; use --help")),
    };
    if args.len() > 2 || args.get(1).is_some_and(|a| a != "--json") {
        return Err(error("only --json is supported"));
    }
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| error(&e.to_string()))?;
    let output = application::convert(&source, from, to)?;
    if args.len() == 2 {
        println!(
            "{}",
            serde_json::to_string(&output).map_err(|e| error(&e.to_string()))?
        );
    } else {
        print!("{}", output.code);
    }
    Ok(())
}

pub fn run() -> ExitCode {
    run_with(std::env::args().skip(1).collect())
}

/// Runs this command group with explicit arguments (used by the unified `meari` CLI).
pub fn run_with(args: Vec<String>) -> ExitCode {
    match execute(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string(&error).expect("diagnostic serialization")
            );
            ExitCode::FAILURE
        }
    }
}
