use serde::{Deserialize, Serialize};
pub mod array_value;
pub mod assets;
pub mod bool_expression;
pub mod build;
pub mod component_state;
pub mod design;
pub(crate) mod hook_value;
pub mod hooks;
pub mod host;
pub mod host_callbacks;
pub mod input_event;
pub mod member_value;
pub mod node_slots;
pub mod plugins;
mod prop_values;
pub mod radio_group;
pub mod range_parts;
pub mod record_value;
pub mod repeat;
pub mod source_modules;
pub mod source_schema;
pub mod state_expression;
pub mod state_program;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PropType {
    Node,
    String,
    Bool,
    Callback(host_callbacks::CallbackSignature),
    Array(Box<PropType>),
    /// Immutable host input shape; this is separate from a live hook heap object.
    Record(Vec<Prop>),
}

impl PartialEq for PropType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Node, Self::Node) | (Self::String, Self::String) | (Self::Bool, Self::Bool) => {
                true
            }
            (Self::Callback(left), Self::Callback(right)) => left == right,
            (Self::Array(left), Self::Array(right)) => left == right,
            (Self::Record(left), Self::Record(right)) => record_value::same_schema(left, right),
            _ => false,
        }
    }
}
impl Eq for PropType {}

impl PropType {
    pub fn contains_record(&self) -> bool {
        match self {
            Self::Record(_) => true,
            Self::Array(element) => element.contains_record(),
            _ => false,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        prop_values::validate_type(self)
    }
    pub fn accepts(&self, value: &Value) -> bool {
        if self.validate().is_err() {
            return false;
        }
        match (self, value) {
            (Self::Node, Value::NodeSlot(_)) => true,
            (Self::String, Value::String(_)) | (Self::Bool, Value::Bool(_)) => true,
            (Self::Callback(signature), Value::CallbackReference(reference)) => {
                *signature == reference.signature && reference.validate().is_ok()
            }
            (Self::Array(element), Value::Array(array)) => {
                array.validate().is_ok() && element.as_ref() == &array.element
            }
            (Self::Record(fields), Value::Record(record)) => {
                record.validate().is_ok() && record_value::same_schema(fields, record.schema())
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prop {
    pub name: String,
    pub kind: PropType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    NodeSlot(node_slots::NodeSlot),
    String(String),
    Bool(bool),
    Array(array_value::ArrayValue),
    Record(record_value::RecordValue),
    Member(member_value::MemberValue),
    Local(String),
    Repeat(Box<repeat::Repeat>),
    Prop(String),
    NotProp(String),
    Predicate(Box<bool_expression::BoolExpression>),
    /// Stable per-document useId binding, never a caller-provided prop.
    Id(String),
    State(String),
    Event(state_program::StateEvent),
    Input(input_event::InputStateBinding),
    Callback(host_callbacks::CallbackHandler),
    CallbackReference(host_callbacks::CallbackReference),
    BoundCallback(host_callbacks::BoundCallback),
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_order: Vec<hooks::HookSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<state_program::StateBinding>,
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
        fn erase_expression(expression: &mut state_expression::StateExpression) {
            use state_expression::StateExpression as E;
            match expression {
                E::Literal {
                    value: component_state::StateValue::Structured(expression),
                } => expression.erase_spans(),
                E::Unary { argument, .. } => erase_expression(argument),
                E::Binary { left, right, .. } => {
                    erase_expression(left);
                    erase_expression(right);
                }
                E::Conditional {
                    test,
                    consequent,
                    alternate,
                } => {
                    erase_expression(test);
                    erase_expression(consequent);
                    erase_expression(alternate);
                }
                _ => (),
            }
        }
        fn erase(node: &mut Node) {
            node.span = Span::default();
            if let NodeKind::Text(Value::Repeat(repeat)) = &mut node.kind {
                repeat.erase_spans();
            }
            if let NodeKind::Text(Value::NodeSlot(node_slots::NodeSlot::Content(nodes))) =
                &mut node.kind
            {
                nodes.iter_mut().for_each(erase);
            }
            if let NodeKind::Element { attributes, .. } = &mut node.kind {
                for attribute in attributes {
                    match &mut attribute.value {
                        Value::Event(state_program::StateEvent {
                            update:
                                state_program::StateUpdate::Set(
                                    component_state::StateValue::Structured(expression),
                                ),
                            ..
                        }) => expression.erase_spans(),
                        Value::Event(state_program::StateEvent {
                            update:
                                state_program::StateUpdate::Capture(expression)
                                | state_program::StateUpdate::Reduce(expression),
                            ..
                        }) => erase_expression(expression),
                        Value::Event(state_program::StateEvent {
                            update:
                                state_program::StateUpdate::HookCapture(expression)
                                | state_program::StateUpdate::HookReduce(expression),
                            ..
                        }) => expression.erase_spans(),
                        Value::Callback(handler) => handler.span = Span::default(),
                        Value::BoundCallback(handler) => handler.span = Span::default(),
                        Value::Predicate(expression) => expression.erase_spans(),
                        Value::NodeSlot(node_slots::NodeSlot::Content(nodes)) => {
                            nodes.iter_mut().for_each(erase)
                        }
                        _ => {}
                    }
                }
            }
            if let NodeKind::Element { children, .. } = &mut node.kind {
                children.iter_mut().for_each(erase);
            }
        }
        let (mut a, mut b) = (self.clone(), other.clone());
        erase(&mut a.root);
        erase(&mut b.root);
        for state in &mut a.states {
            state.span = Span::default();
            if let component_state::StateValue::Structured(expression) = &mut state.initial {
                expression.erase_spans();
            }
        }
        for state in &mut b.states {
            state.span = Span::default();
            if let component_state::StateValue::Structured(expression) = &mut state.initial {
                expression.erase_spans();
            }
        }
        a == b
    }
}
pub mod hook_expression;
pub mod range_interaction;
