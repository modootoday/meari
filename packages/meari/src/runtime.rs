//! Pure construction API for generated Rust; rendering is an adapter concern.
pub use crate::domain::Node;
pub use crate::domain::node_slots::NodeSlot;
pub fn empty_node_slot() -> NodeSlot {
    NodeSlot::Empty
}
pub fn node_slot(nodes: Vec<Node>) -> NodeSlot {
    NodeSlot::Content(nodes)
}
impl IntoValue for NodeSlot {
    fn into_value(self) -> Value {
        Value::NodeSlot(self)
    }
}
use crate::domain::{Attribute, NodeKind, Span, Value};
pub type RenderResult = Result<Node, String>;

#[derive(Clone)]
pub struct ActionCallback(crate::domain::host_callbacks::CallbackReference);
#[derive(Clone)]
pub struct TextCallback(crate::domain::host_callbacks::CallbackReference);
#[derive(Clone)]
pub struct BooleanCallback(crate::domain::host_callbacks::CallbackReference);
impl BooleanCallback {
    pub fn new(binding: impl Into<String>) -> Result<Self, String> {
        let reference = crate::domain::host_callbacks::CallbackReference {
            binding: binding.into(),
            signature: crate::domain::host_callbacks::CallbackSignature::Boolean,
        };
        reference.validate()?;
        Ok(Self(reference))
    }
}
pub fn host_boolean(callback: &BooleanCallback) -> Value {
    Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
        reference: callback.0.clone(),
        source: crate::domain::host_callbacks::CallbackSource::BooleanValue,
        span: Span::default(),
        captured_text: None,
    })
}
impl IntoValue for BooleanCallback {
    fn into_value(self) -> Value {
        Value::CallbackReference(self.0)
    }
}
impl ActionCallback {
    pub fn new(binding: impl Into<String>) -> Result<Self, String> {
        let reference = crate::domain::host_callbacks::CallbackReference {
            binding: binding.into(),
            signature: crate::domain::host_callbacks::CallbackSignature::Action,
        };
        reference.validate()?;
        Ok(Self(reference))
    }
}
impl TextCallback {
    pub fn new(binding: impl Into<String>) -> Result<Self, String> {
        let reference = crate::domain::host_callbacks::CallbackReference {
            binding: binding.into(),
            signature: crate::domain::host_callbacks::CallbackSignature::Text,
        };
        reference.validate()?;
        Ok(Self(reference))
    }
}
pub fn host_action(callback: &ActionCallback) -> Value {
    Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
        reference: callback.0.clone(),
        source: crate::domain::host_callbacks::CallbackSource::Action,
        span: Span::default(),
        captured_text: None,
    })
}
pub fn host_text(callback: &TextCallback) -> Value {
    Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
        reference: callback.0.clone(),
        source: crate::domain::host_callbacks::CallbackSource::CurrentTargetText,
        span: Span::default(),
        captured_text: None,
    })
}
pub fn host_selection(callback: &TextCallback) -> Value {
    Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
        reference: callback.0.clone(),
        source: crate::domain::host_callbacks::CallbackSource::SelectionValue,
        span: Span::default(),
        captured_text: None,
    })
}
pub fn host_text_capture(callback: &TextCallback, value: &str) -> Value {
    Value::BoundCallback(crate::domain::host_callbacks::BoundCallback {
        reference: callback.0.clone(),
        source: crate::domain::host_callbacks::CallbackSource::CapturedText,
        span: Span::default(),
        captured_text: Some(value.to_owned()),
    })
}
impl IntoValue for ActionCallback {
    fn into_value(self) -> Value {
        Value::CallbackReference(self.0)
    }
}
impl IntoValue for TextCallback {
    fn into_value(self) -> Value {
        Value::CallbackReference(self.0)
    }
}

pub struct StateContext {
    pub snapshot: crate::application::component_state::StateCommit,
}

pub fn state_declaration(
    context: &StateContext,
    name: &str,
    _setter: &str,
    initial: &str,
) -> Result<(), String> {
    let initial: crate::domain::component_state::StateValue =
        serde_json::from_str(initial).map_err(|error| error.to_string())?;
    if context
        .snapshot
        .values
        .get(name)
        .is_none_or(|value| value.kind() != initial.kind())
    {
        return Err(format!("state context has no matching slot: {name}"));
    }
    Ok(())
}

pub fn state_read(context: &StateContext, name: &str) -> Result<Value, String> {
    crate::domain::state_program::render_value(
        context
            .snapshot
            .values
            .get(name)
            .ok_or("missing state slot")?,
    )
}

pub fn state_event(source: &str) -> Result<Value, String> {
    let event: crate::domain::state_program::StateEvent =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    if matches!(
        event.update,
        crate::domain::state_program::StateUpdate::Capture(_)
            | crate::domain::state_program::StateUpdate::Reduce(_)
            | crate::domain::state_program::StateUpdate::HookCapture(_)
            | crate::domain::state_program::StateUpdate::HookReduce(_)
    ) {
        return Err("state expression event requires state_captured_event".into());
    }
    Ok(Value::Event(event))
}

pub fn state_bool(context: &StateContext, name: &str) -> Result<bool, String> {
    match context.snapshot.values.get(name) {
        Some(crate::domain::component_state::StateValue::Bool(value)) => Ok(*value),
        _ => Err("boolean state slot missing or has a different type".into()),
    }
}

pub fn input_event(source: &str) -> Result<Value, String> {
    serde_json::from_str(source)
        .map(Value::Input)
        .map_err(|error| error.to_string())
}

pub fn state_captured_event(context: &StateContext, source: &str) -> Result<Value, String> {
    let event: crate::domain::state_program::StateEvent =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    Ok(Value::Event(event.capture(&context.snapshot.values)?))
}

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
impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

pub use array::Element as PropElement;

mod array {
    use super::{ActionCallback, BooleanCallback, IntoValue, TextCallback};
    use crate::domain::host_callbacks::CallbackSignature;
    use crate::domain::{PropType, Value, array_value::ArrayValue};

    pub trait Element: IntoValue {
        fn kind() -> PropType;
    }
    impl Element for String {
        fn kind() -> PropType {
            PropType::String
        }
    }
    impl Element for &str {
        fn kind() -> PropType {
            PropType::String
        }
    }
    impl Element for bool {
        fn kind() -> PropType {
            PropType::Bool
        }
    }
    impl Element for ActionCallback {
        fn kind() -> PropType {
            PropType::Callback(CallbackSignature::Action)
        }
    }
    impl Element for TextCallback {
        fn kind() -> PropType {
            PropType::Callback(CallbackSignature::Text)
        }
    }
    impl Element for BooleanCallback {
        fn kind() -> PropType {
            PropType::Callback(CallbackSignature::Boolean)
        }
    }
    impl<T: Element> Element for Vec<T> {
        fn kind() -> PropType {
            PropType::Array(Box::new(T::kind()))
        }
    }
    impl<T: Element> IntoValue for Vec<T> {
        fn into_value(self) -> Value {
            Value::Array(ArrayValue {
                element: T::kind(),
                items: self.into_iter().map(IntoValue::into_value).collect(),
            })
        }
    }
}

pub fn text(value: impl IntoValue) -> Node {
    Node {
        kind: NodeKind::Text(value.into_value()),
        span: Span::default(),
    }
}
pub fn local(name: &str) -> Value {
    Value::Local(name.into())
}
pub fn member(root: impl IntoValue, path: Vec<&str>) -> Value {
    Value::Member(crate::domain::member_value::MemberValue {
        root: Box::new(root.into_value()),
        path: path.into_iter().map(str::to_owned).collect(),
    })
}
pub fn repeat_member(source: impl IntoValue, item: &str, path: Vec<&str>, template: Node) -> Node {
    let mut node = repeat(source, item, item, template);
    if let NodeKind::Text(Value::Repeat(repeat)) = &mut node.kind {
        repeat.key_path = path.into_iter().map(str::to_owned).collect();
    }
    node
}
pub fn repeat(source: impl IntoValue, item: &str, key: &str, template: Node) -> Node {
    Node {
        kind: NodeKind::Text(Value::Repeat(Box::new(crate::domain::repeat::Repeat {
            source: source.into_value(),
            item: item.into(),
            key: key.into(),
            key_path: Vec::new(),
            template,
            source_span: Span::default(),
            key_span: Span::default(),
        }))),
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
