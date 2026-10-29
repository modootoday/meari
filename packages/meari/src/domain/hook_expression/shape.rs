use super::{Entry, Expression, Kind, LiteralShape};
use crate::domain::{Diagnostic, component_state::StateValue, state_program::StateBinding};
use std::collections::BTreeMap;

pub fn state(value: &StateValue) -> Result<LiteralShape, String> {
    Ok(match value {
        StateValue::Bool(_) => LiteralShape::Boolean,
        StateValue::String(_) => LiteralShape::Text,
        StateValue::Number(_) => LiteralShape::Number,
        StateValue::Structured(expression) => expression.literal_shape().map_err(|e| e.message)?,
        StateValue::Json(_) => return Err("JSON is not a typed hook binding".into()),
    })
}

impl Expression {
    /// Source type checking only; runtime roots remain owned by the document heap.
    pub(crate) fn infer_shape(
        &self,
        bindings: &[StateBinding],
        previous: Option<&LiteralShape>,
    ) -> Result<LiteralShape, Diagnostic> {
        self.validate()?;
        fn charge(shape: &LiteralShape, work: &mut usize, depth: usize) -> Result<(), ()> {
            if *work == 0 || depth > 64 {
                return Err(());
            }
            *work -= 1;
            match shape {
                LiteralShape::Array(items) => {
                    for item in items {
                        charge(item, work, depth + 1)?;
                    }
                }
                LiteralShape::Record(fields) => {
                    for item in fields.values() {
                        charge(item, work, depth + 1)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        fn infer(
            expression: &Expression,
            bindings: &[StateBinding],
            previous: Option<&LiteralShape>,
            work: &mut usize,
        ) -> Result<LiteralShape, Diagnostic> {
            if *work == 0 {
                return Err(Diagnostic::unsupported(
                    expression.span,
                    "hook type work budget exceeded",
                ));
            }
            *work -= 1;
            let fail = |message| Diagnostic::unsupported(expression.span, message);
            macro_rules! child {
                ($value:expr) => {
                    infer($value, bindings, previous, work)
                };
            }
            Ok(match &expression.kind {
                Kind::Undefined => return Err(fail("undefined hook shape requires a union type")),
                Kind::Null => LiteralShape::Null,
                Kind::Boolean(_) => LiteralShape::Boolean,
                Kind::Number(_) => LiteralShape::Number,
                Kind::Text(_) => LiteralShape::Text,
                Kind::State(name) => {
                    let shape = state(
                        &bindings
                            .iter()
                            .find(|b| b.name == *name)
                            .ok_or_else(|| fail("unknown hook state binding"))?
                            .initial,
                    )
                    .map_err(|message| Diagnostic::unsupported(expression.span, message))?;
                    charge(&shape, work, 0).map_err(|()| fail("hook type work budget exceeded"))?;
                    shape
                }
                Kind::Previous => {
                    let shape =
                        previous.ok_or_else(|| fail("previous requires a functional updater"))?;
                    charge(shape, work, 0).map_err(|()| fail("hook type work budget exceeded"))?;
                    shape.clone()
                }
                Kind::Field { target, key } => {
                    let LiteralShape::Record(fields) = child!(target)? else {
                        return Err(fail("hook field requires a record"));
                    };
                    fields
                        .get(key)
                        .cloned()
                        .ok_or_else(|| fail("unknown hook field requires an optional type"))?
                }
                Kind::Element { target, .. } => {
                    let LiteralShape::Array(kinds) = child!(target)? else {
                        return Err(fail("hook index requires an array"));
                    };
                    if kinds.len() != 1 {
                        return Err(fail(
                            "empty or heterogeneous array access requires a union type",
                        ));
                    }
                    kinds[0].clone()
                }
                Kind::Array(items) => {
                    let mut kinds = items
                        .iter()
                        .map(|item| child!(item))
                        .collect::<Result<Vec<_>, _>>()?;
                    kinds.sort();
                    kinds.dedup();
                    LiteralShape::Array(kinds)
                }
                Kind::Record(entries) => {
                    let mut fields = BTreeMap::new();
                    for entry in entries {
                        match entry {
                            Entry::Field { key, value } => {
                                fields.insert(key.clone(), child!(value)?);
                            }
                            Entry::Spread { value } => {
                                let LiteralShape::Record(spread) = child!(value)? else {
                                    return Err(fail("hook object spread requires a record"));
                                };
                                if fields.len().saturating_add(spread.len()) > *work {
                                    return Err(fail("hook type work budget exceeded"));
                                }
                                *work -= spread.len();
                                fields.extend(spread);
                            }
                        }
                    }
                    LiteralShape::Record(fields)
                }
                Kind::Not(value) => {
                    child!(value)?;
                    LiteralShape::Boolean
                }
                Kind::Add { left, right } => {
                    let (left, right) = (child!(left)?, child!(right)?);
                    if left != right || !matches!(left, LiteralShape::Number | LiteralShape::Text) {
                        return Err(fail("hook addition requires two numbers or two strings"));
                    }
                    left
                }
            })
        }
        infer(self, bindings, previous, &mut 65_536)
    }
}
