//! Rule definitions and reporting.

use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_DIRECT_RS: usize = 8;
pub const MAX_FILE_LINES: usize = 600;
pub const FORBIDDEN: [&str; 5] = ["utils", "common", "misc", "helpers", "shared"];

#[derive(Default)]
pub struct Report {
    pub crowded_dirs: Vec<(PathBuf, usize)>,
    pub long_files: Vec<(PathBuf, usize)>,
    pub forbidden_names: Vec<PathBuf>,
    pub file_and_dir: Vec<PathBuf>,
    /// OS `cfg` outside `meari-platform` and outside test code (R-cfg).
    pub os_cfg: Vec<(PathBuf, usize)>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.crowded_dirs.is_empty()
            && self.long_files.is_empty()
            && self.forbidden_names.is_empty()
            && self.file_and_dir.is_empty()
            && self.os_cfg.is_empty()
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_forbidden(name: &str) -> bool {
    name.split(['_', '-']).any(|part| FORBIDDEN.contains(&part))
}

const OS_PREDICATES: [&str; 4] = ["windows", "unix", "target_os", "target_family"];

/// OS selection belongs to `meari-platform` (`CurrentOs`); test code (`tests/`, `tests.rs`,
/// `#[cfg(test)]` support `testing.rs`) may gate OS-only cases.
fn os_cfg_exempt(file: &Path) -> bool {
    file.components().any(|c| {
        let c = c.as_os_str();
        c == "meari-platform" || c == "tests" || c == "tests.rs" || c == "testing.rs"
    })
}

/// 1-based lines whose `cfg`/`cfg!`/`cfg_attr` names an OS predicate.
pub fn os_cfg_lines(file: &Path, text: &str) -> Vec<usize> {
    if os_cfg_exempt(file) {
        return Vec::new();
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or("");
            code.contains("cfg")
                && code
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|word| OS_PREDICATES.contains(&word))
        })
        .map(|(n, _)| n + 1)
        .collect()
}

pub fn check_dir(dir: &Path, files: &[PathBuf], subdirs: &[PathBuf], report: &mut Report) {
    if files.len() > MAX_DIRECT_RS {
        report.crowded_dirs.push((dir.to_path_buf(), files.len()));
    }
    for file in files {
        let lines = fs::read_to_string(file)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        if lines > MAX_FILE_LINES {
            report.long_files.push((file.clone(), lines));
        }
        if is_forbidden(&stem(file)) {
            report.forbidden_names.push(file.clone());
        }
        if let Ok(text) = fs::read_to_string(file) {
            for line in os_cfg_lines(file, &text) {
                report.os_cfg.push((file.clone(), line));
            }
        }
        let twin = dir.join(stem(file));
        if subdirs.contains(&twin) && !twin.join("mod.rs").exists() {
            report.file_and_dir.push(file.clone());
        }
    }
    for sub in subdirs {
        if is_forbidden(&stem(sub)) {
            report.forbidden_names.push(sub.clone());
        }
    }
}

pub fn print(report: &Report) {
    println!(
        "crowded dirs (> {MAX_DIRECT_RS} direct .rs): {}",
        report.crowded_dirs.len()
    );
    for (dir, n) in &report.crowded_dirs {
        println!("  {n:>4}  {}", dir.display());
    }
    println!(
        "long files (> {MAX_FILE_LINES} lines): {}",
        report.long_files.len()
    );
    for (file, n) in &report.long_files {
        println!("  {n:>5}  {}", file.display());
    }
    println!("forbidden names: {}", report.forbidden_names.len());
    for path in &report.forbidden_names {
        println!("  {}", path.display());
    }
    println!("OS cfg outside meari-platform: {}", report.os_cfg.len());
    for (file, line) in &report.os_cfg {
        println!("  {}:{line}", file.display());
    }
    println!("x.rs + x/ coexist: {}", report.file_and_dir.len());
    for path in &report.file_and_dir {
        println!("  {}", path.display());
    }
}

#[cfg(test)]
mod os_cfg_tests {
    use super::os_cfg_lines;
    use std::path::Path;

    #[test]
    fn flags_os_cfg_outside_platform_and_tests() {
        let text = "#[cfg(windows)]\nfn a() {}\n#[cfg(feature = \"gl\")]\nlet x = cfg!(target_os = \"macos\");\n";
        assert_eq!(
            os_cfg_lines(Path::new("crates/meari-native/src/a.rs"), text),
            [1, 4]
        );
        assert!(os_cfg_lines(Path::new("crates/meari-platform/src/lib.rs"), text).is_empty());
        assert!(os_cfg_lines(Path::new("crates/meari-native/tests/x/main.rs"), text).is_empty());
        assert!(os_cfg_lines(Path::new("crates/meari-native/src/a/tests.rs"), text).is_empty());
    }
}
