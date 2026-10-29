use super::super::{
    component_state::{StateType, StateValue},
    state_program::StateBinding,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unary {
    Not,
    Negate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StateExpression {
    Literal {
        value: StateValue,
    },
    State {
        name: String,
    },
    Previous,
    Unary {
        operator: Unary,
        argument: Box<Self>,
    },
    Binary {
        operator: Binary,
        left: Box<Self>,
        right: Box<Self>,
    },
    Conditional {
        test: Box<Self>,
        consequent: Box<Self>,
        alternate: Box<Self>,
    },
}

fn depth_limit(depth: u32) -> Result<(), String> {
    if depth > 64 {
        Err("state expression depth exceeds 64".into())
    } else {
        Ok(())
    }
}

impl StateExpression {
    pub fn infer(
        &self,
        bindings: &[StateBinding],
        previous: Option<StateType>,
    ) -> Result<StateType, String> {
        self.infer_at(bindings, previous, 0)
    }

    fn infer_at(
        &self,
        bindings: &[StateBinding],
        previous: Option<StateType>,
        depth: u32,
    ) -> Result<StateType, String> {
        depth_limit(depth)?;
        let infer = |value: &Self| value.infer_at(bindings, previous, depth + 1);
        let kind = match self {
            Self::Literal { value } => value.kind(),
            Self::State { name } => bindings
                .iter()
                .find(|binding| binding.name == *name)
                .map(|binding| binding.initial.kind())
                .ok_or_else(|| format!("unknown expression state: {name}"))?,
            Self::Previous => previous.ok_or("previous is only valid in an updater")?,
            Self::Unary { operator, argument } => match (operator, infer(argument)?) {
                (Unary::Not, StateType::Bool | StateType::String | StateType::Number) => {
                    StateType::Bool
                }
                (Unary::Negate, StateType::Number) => StateType::Number,
                _ => return Err("invalid unary state expression type".into()),
            },
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let (left, right) = (infer(left)?, infer(right)?);
                use Binary::*;
                match operator {
                    Add if left == StateType::String || right == StateType::String => {
                        StateType::String
                    }
                    Add | Subtract | Multiply | Divide | Remainder
                        if left == StateType::Number && right == StateType::Number =>
                    {
                        StateType::Number
                    }
                    Equal | NotEqual => StateType::Bool,
                    Less | LessEqual | Greater | GreaterEqual
                        if left == right
                            && matches!(left, StateType::Number | StateType::String) =>
                    {
                        StateType::Bool
                    }
                    And | Or if left == right => left,
                    _ => return Err("invalid binary state expression types".into()),
                }
            }
            Self::Conditional {
                test,
                consequent,
                alternate,
            } => {
                infer(test)?;
                let kind = infer(consequent)?;
                if kind != infer(alternate)? {
                    return Err("conditional state branches have different types".into());
                }
                kind
            }
        };
        if matches!(kind, StateType::Json | StateType::Structured) {
            return Err("object/collection state expression requires explicit member IR".into());
        }
        Ok(kind)
    }

    pub fn evaluate(
        &self,
        values: &BTreeMap<String, StateValue>,
        previous: Option<&StateValue>,
    ) -> Result<StateValue, String> {
        self.evaluate_at(values, previous, 0)
    }

    fn evaluate_at(
        &self,
        values: &BTreeMap<String, StateValue>,
        previous: Option<&StateValue>,
        depth: u32,
    ) -> Result<StateValue, String> {
        depth_limit(depth)?;
        let evaluate = |value: &Self| value.evaluate_at(values, previous, depth + 1);
        match self {
            Self::Literal { value } => Ok(value.clone()),
            Self::State { name } => values
                .get(name)
                .cloned()
                .ok_or_else(|| format!("missing expression state: {name}")),
            Self::Previous => previous
                .cloned()
                .ok_or_else(|| "previous is unavailable".into()),
            Self::Unary { operator, argument } => {
                let value = evaluate(argument)?;
                match operator {
                    Unary::Not => Ok(StateValue::Bool(!truthy(&value)?)),
                    Unary::Negate => numeric(-number(&value)?),
                }
            }
            Self::Conditional {
                test,
                consequent,
                alternate,
            } => {
                if truthy(&evaluate(test)?)? {
                    evaluate(consequent)
                } else {
                    evaluate(alternate)
                }
            }
            Self::Binary {
                operator,
                left,
                right,
            } => {
                let left = evaluate(left)?;
                if (*operator == Binary::And && !truthy(&left)?)
                    || (*operator == Binary::Or && truthy(&left)?)
                {
                    return Ok(left);
                }
                let right = evaluate(right)?;
                use Binary::*;
                match operator {
                    And | Or => Ok(right),
                    Add if matches!(left, StateValue::String(_))
                        || matches!(right, StateValue::String(_)) =>
                    {
                        Ok(StateValue::String(format!(
                            "{}{}",
                            text(&left)?,
                            text(&right)?
                        )))
                    }
                    Add => numeric(number(&left)? + number(&right)?),
                    Subtract => numeric(number(&left)? - number(&right)?),
                    Multiply => numeric(number(&left)? * number(&right)?),
                    Divide => numeric(number(&left)? / number(&right)?),
                    Remainder => numeric(number(&left)? % number(&right)?),
                    Equal | NotEqual => {
                        let equal = match (&left, &right) {
                            (StateValue::Number(_), StateValue::Number(_)) => {
                                number(&left)? == number(&right)?
                            }
                            _ => left == right,
                        };
                        Ok(StateValue::Bool(if *operator == Equal {
                            equal
                        } else {
                            !equal
                        }))
                    }
                    Less | LessEqual | Greater | GreaterEqual => {
                        let order = match (&left, &right) {
                            (StateValue::String(left), StateValue::String(right)) => {
                                left.encode_utf16().cmp(right.encode_utf16())
                            }
                            _ => number(&left)?
                                .partial_cmp(&number(&right)?)
                                .ok_or("unordered numeric comparison")?,
                        };
                        Ok(StateValue::Bool(match operator {
                            Less => order.is_lt(),
                            LessEqual => !order.is_gt(),
                            Greater => order.is_gt(),
                            _ => !order.is_lt(),
                        }))
                    }
                }
            }
        }
    }

    pub fn capture(&self, values: &BTreeMap<String, StateValue>) -> Result<Self, String> {
        self.capture_at(values, 0)
    }

    fn capture_at(
        &self,
        values: &BTreeMap<String, StateValue>,
        depth: u32,
    ) -> Result<Self, String> {
        depth_limit(depth)?;
        let capture = |value: &Self| value.capture_at(values, depth + 1).map(Box::new);
        Ok(match self {
            Self::State { name } => Self::Literal {
                value: values.get(name).cloned().ok_or("capture state missing")?,
            },
            Self::Unary { operator, argument } => Self::Unary {
                operator: *operator,
                argument: capture(argument)?,
            },
            Self::Binary {
                operator,
                left,
                right,
            } => Self::Binary {
                operator: *operator,
                left: capture(left)?,
                right: capture(right)?,
            },
            Self::Conditional {
                test,
                consequent,
                alternate,
            } => Self::Conditional {
                test: capture(test)?,
                consequent: capture(consequent)?,
                alternate: capture(alternate)?,
            },
            value => value.clone(),
        })
    }
}

fn number(value: &StateValue) -> Result<f64, String> {
    let StateValue::Number(value) = value else {
        return Err("numeric state operand required".into());
    };
    let number = value.as_f64().ok_or("numeric operand unavailable")?;
    if !number.is_finite()
        || ((value.is_i64() || value.is_u64()) && number.abs() > 9_007_199_254_740_991.0)
    {
        return Err("numeric expression operand cannot preserve JavaScript semantics".into());
    }
    Ok(number)
}

fn numeric(value: f64) -> Result<StateValue, String> {
    serde_json::Number::from_f64(value)
        .map(StateValue::Number)
        .ok_or_else(|| "non-finite state expression result".into())
}

fn truthy(value: &StateValue) -> Result<bool, String> {
    match value {
        StateValue::Bool(value) => Ok(*value),
        StateValue::String(value) => Ok(!value.is_empty()),
        StateValue::Number(_) => Ok(number(value)? != 0.0),
        _ => Err("object truthiness is not implemented".into()),
    }
}

fn text(value: &StateValue) -> Result<String, String> {
    match value {
        StateValue::String(value) => Ok(value.clone()),
        StateValue::Bool(value) => Ok(value.to_string()),
        StateValue::Number(_) => Ok(ryu_js::Buffer::new().format(number(value)?).to_owned()),
        _ => Err("object string conversion is not implemented".into()),
    }
}
