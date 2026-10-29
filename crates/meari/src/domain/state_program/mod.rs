use super::{
    Span, Value,
    component_state::{StateAction, StateValue},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateBinding {
    pub name: String,
    pub setter: String,
    pub initial: StateValue,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StateUpdate {
    Set(StateValue),
    Toggle,
    Add(serde_json::Number),
    Capture(super::state_expression::StateExpression),
    Reduce(super::state_expression::StateExpression),
    HookCapture(super::hook_expression::Expression),
    HookReduce(super::hook_expression::Expression),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateEvent {
    pub setter: String,
    pub update: StateUpdate,
}

impl StateEvent {
    pub fn validate(&self, bindings: &[StateBinding]) -> Result<(), String> {
        let binding = bindings
            .iter()
            .find(|binding| binding.setter == self.setter)
            .ok_or("unknown state setter")?;
        if matches!(binding.initial, StateValue::Structured(_))
            || matches!(
                self.update,
                StateUpdate::HookCapture(_) | StateUpdate::HookReduce(_)
            )
        {
            let expected = super::hook_expression::shape::state(&binding.initial)?;
            let expression = self
                .hook_expression(binding.span)
                .map_err(|error| error.message)?;
            let previous = matches!(
                self.update,
                StateUpdate::Reduce(_) | StateUpdate::HookReduce(_)
            )
            .then_some(&expected);
            let actual = expression
                .infer_shape(bindings, previous)
                .map_err(|error| error.message)?;
            if !expected.accepts(&actual)? {
                return Err("structured setter literal type mismatch".into());
            }
            return Ok(());
        }
        let kind = match &self.update {
            StateUpdate::Capture(expression) => expression.infer(bindings, None)?,
            StateUpdate::Reduce(expression) => {
                expression.infer(bindings, Some(binding.initial.kind()))?
            }
            _ => {
                self.action(bindings)?;
                return Ok(());
            }
        };
        if kind != binding.initial.kind() {
            return Err("state expression setter type mismatch".into());
        }
        Ok(())
    }

    pub fn capture(
        &self,
        values: &std::collections::BTreeMap<String, StateValue>,
    ) -> Result<Self, String> {
        let update = match &self.update {
            StateUpdate::HookCapture(_) | StateUpdate::HookReduce(_) => {
                return Err("hook expression capture requires a prepared hook document".into());
            }
            StateUpdate::Capture(expression) => StateUpdate::Capture(expression.capture(values)?),
            StateUpdate::Reduce(expression) => StateUpdate::Reduce(expression.capture(values)?),
            update => update.clone(),
        };
        Ok(Self {
            setter: self.setter.clone(),
            update,
        })
    }

    pub fn dispatch_action(
        &self,
        bindings: &[StateBinding],
        values: &std::collections::BTreeMap<String, StateValue>,
    ) -> Result<StateAction, String> {
        self.validate(bindings)?;
        if let StateUpdate::Capture(expression) | StateUpdate::Reduce(expression) = &self.update {
            if expression.capture(values)? != *expression {
                return Err("updater free state must be captured during render".into());
            }
            let binding = bindings
                .iter()
                .find(|binding| binding.setter == self.setter)
                .ok_or("unknown setter")?;
            let previous = if matches!(self.update, StateUpdate::Reduce(_)) {
                values.get(&binding.name)
            } else {
                None
            };
            let value = expression.evaluate(values, previous)?;
            return Ok(StateAction::Set {
                slot: binding.name.clone(),
                value,
            });
        }
        self.action(bindings)
    }

    pub fn action(&self, bindings: &[StateBinding]) -> Result<StateAction, String> {
        let binding = bindings
            .iter()
            .find(|binding| binding.setter == self.setter)
            .ok_or("unknown state setter")?;
        if matches!(binding.initial, StateValue::Structured(_)) {
            return Err("structured source state requires a prepared hook runtime".into());
        }
        Ok(match &self.update {
            StateUpdate::Set(value) if value.kind() == binding.initial.kind() => StateAction::Set {
                slot: binding.name.clone(),
                value: value.clone(),
            },
            StateUpdate::Toggle if matches!(binding.initial, StateValue::Bool(_)) => {
                StateAction::Toggle {
                    slot: binding.name.clone(),
                }
            }
            StateUpdate::Add(delta) if matches!(binding.initial, StateValue::Number(_)) => {
                StateAction::Add {
                    slot: binding.name.clone(),
                    delta: delta.clone(),
                }
            }
            StateUpdate::Capture(_)
            | StateUpdate::Reduce(_)
            | StateUpdate::HookCapture(_)
            | StateUpdate::HookReduce(_) => {
                return Err("state expression requires a render capture/runtime update".into());
            }
            _ => return Err("state setter update type mismatch".into()),
        })
    }
}

mod hook_update;
pub(crate) use hook_update::literal as hook_literal;

/// A dense string array: the one state collection a keyed `.map` renders as siblings.
pub fn string_items(value: &StateValue) -> Option<Vec<String>> {
    use super::hook_expression::Kind;
    match value {
        StateValue::Structured(expression) => match &expression.kind {
            Kind::Array(items) => items
                .iter()
                .map(|item| match &item.kind {
                    Kind::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        },
        StateValue::Json(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => None,
    }
}

pub fn render_value(value: &StateValue) -> Result<Value, String> {
    match value {
        StateValue::Bool(value) => Ok(Value::Bool(*value)),
        StateValue::String(value) => Ok(Value::String(value.clone())),
        StateValue::Number(number) => {
            let value = number.as_f64().ok_or("state number is unavailable")?;
            if !value.is_finite()
                || ((number.is_i64() || number.is_u64()) && value.abs() > 9_007_199_254_740_991.0)
            {
                return Err("source state number exceeds JavaScript safe numeric range".into());
            }
            Ok(Value::String(
                ryu_js::Buffer::new().format(value).to_owned(),
            ))
        }
        StateValue::Json(_) | StateValue::Structured(_) => {
            Err("object state rendering requires explicit property/collection expression IR".into())
        }
    }
}
