use crate::domain::Build;
use anyhow::Result;
use std::{fs, path::Path, process::Command, time::Instant};

pub fn build(
    root: &Path,
    generation: u32,
    schema: u32,
    wrong_abi: bool,
    syntax_error: bool,
) -> Result<Build> {
    let abi = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/domain/abi.rs")
        .canonicalize()?;
    let source = include_str!("../../plugin/ui.rs.in")
        .replace("@ABI_PATH@", &format!("{abi:?}"))
        .replace(
            "@ABI_VALUE@",
            if wrong_abi {
                "abi::ABI + 1"
            } else {
                "abi::ABI"
            },
        )
        .replace("@SCHEMA@", &schema.to_string())
        .replace("@GEN@", &generation.to_string())
        .replace("@FACTOR@", &(generation as i64 + 1).to_string());
    let source = if syntax_error {
        format!("{source}\nthis is an intentional compile error")
    } else {
        source
    };
    let input = root.join(format!("generation-{generation}.rs"));
    fs::write(&input, source)?;
    let output = root.join(format!("generation-{generation}.dll"));
    let started = Instant::now();
    let result = Command::new("rustc")
        .args([
            "--crate-type",
            "cdylib",
            "--edition=2024",
            "--crate-name",
            "ui_generation",
            "-C",
            "panic=abort",
        ])
        .arg(&input)
        .arg("-o")
        .arg(output)
        .output()?;
    Ok(Build {
        generation,
        elapsed_ms: started.elapsed().as_millis(),
        source_sha256: super::files::hash(&input)?,
        success: result.status.success(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    })
}
