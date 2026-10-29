//! Package fixtures and list evidence use separate, explicitly scheduled steps.
//! Cargo is invoked by the supervising runner, never by this test process.
use std::{collections::BTreeSet, env, fs, path::Path};

const CRATES: &[&str] = &["meari", "meari-native", "meari-builder"];
const LEGAL: &[&str] = &["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"];
const PRIVATE_DIRS: &[&str] = &[
    ".spec",
    "target",
    "node_modules",
    ".cache",
    "downloads",
    "captures",
    "screenshots",
    "cache",
];

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn package_license_and_notice_copies_preserve_original_bytes() {
    for name in CRATES {
        for legal in LEGAL {
            assert_eq!(
                fs::read(workspace().join("packages").join(name).join(legal)).unwrap(),
                fs::read(workspace().join(legal)).unwrap(),
                "{name}/{legal} differs from the existing library notice"
            );
        }
    }
}

#[test]
fn native_owned_counter_preserves_the_original_source_and_parse_result() {
    let core = fs::read(workspace().join("packages/meari/fixtures/state-counter.tsx")).unwrap();
    let native =
        fs::read(workspace().join("packages/meari-native/fixtures/state-counter.tsx")).unwrap();
    assert_eq!(native, core);
    meari::parse(
        std::str::from_utf8(&native).unwrap(),
        meari::application::Language::Tsx,
    )
    .expect("the packaged Native fixture must remain executable Meari source");
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink(), "package fixture refuses symlink inputs");
        let name = entry.file_name();
        if kind.is_dir() {
            assert!(
                !PRIVATE_DIRS.iter().any(|private| name == *private),
                "prepare only from the clean source overlay"
            );
            copy_tree(&entry.path(), &destination.join(name));
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), destination.join(name)).unwrap();
        }
    }
}

fn sentinel_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for root in ["src", "tests", "fixtures", "assets"] {
        for private in PRIVATE_DIRS {
            // The extension deliberately matches an otherwise permitted source/asset.
            let leaf = if root == "assets" {
                "private.svg"
            } else {
                "private.rs"
            };
            paths.push(format!("{root}/{private}/{leaf}"));
        }
        paths.push(format!("{root}/credentials.json"));
        paths.push(format!("{root}/private.pem"));
        paths.push(format!("{root}/capture.png"));
        paths.push(format!("{root}/.env.local"));
    }
    paths
}

#[test]
#[ignore = "Explicit fresh fixture preparation only; no Cargo or process launch"]
fn prepare_isolated_cargo_package_boundary_inputs() {
    let root = env::var_os("MEARI_PACKAGE_BOUNDARY_FIXTURE").expect("fresh absolute fixture path");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    // Existing directories are never reused or removed, including after a failed attempt.
    fs::create_dir(root).unwrap();
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "NOTICE",
    ] {
        fs::copy(workspace().join(file), root.join(file)).unwrap();
    }
    fs::create_dir(root.join("packages")).unwrap();
    for name in ["meari", "meari-native", "meari-builder", "meari-paint"] {
        copy_tree(
            &workspace().join("packages").join(name),
            &root.join("packages").join(name),
        );
    }
    for name in CRATES {
        let package = root.join("packages").join(name);
        for relative in sentinel_paths() {
            let path = package.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"private package boundary sentinel; not a credential").unwrap();
        }
    }
    fs::create_dir(root.join("package-lists")).unwrap();
}

fn package_inputs(root: &Path, current: &Path, files: &mut BTreeSet<String>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            package_inputs(root, &entry.path(), files);
        } else {
            assert!(kind.is_file());
            files.insert(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
        }
    }
}

#[test]
#[ignore = "Consumes actual runner-produced Cargo --list files after isolated preparation"]
fn actual_package_lists_preserve_sources_and_reject_private_sentinels() {
    let root =
        env::var_os("MEARI_PACKAGE_BOUNDARY_FIXTURE").expect("prepared absolute fixture path");
    let root = Path::new(&root);
    assert!(root.is_absolute());
    for name in CRATES {
        let text =
            fs::read_to_string(root.join("package-lists").join(format!("{name}.txt"))).unwrap();
        let listed: BTreeSet<_> = text.lines().map(|line| line.replace('\\', "/")).collect();
        assert!(!listed.is_empty(), "zero package paths is not acceptance");
        for private in sentinel_paths() {
            assert!(root.join("packages").join(name).join(&private).is_file());
            assert!(!listed.contains(&private), "{name} leaked {private}");
        }
        for legal in LEGAL {
            assert!(listed.contains(*legal), "{name} omitted {legal}");
        }
        assert!(listed.contains("Cargo.toml"));
        let package = workspace().join("packages").join(name);
        let mut inputs = BTreeSet::new();
        for directory in ["src", "tests", "fixtures", "assets"] {
            let directory = package.join(directory);
            if directory.is_dir() {
                package_inputs(&package, &directory, &mut inputs);
            }
        }
        for input in inputs {
            assert!(
                listed.contains(&input),
                "{name} omitted real package input {input}"
            );
        }
        if *name == "meari-native" {
            assert!(listed.contains("fixtures/state-counter.tsx"));
        }
    }
}
