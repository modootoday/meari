//! Component semantics -> HTML DOM. CSS utility syntax is kept intact for Tailwind/Stylo.
pub(crate) mod composition;
mod mount;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Element {
    pub path: String,
    pub aliases: Vec<String>,
    pub dom_tag: String,
    pub id: String,
    pub attributes: BTreeMap<String, String>,
    pub text_override: Option<String>,
    pub text_runs: Vec<TextRun>,
    pub preserve_dirty_value: bool,
    pub controlled_checked: Option<bool>,
}
impl Element {
    pub fn owns_path(&self, path: &str) -> bool {
        self.path == path || self.aliases.iter().any(|alias| alias == path)
    }
    pub(crate) fn same_composition(&self, next: &Self) -> bool {
        self.dom_tag == next.dom_tag
            && self
                .aliases
                .iter()
                .map(|alias| alias.strip_prefix(&self.path))
                .eq(next
                    .aliases
                    .iter()
                    .map(|alias| alias.strip_prefix(&next.path)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    pub before: Option<String>,
    pub text: String,
}

pub struct Markup {
    pub html: String,
    pub classes: String,
    pub elements: Vec<Element>,
    panels: BTreeMap<(String, String), String>,
    force_mounted: BTreeSet<String>,
}

mod panels;
pub use self::panels::*;
mod visitor;
use self::visitor::*;
mod attributes;
pub use self::attributes::*;
