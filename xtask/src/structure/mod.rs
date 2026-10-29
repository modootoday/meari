//! Structure lint: convention rules from kinekino-redesign (shared with meari).

mod rules;
mod walk;

pub use rules::{Report, print};

use std::path::Path;

/// Scan a source root and collect every rule violation.
pub fn scan(root: &Path) -> Report {
    let mut report = Report::default();
    walk::visit(root, &mut |dir, files, subdirs| {
        rules::check_dir(dir, files, subdirs, &mut report);
    });
    report
}
