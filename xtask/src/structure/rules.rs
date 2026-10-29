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
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.crowded_dirs.is_empty()
            && self.long_files.is_empty()
            && self.forbidden_names.is_empty()
            && self.file_and_dir.is_empty()
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
    println!("x.rs + x/ coexist: {}", report.file_and_dir.len());
    for path in &report.file_and_dir {
        println!("  {}", path.display());
    }
}
