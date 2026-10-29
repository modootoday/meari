//! Runtime workspace discovery. A shared Cargo target dir lets several checkouts run the
//! same `xtask` binary, so paths must never come from compile time (`env!`): they are
//! resolved per invocation from `cargo run`'s runtime `CARGO_MANIFEST_DIR`, else by walking
//! up from the current directory to the `[workspace]` manifest (as `cargo locate-project
//! --workspace` does).
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

/// Root of the workspace this invocation was started in.
pub fn root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("current dir: {e}"))?;
    root_from(std::env::var_os("CARGO_MANIFEST_DIR"), &cwd)
}

pub fn root_from(manifest_dir: Option<OsString>, cwd: &Path) -> Result<PathBuf, String> {
    let start = manifest_dir.map_or_else(|| cwd.to_path_buf(), PathBuf::from);
    start
        .ancestors()
        .find(|dir| is_workspace_root(dir))
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no [workspace] Cargo.toml above {}", start.display()))
}

fn is_workspace_root(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .is_ok_and(|body| body.lines().any(|l| l.trim() == "[workspace]"))
}

/// Cargo target dir: `CARGO_TARGET_DIR`, else `[build] target-dir` from the workspace's
/// `.cargo/config.toml` (relative to the workspace root, as Cargo resolves it), else `target`.
pub fn target_dir(root: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
        return root.join(dir);
    }
    let configured = fs::read_to_string(root.join(".cargo/config.toml"))
        .ok()
        .and_then(|body| configured_target_dir(&body));
    root.join(configured.unwrap_or_else(|| "target".to_owned()))
}

fn configured_target_dir(config: &str) -> Option<String> {
    let mut in_build = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_build = line == "[build]";
        } else if in_build && let Some(value) = line.strip_prefix("target-dir") {
            let value = value.trim_start().strip_prefix('=')?.trim();
            let value = value.split('#').next()?.trim();
            return Some(value.trim_matches('"').to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_dir_is_read_from_the_build_table_only() {
        let config = "[env]\ntarget-dir = \"wrong\"\n[build]\njobs = 1\ntarget-dir = \"../.cargo-target\" # shared\n";
        assert_eq!(
            configured_target_dir(config).as_deref(),
            Some("../.cargo-target")
        );
        assert_eq!(configured_target_dir("[build]\njobs = 1\n"), None);
    }

    #[test]
    fn root_walks_up_to_the_workspace_manifest() {
        let base = std::env::temp_dir().join(format!("xtask-root-{}", std::process::id()));
        let nested = base.join("xtask/src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(base.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        fs::write(
            base.join("xtask/Cargo.toml"),
            "[package]\nname = \"xtask\"\n",
        )
        .unwrap();
        assert_eq!(root_from(None, &nested).unwrap(), base);
        assert_eq!(
            root_from(Some(base.join("xtask").into()), Path::new("/")).unwrap(),
            base
        );
        fs::remove_dir_all(&base).unwrap();
    }
}
