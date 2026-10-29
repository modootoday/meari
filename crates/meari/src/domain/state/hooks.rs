use super::super::{Node, NodeKind, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum HookSlot {
    Id(String),
    State(String),
}

pub fn ids(root: &Node) -> BTreeSet<String> {
    fn visit(node: &Node, out: &mut BTreeSet<String>) {
        fn value(v: &Value, out: &mut BTreeSet<String>) {
            if let Value::Id(name) = v {
                out.insert(name.clone());
            }
            if let Value::NodeSlot(super::super::node_slots::NodeSlot::Content(nodes)) = v {
                for child in nodes {
                    visit(child, out);
                }
            }
        }
        match &node.kind {
            NodeKind::Text(Value::Repeat(repeat)) => visit(&repeat.template, out),
            NodeKind::Text(v) => value(v, out),
            NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for a in attributes {
                    value(&a.value, out);
                }
                for c in children {
                    visit(c, out);
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    visit(root, &mut out);
    out
}

/// Native documents contain one component instance; IDs are scoped to that document.
pub fn stable_id(binding: &str) -> String {
    format!("bridge-hook-{binding}")
}
