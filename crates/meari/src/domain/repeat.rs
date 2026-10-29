//! Source-preserving keyed Array.map expression; expansion belongs to runtime projection.
use super::{Component, Diagnostic, Node, NodeKind, Prop, PropType, Span, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repeat {
    pub source: Value,
    pub item: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_path: Vec<String>,
    pub template: Node,
    pub source_span: Span,
    pub key_span: Span,
}

pub fn map_values(node: &mut Node, map: &mut impl FnMut(&mut Value)) {
    fn map_value(binding: &mut Value, map: &mut impl FnMut(&mut Value)) {
        if let Value::Member(member) = binding {
            map(&mut member.root);
        }
        map(binding);
    }
    match &mut node.kind {
        NodeKind::Text(value) => {
            if let Value::NodeSlot(super::node_slots::NodeSlot::Content(nodes)) = value {
                for child in nodes {
                    map_values(child, map);
                }
            }
            if let Value::Repeat(repeat) = value {
                map_value(&mut repeat.source, map);
                map_values(&mut repeat.template, map);
            }
            if let Value::Member(member) = value {
                map(&mut member.root);
            }
            map(value);
        }
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                if let Value::NodeSlot(super::node_slots::NodeSlot::Content(nodes)) =
                    &mut attribute.value
                {
                    for child in nodes {
                        map_values(child, map);
                    }
                }
                map_value(&mut attribute.value, map);
            }
            for child in children {
                map_values(child, map);
            }
        }
    }
}

/// A nested arrow may shadow the outer binding; its source is outside that new scope.
pub fn substitute_local(node: &mut Node, name: &str, replacement: &Value) {
    fn replace(value: &mut Value, name: &str, replacement: &Value) {
        if let Value::Predicate(expression) = value {
            expression.substitute_local(name, replacement);
        }
        if let Value::NodeSlot(super::node_slots::NodeSlot::Content(nodes)) = value {
            for node in nodes {
                substitute_local(node, name, replacement);
            }
        }
        if let Value::Member(member) = value
            && matches!(member.root.as_ref(), Value::Local(binding) if binding == name)
        {
            *member.root = replacement.clone();
        }
        if matches!(value, Value::Local(binding) if binding == name) {
            *value = replacement.clone();
        }
    }
    match &mut node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => {
            replace(&mut repeat.source, name, replacement);
            if repeat.item != name {
                substitute_local(&mut repeat.template, name, replacement);
            }
        }
        NodeKind::Text(value) => replace(value, name, replacement),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                replace(&mut attribute.value, name, replacement);
            }
            for child in children {
                substitute_local(child, name, replacement);
            }
        }
    }
}

impl Repeat {
    pub fn validate(&self, component: &Component) -> Result<(), Diagnostic> {
        let fail = |span, message| Diagnostic::unsupported(span, message);
        let mut pending = vec![(&self.template, 1usize)];
        let mut count = 0usize;
        while let Some((node, depth)) = pending.pop() {
            count += 1;
            if depth > 32 || count > 16384 {
                return Err(fail(
                    self.source_span,
                    "repeat template exceeds depth/node budget",
                ));
            }
            match &node.kind {
                NodeKind::Text(Value::Repeat(repeat)) => {
                    pending.push((&repeat.template, depth + 1))
                }
                NodeKind::Element { children, .. } => {
                    pending.extend(children.iter().map(|child| (child, depth + 1)))
                }
                _ => {}
            }
        }
        if !matches!(self.template.kind, NodeKind::Element { .. }) {
            return Err(fail(self.key_span, "repeat template requires an element"));
        }
        if self.item != self.key || self.item == "props" || !identifier(&self.item) {
            return Err(fail(
                self.key_span,
                "repeat requires a lexical string item key",
            ));
        }
        let element = match &self.source {
            Value::Prop(name) => match component.props.iter().find(|prop| prop.name == *name) {
                Some(Prop {
                    kind: PropType::Array(element),
                    ..
                }) => element.as_ref().clone(),
                _ => {
                    return Err(fail(
                        self.source_span,
                        "repeat source requires a declared Array prop",
                    ));
                }
            },
            Value::Array(array) => {
                array
                    .validate()
                    .map_err(|message| Diagnostic::unsupported(self.source_span, message))?;
                array.element.clone()
            }
            Value::Member(member) => match member
                .kind(&component.props)
                .map_err(|message| Diagnostic::unsupported(self.source_span, message))?
            {
                PropType::Array(element) => *element,
                _ => {
                    return Err(fail(
                        self.source_span,
                        "repeat member source requires an Array field",
                    ));
                }
            },
            _ => {
                return Err(fail(
                    self.source_span,
                    "repeat source requires a typed Array value",
                ));
            }
        };
        let key_type = if self.key_path.is_empty() {
            element.clone()
        } else {
            super::member_value::MemberValue::field_type(element.clone(), &self.key_path)
                .map_err(|message| Diagnostic::unsupported(self.key_span, message))?
        };
        if key_type != PropType::String {
            return Err(fail(
                self.key_span,
                "repeat key requires a declared string item or string record field",
            ));
        }
        let mut scoped = component.clone();
        let mut local_name = format!("meariLexical{}", scoped.props.len());
        let id_bindings = super::hooks::ids(&scoped.root);
        while scoped.props.iter().any(|prop| prop.name == local_name)
            || scoped
                .states
                .iter()
                .any(|state| state.name == local_name || state.setter == local_name)
            || id_bindings.contains(&local_name)
        {
            local_name.push('X');
        }
        scoped.props.push(Prop {
            name: local_name.clone(),
            kind: element.clone(),
        });
        scoped.root = self.template.clone();
        substitute_local(&mut scoped.root, &self.item, &Value::Prop(local_name));
        let ids = super::hooks::ids(&scoped.root);
        if scoped.states.is_empty() || ids.is_empty() {
            scoped.hook_order.clear();
        } else {
            scoped.hook_order.retain(|slot| match slot {
                super::hooks::HookSlot::Id(name) => ids.contains(name),
                super::hooks::HookSlot::State(_) => true,
            });
        }
        super::validation::validate(&scoped)
    }

    pub(crate) fn erase_spans(&mut self) {
        self.source_span = Span::default();
        self.key_span = Span::default();
        fn visit(node: &mut Node) {
            node.span = Span::default();
            match &mut node.kind {
                NodeKind::Text(Value::Repeat(repeat)) => repeat.erase_spans(),
                NodeKind::Element {
                    attributes,
                    children,
                    ..
                } => {
                    for attribute in attributes {
                        match &mut attribute.value {
                            Value::Predicate(expression) => expression.erase_spans(),
                            Value::Callback(handler) => handler.span = Span::default(),
                            _ => {}
                        }
                    }
                    for child in children {
                        visit(child);
                    }
                }
                _ => {}
            }
        }
        visit(&mut self.template);
    }
}

fn identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}
