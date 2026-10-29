use super::component_state::{ComponentState, StateCommit};
use crate::domain::{
    Component, Node, NodeKind, Value,
    component_state::{InstanceHandle, InstanceKey, StateSchema, StateSlot},
    state_program::StateEvent,
};
use std::collections::BTreeMap;
mod input;
mod media;
pub use input::InputHandler;
mod reload;
pub use reload::{PreparedReplacement, ReloadDisposition, ReloadPolicy};

#[derive(Clone)]
pub struct StateProgram {
    definition: Component,
    state: ComponentState,
    owner: InstanceHandle,
    definition_revision: u64,
}

#[derive(Clone)]
pub struct StateProjection {
    pub component: Component,
    pub events: BTreeMap<String, StateEvent>,
    pub inputs: BTreeMap<String, crate::domain::input_event::InputStateBinding>,
    pub callbacks: BTreeMap<String, crate::domain::host_callbacks::BoundCallback>,
}

impl StateProgram {
    fn schema(definition: &Component) -> StateSchema {
        StateSchema {
            revision: "source-state-v1".into(),
            slots: definition
                .states
                .iter()
                .map(|binding| {
                    (
                        binding.name.clone(),
                        StateSlot {
                            kind: binding.initial.kind(),
                            initial: binding.initial.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
    pub fn mount(definition: Component, document: String, epoch: u64) -> Result<Self, String> {
        crate::domain::validation::validate(&definition).map_err(|error| error.message)?;
        let mut state = ComponentState::new(document, epoch)?;
        let schema = Self::schema(&definition);
        let owner = state.mount(
            None,
            InstanceKey::Position(0),
            definition.name.clone(),
            schema,
        )?;
        Ok(Self {
            definition,
            state,
            owner,
            definition_revision: 1,
        })
    }

    pub fn snapshot(&self) -> Result<StateCommit, String> {
        self.state.snapshot(&self.owner)
    }

    pub fn dispatch(&mut self, event: &StateEvent) -> Result<StateProjection, String> {
        let snapshot = self.snapshot()?;
        let action = event.dispatch_action(&self.definition.states, &snapshot.values)?;
        let revision = snapshot.revision;
        let mut staged = self.clone();
        staged.state.dispatch(&staged.owner, revision, &[action])?;
        let projection = staged.project()?;
        *self = staged;
        Ok(projection)
    }

    pub fn project(&self) -> Result<StateProjection, String> {
        let snapshot = self.snapshot()?;
        for value in snapshot.values.values() {
            if crate::domain::state_program::string_items(value).is_none() {
                crate::domain::state_program::render_value(value)?;
            }
        }
        Self::project_bindings(&self.definition, &snapshot.values, false)
    }

    pub(crate) fn project_bindings(
        definition: &Component,
        values: &BTreeMap<String, crate::domain::component_state::StateValue>,
        defer_hook_events: bool,
    ) -> Result<StateProjection, String> {
        let mut component = definition.clone();
        state_lists(&mut component, values)?;
        let mut events = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        let mut callbacks = BTreeMap::new();
        fn visit(
            node: &mut Node,
            path: String,
            values: &BTreeMap<String, crate::domain::component_state::StateValue>,
            defer_hook_events: bool,
            events: &mut BTreeMap<String, StateEvent>,
            inputs: &mut BTreeMap<String, crate::domain::input_event::InputStateBinding>,
            callbacks: &mut BTreeMap<String, crate::domain::host_callbacks::BoundCallback>,
        ) -> Result<(), String> {
            let resolve = |value: &mut Value| -> Result<(), String> {
                if let Value::State(name) = value {
                    *value = crate::domain::state_program::render_value(
                        values.get(name).ok_or("state slot is not a scalar projection; structured reads require a field expression")?,
                    )?;
                } else if let Value::Predicate(expression) = value {
                    *value = Value::Bool(
                        expression
                            .evaluate_with_states(&BTreeMap::new(), values)
                            .map_err(|error| error.message)?,
                    );
                }
                Ok(())
            };
            match &mut node.kind {
                NodeKind::Text(value) => {
                    resolve(value)?;
                    if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(nodes)) =
                        value
                    {
                        for (index, child) in nodes.iter_mut().enumerate() {
                            visit(
                                child,
                                format!("{path}/slot/{index}"),
                                values,
                                defer_hook_events,
                                events,
                                inputs,
                                callbacks,
                            )?;
                        }
                    }
                }
                NodeKind::Element {
                    attributes,
                    children,
                    ..
                } => {
                    for attribute in attributes.iter() {
                        if let Value::Event(event) = &attribute.value {
                            events.insert(
                                path.clone(),
                                if defer_hook_events {
                                    event.clone()
                                } else {
                                    event.capture(values)?
                                },
                            );
                        }
                        if let Value::Input(binding) = &attribute.value {
                            inputs.insert(path.clone(), binding.clone());
                        }
                        if let Value::BoundCallback(binding) = &attribute.value {
                            binding.validate()?;
                            let key = if attribute.name == "onValueCommit" {
                                super::host_callbacks::DocumentCallbacks::value_commit_key(&path)
                            } else {
                                path.clone()
                            };
                            callbacks.insert(key, binding.clone());
                        }
                    }
                    attributes.retain(|attribute| {
                        !matches!(
                            attribute.value,
                            Value::Event(_) | Value::Input(_) | Value::BoundCallback(_)
                        )
                    });
                    for attribute in attributes {
                        resolve(&mut attribute.value)?;
                        if let Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(
                            nodes,
                        )) = &mut attribute.value
                        {
                            for (index, child) in nodes.iter_mut().enumerate() {
                                visit(
                                    child,
                                    format!("{path}/slot:{}/{index}", attribute.name),
                                    values,
                                    defer_hook_events,
                                    events,
                                    inputs,
                                    callbacks,
                                )?;
                            }
                        }
                    }
                    for (index, child) in children.iter_mut().enumerate() {
                        visit(
                            child,
                            format!("{path}/{index}"),
                            values,
                            defer_hook_events,
                            events,
                            inputs,
                            callbacks,
                        )?;
                    }
                }
            }
            Ok(())
        }
        visit(
            &mut component.root,
            "0".into(),
            values,
            defer_hook_events,
            &mut events,
            &mut inputs,
            &mut callbacks,
        )?;
        component.states.clear();
        component.hook_order.clear();
        Ok(StateProjection {
            component,
            events,
            inputs,
            callbacks,
        })
    }
}

/// Bind state-driven `.map` sources to the current string items, then expand them with the
/// same keyed identities as prop-driven lists, before events and reads inside are projected.
fn state_lists(
    component: &mut Component,
    values: &BTreeMap<String, crate::domain::component_state::StateValue>,
) -> Result<(), String> {
    fn visit(
        node: &mut Node,
        values: &BTreeMap<String, crate::domain::component_state::StateValue>,
        found: &mut bool,
    ) -> Result<(), String> {
        match &mut node.kind {
            NodeKind::Text(Value::Repeat(repeat)) => {
                if let Value::State(name) = &repeat.source {
                    let items = values
                        .get(name)
                        .and_then(crate::domain::state_program::string_items)
                        .ok_or("repeat state source requires a string array projection")?;
                    repeat.source =
                        Value::Array(crate::domain::array_value::ArrayValue::strings(items));
                    *found = true;
                }
                visit(&mut repeat.template, values, found)
            }
            NodeKind::Text(Value::NodeSlot(crate::domain::node_slots::NodeSlot::Content(
                nodes,
            ))) => nodes
                .iter_mut()
                .try_for_each(|child| visit(child, values, found)),
            NodeKind::Text(_) => Ok(()),
            NodeKind::Element { children, .. } => children
                .iter_mut()
                .try_for_each(|child| visit(child, values, found)),
        }
    }
    let mut found = false;
    visit(&mut component.root, values, &mut found)?;
    if found {
        super::repeat::expand(component, None)?;
    }
    Ok(())
}
