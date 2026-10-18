use super::{Node, NodeKind, Value};
use std::collections::BTreeSet;

pub fn ids(root: &Node) -> BTreeSet<String> {
    fn visit(node: &Node, out: &mut BTreeSet<String>) {
        let mut value = |v: &Value| {
            if let Value::Id(name) = v {
                out.insert(name.clone());
            }
        };
        match &node.kind {
            NodeKind::Text(v) => value(v),
            NodeKind::Element {
                attributes,
                children,
                ..
            } => {
                for a in attributes {
                    value(&a.value);
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
