//! Authoring subtrees and opaque graph-owned runtime slots, never HTML or host authority.
use super::super::{Node, Value, source_modules::ComponentReference};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeSlot {
    Empty,
    Content(Vec<Node>),
    #[serde(skip)]
    Owned(OwnedNodeSlot),
    #[serde(skip)]
    Captured(SlotReference),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedNodeSlot {
    pub(crate) owner: u64,
    pub(crate) nonce: u64,
    pub(crate) generation: u64,
    pub(crate) key: String,
    pub(crate) movable: bool,
    pub(crate) reference: ComponentReference,
    pub(crate) props: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SlotReference {
    pub(crate) owner: u64,
    pub(crate) identity: String,
}

/// Bound new recursive source values before recursive domain validation runs.
pub(crate) fn validate_tree(root: &Node) -> Result<(), String> {
    validate_tree_budget(root, 100000, 128)
}
fn validate_tree_budget(root: &Node, max_nodes: usize, max_depth: usize) -> Result<(), String> {
    fn slot<'a>(value: &'a Value, depth: usize, pending: &mut Vec<(&'a Node, usize)>) {
        match value {
            Value::NodeSlot(NodeSlot::Content(nodes)) => {
                pending.extend(nodes.iter().map(|node| (node, depth + 1)))
            }
            Value::Repeat(repeat) => pending.push((&repeat.template, depth + 1)),
            _ => {}
        }
    }
    let mut pending = vec![(root, 0usize)];
    let mut count = 0usize;
    while let Some((node, depth)) = pending.pop() {
        count += 1;
        if depth > max_depth || count > max_nodes {
            return Err("source node/slot depth or size budget exceeded".into());
        }
        match &node.kind {
            super::super::NodeKind::Text(value) => slot(value, depth, &mut pending),
            super::super::NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for attribute in attributes {
                    slot(&attribute.value, depth, &mut pending);
                }
                pending.extend(children.iter().map(|node| (node, depth + 1)));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_slot_size_and_depth_are_checked_before_recursive_visitors() {
        let leaf = crate::runtime::text("leaf");
        let content = Node {
            kind: super::super::super::NodeKind::Text(Value::NodeSlot(NodeSlot::Content(
                vec![leaf.clone(); 5],
            ))),
            span: Default::default(),
        };
        assert!(validate_tree_budget(&content, 4, 8).is_err());
        assert!(validate_tree_budget(&content, 6, 1).is_ok());
        let mut deep = leaf;
        for _ in 0..5 {
            deep = crate::runtime::element("div", vec![], vec![deep]);
        }
        assert!(validate_tree_budget(&deep, 10, 3).is_err());
        assert!(validate_tree_budget(&deep, 10, 5).is_ok());
    }
}
