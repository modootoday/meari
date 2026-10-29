use super::super::{
    component_state::{StateAction, StateType, StateValue},
    state_program::StateBinding,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Text,
    Checked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum InputPayload {
    Text(String),
    Checked(bool),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStateBinding {
    pub setter: String,
    pub kind: InputKind,
}

impl InputStateBinding {
    pub fn validate(&self, bindings: &[StateBinding]) -> Result<(), String> {
        let binding = bindings
            .iter()
            .find(|binding| binding.setter == self.setter)
            .ok_or("input state setter is unknown")?;
        let kind = match self.kind {
            InputKind::Text => StateType::String,
            InputKind::Checked => StateType::Bool,
        };
        if binding.initial.kind() != kind {
            return Err("input payload type differs from its state slot".into());
        }
        Ok(())
    }

    pub fn action(
        &self,
        bindings: &[StateBinding],
        payload: &InputPayload,
    ) -> Result<StateAction, String> {
        self.validate(bindings)?;
        let value = match (self.kind, payload) {
            (InputKind::Text, InputPayload::Text(value)) => StateValue::String(value.clone()),
            (InputKind::Checked, InputPayload::Checked(value)) => StateValue::Bool(*value),
            _ => return Err("input event payload kind mismatch".into()),
        };
        let slot = bindings
            .iter()
            .find(|binding| binding.setter == self.setter)
            .ok_or("input state setter disappeared")?
            .name
            .clone();
        Ok(StateAction::Set { slot, value })
    }
}
