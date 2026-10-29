//! A value-only transaction retains an existing live export, never renews its lease.
use super::DocumentCallbacks;
use crate::domain::{Component, Node, NodeKind, Value, host_callbacks::BoundCallback};
use std::collections::{BTreeMap, BTreeSet};

fn erase_display_text(root: &mut Node, paths: &BTreeSet<String>) -> Result<(), String> {
    if paths.len() > 64 {
        return Err("retained range display path budget exceeded".into());
    }
    for path in paths {
        if path.len() > 1024 {
            return Err("retained range display path is too long".into());
        }
        let mut parts = path.split('/');
        if parts.next() != Some("0") {
            return Err("retained range display path requires root 0".into());
        }
        let mut node = &mut *root;
        for part in parts {
            let index: usize = part
                .parse()
                .map_err(|_| "retained range display index invalid")?;
            if index.to_string() != part {
                return Err("retained range display index is not canonical".into());
            }
            let NodeKind::Element { children, .. } = &mut node.kind else {
                return Err("retained range display path crosses a text node".into());
            };
            node = children
                .get_mut(index)
                .ok_or("retained range display node missing")?;
        }
        let NodeKind::Text(Value::String(text)) = &mut node.kind else {
            return Err(
                "retained range display path must select an existing string text node".into(),
            );
        };
        text.clear();
    }
    Ok(())
}

fn erase_range_values(node: &mut Node) {
    if let NodeKind::Element {
        tag,
        attributes,
        children,
    } = &mut node.kind
    {
        let range = ["input", "Input"].contains(&tag.as_str())
            && attributes.iter().any(|attribute| {
                attribute.name == "type" && attribute.value == Value::String("range".into())
            });
        if range {
            for attribute in attributes {
                if ["value", "aria-valuenow"].contains(&attribute.name.as_str())
                    && let Value::String(value) = &mut attribute.value
                {
                    value.clear();
                }
            }
        }
        for child in children {
            erase_range_values(child);
        }
    }
}

impl DocumentCallbacks {
    /// Only range value/aria-valuenow strings may change. Node shape, callbacks,
    /// captures, disabled state and all other declarations must remain identical.
    pub fn validate_retained_range_props(
        &self,
        previous: &Component,
        next: &Component,
        handlers: &BTreeMap<String, BoundCallback>,
    ) -> Result<(), String> {
        self.validate_retained_range_props_with_text(previous, next, handlers, &BTreeSet::new())
    }
    /// Explicit display paths permit only existing string text content to change.
    /// A path never relaxes callback captures, node identity or control attributes.
    pub fn validate_retained_range_props_with_text(
        &self,
        previous: &Component,
        next: &Component,
        handlers: &BTreeMap<String, BoundCallback>,
        display_text_paths: &BTreeSet<String>,
    ) -> Result<(), String> {
        if handlers.len() != self.entries.len() {
            return Err("retained range props changed callback inventory".into());
        }
        for (path, entry) in &self.entries {
            if handlers.get(path) != Some(&entry.handler) {
                return Err("retained range props changed callback authority or capture".into());
            }
            self.bindings
                .resolve(&entry.handler.reference)
                .map_err(|error| error.message)?;
        }
        let mut previous = previous.clone();
        let mut next = next.clone();
        erase_range_values(&mut previous.root);
        erase_range_values(&mut next.root);
        erase_display_text(&mut previous.root, display_text_paths)?;
        erase_display_text(&mut next.root, display_text_paths)?;
        if !previous.semantic_eq(&next) {
            return Err("retained range props require value-only unchanged controls".into());
        }
        Ok(())
    }
}
