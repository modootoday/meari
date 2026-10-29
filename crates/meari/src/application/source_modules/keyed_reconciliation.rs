//! Renderer-independent stable instance reconciliation policy.
use crate::domain::{Diagnostic, Span};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub instance: String,
    pub before: Option<String>,
    pub retained: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reconciliation {
    pub removed: Vec<String>,
    /// Apply in order, resolving anchors by stable identity rather than an obsolete index.
    pub placements: Vec<Placement>,
}

/// Prepare every operation before touching a renderer. This is not a DOM transaction.
pub fn plan(
    previous: &[String],
    next: &[String],
    span: Span,
) -> Result<Reconciliation, Diagnostic> {
    fn identities(values: &[String], span: Span) -> Result<BTreeSet<&str>, Diagnostic> {
        if values.len() > 16384 {
            return Err(Diagnostic::unsupported(
                span,
                "keyed child count exceeds budget",
            ));
        }
        let mut set = BTreeSet::new();
        for value in values {
            if value.is_empty() || value.len() > 256 || !set.insert(value.as_str()) {
                return Err(Diagnostic::unsupported(
                    span,
                    "invalid or duplicate keyed instance",
                ));
            }
        }
        Ok(set)
    }
    let old = identities(previous, span)?;
    let new = identities(next, span)?;
    let removed = previous
        .iter()
        .rev()
        .filter(|instance| !new.contains(instance.as_str()))
        .cloned()
        .collect();
    let mut before = None;
    let mut placements = Vec::with_capacity(next.len());
    // Every anchor has already been placed when the renderer applies this operation.
    for instance in next.iter().rev() {
        placements.push(Placement {
            instance: instance.clone(),
            before,
            retained: old.contains(instance.as_str()),
        });
        before = Some(instance.clone());
    }
    Ok(Reconciliation {
        removed,
        placements,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }
    fn apply(previous: &[String], operations: &Reconciliation) -> Vec<String> {
        let mut children = previous.to_vec();
        for removed in &operations.removed {
            let index = children.iter().position(|value| value == removed).unwrap();
            children.remove(index);
        }
        for place in &operations.placements {
            if place.retained {
                let index = children
                    .iter()
                    .position(|value| value == &place.instance)
                    .unwrap();
                children.remove(index);
            } else {
                assert!(!children.contains(&place.instance));
            }
            let index = place.before.as_ref().map_or(children.len(), |anchor| {
                children.iter().position(|value| value == anchor).unwrap()
            });
            children.insert(index, place.instance.clone());
        }
        children
    }
    #[test]
    fn ordered_operations_preserve_retained_identity_through_insert_delete_and_reorder() {
        let old = ids(&["a", "원문🦀", "b"]);
        for next in [
            ids(&[]),
            ids(&["a"]),
            old.clone(),
            ids(&["b", "new", "원문🦀", "a"]),
        ] {
            let operations = plan(&old, &next, Span::default()).unwrap();
            assert_eq!(apply(&old, &operations), next);
            for place in operations.placements {
                assert_eq!(place.retained, old.contains(&place.instance));
            }
        }
        let next = ids(&["a", "b"]);
        assert_eq!(
            apply(&[], &plan(&[], &next, Span::default()).unwrap()),
            next
        );
    }
    #[test]
    fn invalid_identity_lists_fail_before_returning_any_mutation_plan() {
        let span = Span {
            start: 538,
            end: 609,
        };
        for invalid in [
            ids(&["a", "a"]),
            ids(&[""]),
            vec!["x".repeat(257)],
            vec!["a".into(); 16385],
        ] {
            assert_eq!(plan(&[], &invalid, span).unwrap_err().span, span);
            assert_eq!(plan(&invalid, &[], span).unwrap_err().span, span);
        }
    }
}
