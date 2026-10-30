//! Unmodified vendored upstream crates (R-vendored).
//!
//! A crate directory with `VENDORED.sha256` holds upstream source copied byte for byte
//! (e.g. `crates/meari-shell` = crates.io `blitz-shell` 0.3.0-beta.4). Its `src/` is exempt
//! from the file-length and OS-cfg rules, because splitting files or moving `cfg`s would
//! make it a fork. The exemption holds only while every listed file matches its sha256 and
//! `src/` has no unlisted file; any drift is itself a violation and drops the exemption.
//! `scripts/vendored-upstream-diff.sh` re-checks the manifest against crates.io.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "VENDORED.sha256";

#[derive(Default)]
pub struct Vendored {
    /// Verified `src/` roots whose files are exempt.
    pub roots: Vec<PathBuf>,
    /// Drift messages (missing, changed or unlisted files; empty manifest).
    pub drift: Vec<String>,
}

impl Vendored {
    pub fn exempt(&self, file: &Path) -> bool {
        self.roots.iter().any(|root| file.starts_with(root))
    }
}

fn files_under(dir: &Path, base: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks one crate directory against its manifest text; returns drift messages.
pub fn verify(crate_dir: &Path, manifest: &str) -> Vec<String> {
    let mut drift = Vec::new();
    let mut listed = BTreeSet::new();
    for line in manifest
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let Some((hash, rel)) = line.split_once("  ") else {
            drift.push(format!("{}: malformed line {line:?}", crate_dir.display()));
            continue;
        };
        listed.insert(rel.to_string());
        match fs::read(crate_dir.join(rel)) {
            Ok(bytes) if sha256_hex(&bytes) == hash => {}
            Ok(_) => drift.push(format!("{}: {rel} changed", crate_dir.display())),
            Err(_) => drift.push(format!("{}: {rel} missing", crate_dir.display())),
        }
    }
    if listed.is_empty() {
        drift.push(format!(
            "{}: {MANIFEST} lists no files",
            crate_dir.display()
        ));
    }
    let mut actual = BTreeSet::new();
    files_under(&crate_dir.join("src"), crate_dir, &mut actual);
    for extra in actual.difference(&listed) {
        drift.push(format!(
            "{}: {extra} not in {MANIFEST}",
            crate_dir.display()
        ));
    }
    drift
}

/// Finds `crates/*/VENDORED.sha256` and verifies each.
pub fn load(crates: &Path) -> Vendored {
    let mut vendored = Vendored::default();
    let Ok(entries) = fs::read_dir(crates) else {
        return vendored;
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        let Ok(manifest) = fs::read_to_string(dir.join(MANIFEST)) else {
            continue;
        };
        let drift = verify(&dir, &manifest);
        if drift.is_empty() {
            vendored.roots.push(dir.join("src"));
        } else {
            vendored.drift.extend(drift);
        }
    }
    vendored
}

#[cfg(test)]
mod tests {
    use super::{load, sha256_hex};
    use std::fs;

    fn fixture(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("meari-vendored-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("v/src")).unwrap();
        fs::write(root.join("v/src/lib.rs"), "#[cfg(windows)]\nfn a() {}\n").unwrap();
        let line = format!(
            "{}  src/lib.rs\n",
            sha256_hex(b"#[cfg(windows)]\nfn a() {}\n")
        );
        fs::write(
            root.join("v/VENDORED.sha256"),
            format!("# upstream\n{line}"),
        )
        .unwrap();
        root
    }

    #[test]
    fn unchanged_source_is_exempt() {
        let root = fixture("ok");
        let v = load(&root);
        assert!(v.drift.is_empty(), "{:?}", v.drift);
        assert!(v.exempt(&root.join("v/src/lib.rs")));
        assert!(!v.exempt(&root.join("other/src/lib.rs")));
    }

    #[test]
    fn edited_or_added_file_is_drift_and_not_exempt() {
        let root = fixture("drift");
        fs::write(root.join("v/src/lib.rs"), "fn changed() {}\n").unwrap();
        fs::write(root.join("v/src/new.rs"), "\n").unwrap();
        let v = load(&root);
        assert_eq!(v.drift.len(), 2, "{:?}", v.drift);
        assert!(!v.exempt(&root.join("v/src/lib.rs")));
    }

    #[test]
    fn empty_manifest_is_drift() {
        let root = fixture("empty");
        fs::write(root.join("v/VENDORED.sha256"), "# nothing\n").unwrap();
        assert!(!load(&root).drift.is_empty());
    }
}
