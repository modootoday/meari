//! Regression: one `xtask` binary (as shared through a common Cargo target dir) must
//! report the workspace it is invoked from, not the checkout it was compiled in.
use std::{fs, path::Path, process::Command};

fn workspace(base: &Path, name: &str) -> std::path::PathBuf {
    let root = base.join(name);
    fs::create_dir_all(root.join("xtask/src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"xtask\"]\n",
    )
    .unwrap();
    fs::write(
        root.join("xtask/Cargo.toml"),
        "[package]\nname = \"xtask\"\n",
    )
    .unwrap();
    root.canonicalize().unwrap()
}

fn reported(cwd: &Path, manifest_dir: Option<&Path>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_meari-xtask"));
    command
        .arg("print-root")
        .current_dir(cwd)
        .env_remove("CARGO_MANIFEST_DIR");
    if let Some(dir) = manifest_dir {
        command.env("CARGO_MANIFEST_DIR", dir);
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn the_same_binary_reports_each_invoking_workspace_root() {
    let base = std::env::temp_dir().join(format!("xtask-runtime-root-{}", std::process::id()));
    let (a, b) = (workspace(&base, "a"), workspace(&base, "b"));
    for root in [&a, &b] {
        let expected = root.display().to_string();
        assert_eq!(reported(&root.join("xtask/src"), None), expected);
        assert_eq!(reported(&base, Some(&root.join("xtask"))), expected);
    }
    fs::remove_dir_all(&base).unwrap();
}
