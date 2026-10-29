use meari::application::builder::BuildFailure;
use std::{io::Read, path::Path};

fn execute() -> Result<(), BuildFailure> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [argument] if argument == "--version" => {
            println!("meari-builder {}", env!("CARGO_PKG_VERSION"))
        }
        [argument] if argument == "--help" => println!(
            "meari-builder build REQUEST.json\nmeari-builder serve STORE_ROOT PORT SECONDS\nTrusted tool registration is separate from editable app inputs. Build publication is not an HMR frame acknowledgement."
        ),
        [command, path] if command == "build" => {
            let mut text = String::new();
            std::fs::File::open(path)
                .map_err(|error| error.to_string())?
                .take(16 * 1024 * 1024 + 1)
                .read_to_string(&mut text)
                .map_err(|error| error.to_string())?;
            if text.len() > 16 * 1024 * 1024 {
                return Err("build request exceeds size budget".into());
            }
            let request = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            let publication = meari_builder::application::build::execute(request)?;
            println!(
                "{}",
                serde_json::to_string(&publication).map_err(|error| error.to_string())?
            );
        }
        [command, store, port, seconds] if command == "serve" => {
            let port = port.parse().map_err(|_| "invalid server port")?;
            let seconds = seconds.parse().map_err(|_| "invalid server duration")?;
            meari_builder::infrastructure::server::serve(Path::new(store), port, seconds)?;
        }
        _ => return Err("expected build or serve; use --help".into()),
    }
    Ok(())
}
pub fn run() -> std::process::ExitCode {
    match execute() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string(&error).expect("build diagnostic serialization")
            );
            std::process::ExitCode::FAILURE
        }
    }
}
