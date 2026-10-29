//! Whether a component tag is referenced anywhere in a lowered tree.
use crate::domain::{Node, NodeKind, Value, node_slots};

pub(super) fn uses(node: &Node, name: &str) -> bool {
    fn slot(value: &Value, name: &str) -> bool {
        matches!(value,Value::NodeSlot(node_slots::NodeSlot::Content(nodes)) if nodes.iter().any(|node|uses(node,name)))
    }
    match &node.kind {
        NodeKind::Text(value) if slot(value, name) => true,
        NodeKind::Text(Value::Repeat(repeat)) => uses(&repeat.template, name),
        NodeKind::Element {
            tag,
            children,
            attributes,
        } => {
            tag == name
                || attributes
                    .iter()
                    .any(|attribute| slot(&attribute.value, name))
                || children.iter().any(|child| uses(child, name))
        }
        _ => false,
    }
}
