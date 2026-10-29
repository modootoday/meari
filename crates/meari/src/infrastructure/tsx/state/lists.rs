//! State-driven keyed lists: `const [items, setItems] = useState(["a", "b"])` rendered by
//! `items.map(item => <li key={item}>{item}</li>)`.
//!
//! A state read only as a `.map` source with a dense string-array literal is held as a JSON
//! string array, which the scalar state program can store, replace with `Set` literals, and
//! expand per projection. Any other update shape stays structured and is rejected by
//! validation instead of being approximated.
use crate::domain::{
    Node, NodeKind, Value,
    component_state::StateValue,
    node_slots::NodeSlot,
    state_program::{StateBinding, StateUpdate, string_items},
};

fn sources(node: &Node, out: &mut Vec<String>) {
    match &node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => {
            if let Value::State(name) = &repeat.source {
                out.push(name.clone());
            }
            sources(&repeat.template, out);
        }
        NodeKind::Text(Value::NodeSlot(NodeSlot::Content(nodes))) => {
            nodes.iter().for_each(|child| sources(child, out))
        }
        NodeKind::Text(_) => {}
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                if let Value::NodeSlot(NodeSlot::Content(nodes)) = &attribute.value {
                    nodes.iter().for_each(|child| sources(child, out));
                }
            }
            children.iter().for_each(|child| sources(child, out));
        }
    }
}

fn json(value: &StateValue) -> Option<StateValue> {
    let items = string_items(value)?;
    (!items.is_empty()).then(|| {
        StateValue::Json(serde_json::Value::Array(
            items.into_iter().map(serde_json::Value::String).collect(),
        ))
    })
}

fn events(node: &mut Node, setters: &[String]) {
    let convert = |value: &mut Value| {
        if let Value::Event(event) = value
            && setters.contains(&event.setter)
            && let StateUpdate::Set(update) = &event.update
            && let Some(update) = json(update)
        {
            event.update = StateUpdate::Set(update);
        }
    };
    match &mut node.kind {
        NodeKind::Text(Value::Repeat(repeat)) => events(&mut repeat.template, setters),
        NodeKind::Text(Value::NodeSlot(NodeSlot::Content(nodes))) => {
            nodes.iter_mut().for_each(|child| events(child, setters))
        }
        NodeKind::Text(value) => convert(value),
        NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            for attribute in attributes {
                convert(&mut attribute.value);
                if let Value::NodeSlot(NodeSlot::Content(nodes)) = &mut attribute.value {
                    nodes.iter_mut().for_each(|child| events(child, setters));
                }
            }
            children.iter_mut().for_each(|child| events(child, setters));
        }
    }
}

pub(in super::super) fn admit(root: &mut Node, states: &mut [StateBinding]) {
    let mut names = Vec::new();
    sources(root, &mut names);
    let mut setters = Vec::new();
    for state in states
        .iter_mut()
        .filter(|state| names.contains(&state.name))
    {
        if let Some(initial) = json(&state.initial) {
            state.initial = initial;
            setters.push(state.setter.clone());
        }
    }
    events(root, &setters);
}
