//! Shared retained-child matching. Identity keys do not grant node-slot authority.
use super::keyed_reconciliation;
use crate::domain::{Diagnostic, Span};
use std::collections::BTreeMap;

/// Caller validates source/owned leases and recursively checks retained control shapes.
pub fn plan(
    previous: &[Option<&str>],
    next: &[Option<&str>],
    span: Span,
) -> Result<Vec<Option<usize>>, Diagnostic> {
    let keyed = previous.iter().chain(next).any(Option::is_some);
    if !keyed {
        if previous.len() != next.len() {
            return Err(Diagnostic::unsupported(
                span,
                "positional child count changed",
            ));
        }
        return Ok((0..next.len()).map(Some).collect());
    }
    fn keys(values: &[Option<&str>], span: Span) -> Result<Vec<String>, Diagnostic> {
        if values.len() > 16384 {
            return Err(Diagnostic::unsupported(
                span,
                "keyed child count exceeds budget",
            ));
        }
        values
            .iter()
            .flatten()
            .map(|value| {
                if value.is_empty() || value.len() > 256 {
                    return Err(Diagnostic::unsupported(
                        span,
                        "invalid or duplicate keyed instance",
                    ));
                }
                Ok((*value).to_owned())
            })
            .collect()
    }
    let before = keys(previous, span)?;
    let after = keys(next, span)?;
    keyed_reconciliation::plan(&before, &after, span)?;
    let positional = previous
        .iter()
        .enumerate()
        .filter_map(|(index, key)| key.is_none().then_some(index))
        .collect::<Vec<_>>();
    if positional.len() != next.iter().filter(|key| key.is_none()).count() {
        return Err(Diagnostic::unsupported(
            span,
            "unkeyed sibling count changed; an explicit source remount is required",
        ));
    }
    let indices = previous
        .iter()
        .enumerate()
        .filter_map(|(index, key)| key.map(|key| (key, index)))
        .collect::<BTreeMap<_, _>>();
    let mut positional = positional.into_iter();
    Ok(next
        .iter()
        .map(|key| match key {
            Some(key) => indices.get(key).copied(),
            None => positional.next(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_named_slots_keep_each_position_among_fixed_siblings() {
        let children = [None, Some("owned-editor"), None, Some("owned-inspector")];
        assert_eq!(
            plan(&children, &children, Span::default()).unwrap(),
            vec![Some(0), Some(1), Some(2), Some(3)]
        );
    }
    #[test]
    fn mixed_slots_move_replace_and_disappear_without_rebinding_fixed_siblings() {
        let previous = [None, Some("owned")];
        assert_eq!(
            plan(&previous, &[Some("owned"), None], Span::default()).unwrap(),
            vec![Some(1), Some(0)]
        );
        assert_eq!(
            plan(&previous, &[None, Some("replacement")], Span::default()).unwrap(),
            vec![Some(0), None]
        );
        assert_eq!(
            plan(&previous, &[None], Span::default()).unwrap(),
            vec![Some(0)]
        );
    }
    #[test]
    fn ambiguous_unkeyed_changes_and_duplicate_keys_still_fail() {
        let previous = [None, Some("owned")];
        for next in [
            vec![None, Some("owned"), None],
            vec![Some("owned"), Some("owned")],
        ] {
            assert!(plan(&previous, &next, Span::default()).is_err());
        }
    }
    #[test]
    fn empty_slot_insertion_removal_and_multiple_moves_use_original_indices() {
        assert_eq!(
            plan(&[None, None], &[None, Some("new"), None], Span::default()).unwrap(),
            vec![Some(0), None, Some(1)]
        );
        assert_eq!(
            plan(
                &[None, Some("a"), None, Some("b")],
                &[Some("b"), None, Some("a"), None],
                Span::default()
            )
            .unwrap(),
            vec![Some(3), Some(0), Some(1), Some(2)]
        );
        assert_eq!(
            plan(&[None, Some("a"), None], &[None, None], Span::default()).unwrap(),
            vec![Some(0), Some(2)]
        );
    }
    #[test]
    fn existing_keyed_reorder_insertion_and_removal_still_match_old_indices() {
        assert_eq!(
            plan(
                &[Some("a"), Some("b")],
                &[Some("b"), Some("new")],
                Span::default()
            )
            .unwrap(),
            vec![Some(1), None]
        );
        assert_eq!(
            plan(&[None, None], &[None, None], Span::default()).unwrap(),
            vec![Some(0), Some(1)]
        );
        assert!(plan(&[None], &[], Span::default()).is_err());
    }
    #[test]
    fn duplicate_empty_large_keys_and_child_budget_are_rejected() {
        for keys in [vec![None, Some("same"), Some("same")], vec![None, Some("")]] {
            assert!(plan(&keys, &keys, Span::default()).is_err());
        }
        let oversized = "x".repeat(257);
        assert!(
            plan(
                &[None, Some(&oversized)],
                &[None, Some(&oversized)],
                Span::default()
            )
            .is_err()
        );
        let oversized_children = vec![Some("key"); 16385];
        assert!(plan(&oversized_children, &oversized_children, Span::default()).is_err());
    }
}
