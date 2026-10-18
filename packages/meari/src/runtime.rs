//! Pure construction API for generated Rust; rendering is an adapter concern.
pub use crate::domain::Node;
use crate::domain::{Attribute, NodeKind, Span, Value};

pub trait IntoValue {
    fn into_value(self) -> Value;
}
pub fn use_id(binding: &str) -> String {
    crate::domain::hooks::stable_id(binding)
}
impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}
impl IntoValue for &str {
    fn into_value(self) -> Value {
        Value::String(self.into())
    }
}
impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

pub fn text(value: impl Into<String>) -> Node {
    Node {
        kind: NodeKind::Text(Value::String(value.into())),
        span: Span::default(),
    }
}
pub fn attribute(name: &str, value: impl IntoValue) -> Attribute {
    Attribute {
        name: name.into(),
        value: value.into_value(),
    }
}
pub fn element(tag: &str, attributes: Vec<Attribute>, children: Vec<Node>) -> Node {
    Node {
        kind: NodeKind::Element {
            tag: tag.into(),
            attributes,
            children,
        },
        span: Span::default(),
    }
}
