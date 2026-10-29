//! Original declaration identity retained separately from resolved runtime bindings.
use super::super::{
    Component, Prop,
    build::ContentHash,
    component_state::StateType,
    hooks::{self, HookSlot},
};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Hook {
    Id {
        name: String,
    },
    State {
        name: String,
        value_type: StateType,
        #[serde(skip_serializing_if = "Option::is_none")]
        shape: Option<super::super::hook_expression::LiteralShape>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SourceSchema {
    version: u32,
    component: String,
    client: bool,
    props: Vec<Prop>,
    hooks: Vec<Hook>,
}
impl SourceSchema {
    pub fn from_component(component: &Component) -> Result<Self, String> {
        super::super::validation::validate(component).map_err(|error| error.message)?;
        let state_hook = |name: &str| -> Result<Hook, String> {
            let state = component
                .states
                .iter()
                .find(|state| state.name == name)
                .ok_or("source hook references an absent state")?;
            Ok(Hook::State {
                name: name.into(),
                value_type: state.initial.kind(),
                shape: match &state.initial {
                    super::super::component_state::StateValue::Structured(expression) => {
                        Some(expression.literal_shape().map_err(|error| error.message)?)
                    }
                    _ => None,
                },
            })
        };
        let hooks = if component.hook_order.is_empty() {
            let mut result = hooks::ids(&component.root)
                .into_iter()
                .map(|name| Hook::Id { name })
                .collect::<Vec<_>>();
            for state in &component.states {
                result.push(state_hook(&state.name)?);
            }
            result
        } else {
            component
                .hook_order
                .iter()
                .map(|slot| match slot {
                    HookSlot::Id(name) => Ok(Hook::Id { name: name.clone() }),
                    HookSlot::State(name) => state_hook(name),
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        Ok(Self {
            version: if component
                .props
                .iter()
                .any(|prop| prop.kind.contains_record())
            {
                5
            } else if component.states.iter().any(|state| {
                matches!(
                    state.initial,
                    super::super::component_state::StateValue::Structured(_)
                )
            }) {
                4
            } else if component
                .props
                .iter()
                .any(|prop| matches!(prop.kind, super::super::PropType::Array(_)))
            {
                3
            } else if component
                .props
                .iter()
                .any(|prop| matches!(prop.kind, super::super::PropType::Callback(_)))
            {
                2
            } else {
                1
            },
            component: component.name.clone(),
            client: component.client,
            props: component.props.clone(),
            hooks,
        })
    }
    pub fn fingerprint(&self) -> Result<ContentHash, String> {
        Ok(ContentHash::of(
            &serde_json::to_vec(self).map_err(|error| error.to_string())?,
        ))
    }
}
