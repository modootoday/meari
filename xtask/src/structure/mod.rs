//! Structure lint: convention rules from kinekino-redesign (shared with meari).

mod rules;
mod vendored;
mod walk;

pub use rules::{Report, print};

use std::path::Path;

/// Scan a source root and collect every rule violation.
pub fn scan(root: &Path) -> Report {
    let vendored = vendored::load(root);
    let mut report = Report {
        vendored_drift: vendored.drift.clone(),
        vendored_exempt: vendored.roots.clone(),
        ..Report::default()
    };
    walk::visit(root, &mut |dir, files, subdirs| {
        rules::check_dir(dir, files, subdirs, &vendored, &mut report);
    });
    report
}
