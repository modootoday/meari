//! `cargo xtask clean-cache [--max-gb N] [--older-days D] [--dry-run]`
//!
//! Shrinks the Cargo target directory without touching sources: removes
//! `incremental/`, `deps/` files not written for `--older-days` (default 7), then the oldest `deps/` files until the
//! target fits `--max-gb` (default 40). Cargo rebuilds anything removed. Run it
//! between builds, never while one is running.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

struct Options {
    max_bytes: u64,
    older: Duration,
    dry_run: bool,
}

pub fn run(args: &[String], root: &Path) -> ExitCode {
    let Some(options) = parse(args) else {
        eprintln!("usage: cargo xtask clean-cache [--max-gb N] [--older-days D] [--dry-run]");
        return ExitCode::from(2);
    };
    let target = crate::workspace::target_dir(root);
    let before = size(&target);
    let mut cleaner = Cleaner {
        dry_run: options.dry_run,
        freed: 0,
    };
    let profiles = profiles(&target);
    for profile in &profiles {
        cleaner.remove(&profile.join("incremental"));
    }
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for profile in &profiles {
        files.extend(entries(&profile.join("deps")));
    }
    let cutoff = SystemTime::now() - options.older;
    files.sort_by_key(|(time, ..)| *time);
    let mut current = before.saturating_sub(cleaner.freed);
    for (time, len, path) in files {
        if time >= cutoff && current <= options.max_bytes {
            break;
        }
        cleaner.remove(&path);
        current = current.saturating_sub(len);
    }
    let gb = |b: u64| b as f64 / 1e9;
    println!(
        "{} {}: {:.1} GB -> {:.1} GB (freed {:.1} GB)",
        if options.dry_run {
            "would clean"
        } else {
            "cleaned"
        },
        target.display(),
        gb(before),
        gb(before.saturating_sub(cleaner.freed)),
        gb(cleaner.freed)
    );
    ExitCode::SUCCESS
}

fn parse(args: &[String]) -> Option<Options> {
    let mut options = Options {
        max_bytes: 40_000_000_000,
        older: Duration::from_secs(7 * 86_400),
        dry_run: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--max-gb" => options.max_bytes = (iter.next()?.parse::<f64>().ok()? * 1e9) as u64,
            "--older-days" => {
                options.older =
                    Duration::from_secs_f64(iter.next()?.parse::<f64>().ok()? * 86_400.0)
            }
            "--dry-run" => options.dry_run = true,
            _ => return None,
        }
    }
    Some(options)
}

/// Profile directories (`debug`, `release`, `<triple>/debug`, ...) holding `deps/`.
fn profiles(target: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in subdirs(target) {
        if dir.join("deps").is_dir() {
            found.push(dir);
        } else {
            found.extend(
                subdirs(&dir)
                    .into_iter()
                    .filter(|d| d.join("deps").is_dir()),
            );
        }
    }
    found
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn entries(dir: &Path) -> Vec<(SystemTime, u64, PathBuf)> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect()
}

fn size(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| size(&e.path()))
            .sum(),
        Ok(meta) => meta.len(),
        Err(_) => 0,
    }
}

struct Cleaner {
    dry_run: bool,
    freed: u64,
}

impl Cleaner {
    fn remove(&mut self, path: &Path) {
        let len = size(path);
        if len == 0 && !path.exists() {
            return;
        }
        let removed = self.dry_run
            || if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            }
            .is_ok();
        if removed {
            self.freed += len;
        }
    }
}
