use super::CompilerRegistration;
use meari::{
    application::builder::BuildFailure,
    domain::build::{BuildPath, OutputRole},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
type Products = BTreeMap<BuildPath, (OutputRole, Vec<u8>)>;

fn command(registration: &CompilerRegistration, tool: &Path, directory: &Path) -> Command {
    let mut command = Command::new(tool);
    command
        .current_dir(directory)
        .env("RUSTC", &registration.rustc.path)
        .env("CARGO_TARGET_DIR", &registration.target_dir)
        .env("CARGO_BUILD_JOBS", "1")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET");
    command
}
fn run(mut command: Command, directory: &Path, label: &str) -> Result<(), BuildFailure> {
    let stdout = fs::File::create_new(directory.join(format!("{label}.stdout")))
        .map_err(|error| error.to_string())?;
    let stderr = fs::File::create_new(directory.join(format!("{label}.stderr")))
        .map_err(|error| error.to_string())?;
    let status = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!(
            "{label} failed ({status}); preserved logs: {}",
            directory.display()
        )
        .into());
    }
    Ok(())
}
pub(super) fn native(
    registration: &CompilerRegistration,
    directory: &Path,
    name: &str,
    triple: &str,
) -> Result<Products, BuildFailure> {
    let mut cargo = command(registration, &registration.cargo.path, directory);
    // Registration accepts only rustc's observed host triple. Use its host build cache.
    cargo.args([
        "build",
        "--offline",
        "--message-format=json-render-diagnostics",
        "-j1",
    ]);
    run(cargo, directory, "cargo-build")?;
    let log = fs::read_to_string(directory.join("cargo-build.stdout"))
        .map_err(|error| error.to_string())?;
    let mut executable = None;
    for line in log.lines() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
            && value["reason"] == "compiler-artifact"
            && value["target"]["name"] == name
            && let Some(path) = value["executable"].as_str()
        {
            executable = Some(PathBuf::from(path));
        }
    }
    let path =
        fs::canonicalize(executable.ok_or("Cargo did not report the application executable")?)
            .map_err(|error| error.to_string())?;
    if !path.starts_with(&registration.target_dir) {
        return Err("Cargo artifact escapes registered target directory".into());
    }
    let expected = registration
        .target_dir
        .join("debug")
        .join(if triple.contains("windows") {
            format!("{name}.exe")
        } else {
            name.to_owned()
        });
    if path != fs::canonicalize(expected).map_err(|error| error.to_string())? {
        return Err("Cargo output differs from the registered host/debug target".into());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let name = if triple.contains("windows") {
        "app.exe"
    } else {
        "app"
    };
    Ok(BTreeMap::from([(
        name.to_owned().try_into()?,
        (OutputRole::NativeExecutable, bytes),
    )]))
}
pub(super) fn web(
    registration: &CompilerRegistration,
    directory: &Path,
) -> Result<Products, BuildFailure> {
    let trunk = registration
        .trunk
        .as_ref()
        .ok_or("Trunk registration missing")?;
    let bindgen = registration
        .wasm_bindgen
        .as_ref()
        .ok_or("wasm-bindgen registration missing")?;
    trunk.verify()?;
    bindgen.verify()?;
    let version = bindgen
        .pin
        .version
        .strip_prefix("wasm-bindgen ")
        .ok_or("invalid wasm-bindgen version probe")?;
    fs::write(
        directory.join("Trunk.toml"),
        format!("[build]\nfilehash = false\n[tools]\nwasm_bindgen = {version:?}\n"),
    )
    .map_err(|error| error.to_string())?;
    let bindgen_dir = bindgen.path.parent().ok_or("wasm-bindgen parent missing")?;
    let cargo_dir = registration
        .cargo
        .path
        .parent()
        .ok_or("Cargo parent missing")?;
    let mut search = vec![bindgen_dir.to_path_buf(), cargo_dir.to_path_buf()];
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut build = command(registration, &trunk.path, directory);
    build
        .args([
            "build",
            "--offline",
            "true",
            "--skip-version-check",
            "--color",
            "never",
        ])
        .env_remove("NO_COLOR")
        .env(
            "PATH",
            std::env::join_paths(search).map_err(|error| error.to_string())?,
        );
    run(build, directory, "trunk-build")?;
    trunk.verify()?;
    bindgen.verify()?;
    let dist = directory.join("dist");
    let mut pending = vec![dist.clone()];
    let mut products = BTreeMap::new();
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if super::super::files::is_link(
                &fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?,
            ) {
                return Err("Web compiler emitted a link".into());
            }
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let file = entry.path();
            let relative = file
                .strip_prefix(&dist)
                .map_err(|error| error.to_string())?
                .to_str()
                .ok_or("Web output path is not UTF-8")?
                .replace('\\', "/");
            if relative.starts_with('.') {
                continue;
            }
            let role = match file.extension().and_then(|value| value.to_str()) {
                Some("wasm") => OutputRole::WasmModule,
                Some("js") => OutputRole::WebModule,
                Some("html") => OutputRole::Bootstrap,
                Some("css") => OutputRole::Stylesheet,
                _ => OutputRole::Asset,
            };
            // Source index.html belongs to the generated project; final bootstrap is distinct.
            products.insert(
                format!("web/{relative}").try_into()?,
                (role, fs::read(file).map_err(|error| error.to_string())?),
            );
        }
    }
    Ok(products)
}
