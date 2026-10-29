//! Directory traversal that skips build output.

use std::fs;
use std::path::{Path, PathBuf};

/// Visitor receiving `(dir, rs_files, subdirs)`.
pub type Visitor<'a> = dyn FnMut(&Path, &[PathBuf], &[PathBuf]) + 'a;

const SKIP: [&str; 3] = ["target", "runtime", "node_modules"];

/// Calls `f(dir, rs_files, subdirs)` for every directory under `root`.
pub fn visit(root: &Path, f: &mut Visitor<'_>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !SKIP.contains(&name.as_str()) && !name.starts_with('.') {
                dirs.push(path);
            }
        } else if name.ends_with(".rs") {
            files.push(path);
        }
    }
    files.sort();
    dirs.sort();
    f(root, &files, &dirs);
    for dir in &dirs {
        visit(dir, f);
    }
}
