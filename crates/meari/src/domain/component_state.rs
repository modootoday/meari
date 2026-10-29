use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    Bool,
    String,
    Number,
    Json,
    Structured,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StateValue {
    Bool(bool),
    String(String),
    Number(serde_json::Number),
    Json(serde_json::Value),
    /// Static source description, never a live runtime root or its equality policy.
    Structured(Box<super::hook_expression::Expression>),
}

impl StateValue {
    pub fn kind(&self) -> StateType {
        match self {
            Self::Bool(_) => StateType::Bool,
            Self::String(_) => StateType::String,
            Self::Number(_) => StateType::Number,
            Self::Json(_) => StateType::Json,
            Self::Structured(_) => StateType::Structured,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSlot {
    pub kind: StateType,
    pub initial: StateValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSchema {
    pub revision: String,
    pub slots: BTreeMap<String, StateSlot>,
}

pub fn identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

impl StateSchema {
    pub fn validate(&self) -> Result<(), String> {
        if self.slots.len() > 1024 {
            return Err("component state exceeds 1024 slots".into());
        }
        if !identifier(&self.revision) {
            return Err("state schema revision is invalid".into());
        }
        for (name, slot) in &self.slots {
            if slot.kind == StateType::Structured {
                return Err("structured source state requires a prepared hook runtime".into());
            }
            if !identifier(name) || slot.kind != slot.initial.kind() {
                return Err(format!("invalid state slot: {name}"));
            }
        }
        Ok(())
    }

    pub fn same_layout(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.slots.len() == other.slots.len()
            && self.slots.iter().all(|(name, slot)| {
                other
                    .slots
                    .get(name)
                    .is_some_and(|other| slot.kind == other.kind)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceKey {
    Explicit(String),
    Position(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceHandle {
    pub(crate) document: String,
    pub(crate) epoch: u64,
    pub(crate) id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum StateAction {
    Set {
        slot: String,
        value: StateValue,
    },
    Toggle {
        slot: String,
    },
    Add {
        slot: String,
        delta: serde_json::Number,
    },
}

impl StateAction {
    pub fn slot(&self) -> &str {
        match self {
            Self::Set { slot, .. } | Self::Toggle { slot } | Self::Add { slot, .. } => slot,
        }
    }
}
