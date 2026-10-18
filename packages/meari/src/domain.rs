use serde::{Deserialize, Serialize};
pub mod design;
pub mod hooks;
pub mod validation;
pub mod viewport;

/// Half-open UTF-8 byte offsets in the original source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn unsupported(span: Span, message: impl Into<String>) -> Self {
        Self {
            code: "unsupported".into(),
            message: message.into(),
            span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropType {
    String,
    Bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prop {
    pub name: String,
    pub kind: PropType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    String(String),
    Bool(bool),
    Prop(String),
    /// Stable per-document useId binding, never a caller-provided prop.
    Id(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Element {
        tag: String,
        attributes: Vec<Attribute>,
        children: Vec<Node>,
    },
    Text(Value),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    /// React client entry boundary, preserved as metadata by both frontends.
    #[serde(default)]
    pub client: bool,
    pub name: String,
    pub props: Vec<Prop>,
    pub root: Node,
}

impl Component {
    /// Source formatting and locations are not component semantics.
    pub fn semantic_eq(&self, other: &Self) -> bool {
        fn erase(node: &mut Node) {
            node.span = Span::default();
            if let NodeKind::Element { children, .. } = &mut node.kind {
                children.iter_mut().for_each(erase);
            }
        }
        let (mut a, mut b) = (self.clone(), other.clone());
        erase(&mut a.root);
        erase(&mut b.root);
        a == b
    }
}
