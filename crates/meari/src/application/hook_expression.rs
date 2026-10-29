//! Evaluate source IR into one unpublished candidate. No renderer or committed owner mutation.
use super::hook_values::{Error, Transaction, Value, ValueKind};
use crate::domain::{
    Diagnostic, Span,
    hook_expression::{Entry, Expression, Kind},
};
use std::collections::BTreeMap;

pub struct Bindings<'a> {
    pub states: &'a BTreeMap<String, Value>,
    pub previous: Option<&'a Value>,
}
pub struct Evaluated {
    pub candidate: Transaction,
    pub root: Value,
}
pub struct EvaluationFailure {
    pub candidate: Transaction,
    pub diagnostic: Diagnostic,
    pub cause: Option<Error>,
}
impl std::fmt::Debug for EvaluationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvaluationFailure")
            .field("diagnostic", &self.diagnostic)
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
struct Fault {
    span: Span,
    message: String,
    cause: Option<Error>,
}
fn fault(span: Span, message: &'static str) -> Fault {
    Fault {
        span,
        message: message.into(),
        cause: None,
    }
}
fn heap<T>(span: Span, value: Result<T, Error>) -> Result<T, Fault> {
    value.map_err(|cause| Fault {
        span,
        message: "structured candidate value rejected".into(),
        cause: Some(cause),
    })
}

pub fn evaluate(
    mut candidate: Transaction,
    expression: &Expression,
    bindings: Bindings<'_>,
) -> Result<Evaluated, Box<EvaluationFailure>> {
    let result = expression
        .validate()
        .map_err(|diagnostic| Fault {
            span: diagnostic.span,
            message: diagnostic.message,
            cause: None,
        })
        .and_then(|()| visit(&mut candidate, expression, &bindings, &mut 0));
    match result {
        Ok(root) => Ok(Evaluated { candidate, root }),
        Err(error) => {
            if error.cause != Some(Error::Busy) {
                candidate.reject_expression();
            }
            Err(Box::new(EvaluationFailure {
                candidate,
                diagnostic: Diagnostic::unsupported(error.span, error.message),
                cause: error.cause,
            }))
        }
    }
}

fn charge(span: Span, work: &mut usize) -> Result<(), Fault> {
    *work += 1;
    if *work > 65_536 {
        return Err(fault(span, "structured evaluation work budget exceeded"));
    }
    Ok(())
}
fn visit(
    tx: &mut Transaction,
    expr: &Expression,
    bindings: &Bindings<'_>,
    work: &mut usize,
) -> Result<Value, Fault> {
    charge(expr.span, work)?;
    let span = expr.span;
    let value = match &expr.kind {
        Kind::Undefined => heap(span, tx.undefined())?,
        Kind::Null => heap(span, tx.null())?,
        Kind::Boolean(value) => heap(span, tx.boolean(*value))?,
        Kind::Number(bits) => heap(span, tx.number(f64::from_bits(*bits)))?,
        Kind::Text(value) => heap(span, tx.string(value.clone()))?,
        Kind::State(name) => bindings
            .states
            .get(name)
            .cloned()
            .ok_or_else(|| fault(span, "unbound structured state"))?,
        Kind::Previous => bindings
            .previous
            .cloned()
            .ok_or_else(|| fault(span, "previous state is only available inside an updater"))?,
        Kind::Field { target, key } => {
            let target = visit(tx, target, bindings, work)?;
            match heap(span, tx.field(&target, key))? {
                Some(value) => value,
                None => heap(span, tx.undefined())?,
            }
        }
        Kind::Element { target, index } => {
            let target = visit(tx, target, bindings, work)?;
            match heap(span, tx.element(&target, *index))? {
                Some(value) => value,
                None => heap(span, tx.undefined())?,
            }
        }
        Kind::Array(items) => {
            let mut values = Vec::new();
            for item in items {
                values.push(visit(tx, item, bindings, work)?);
            }
            heap(span, tx.array(values))?
        }
        Kind::Record(entries) => {
            let mut fields = BTreeMap::new();
            for entry in entries {
                match entry {
                    Entry::Field { key, value } => {
                        fields.insert(key.clone(), visit(tx, value, bindings, work)?);
                    }
                    Entry::Spread { value } => {
                        let value = visit(tx, value, bindings, work)?;
                        for (key, value) in heap(span, tx.fields(&value))? {
                            charge(span, work)?;
                            fields.insert(key.to_owned(), value);
                        }
                    }
                }
            }
            heap(span, tx.record(fields.into_iter().collect()))?
        }
        Kind::Not(value) => {
            let value = visit(tx, value, bindings, work)?;
            let truth = match heap(span, tx.kind(&value))? {
                ValueKind::Undefined | ValueKind::Null => false,
                ValueKind::Boolean => heap(span, tx.read_boolean(&value))?,
                ValueKind::Number => {
                    let n = heap(span, tx.read_number(&value))?;
                    n != 0.0 && !n.is_nan()
                }
                ValueKind::String => !heap(span, tx.read_text(&value))?.is_empty(),
                ValueKind::Array | ValueKind::Record => true,
            };
            heap(span, tx.boolean(!truth))?
        }
        Kind::Add { left, right } => {
            let left = visit(tx, left, bindings, work)?;
            let right = visit(tx, right, bindings, work)?;
            match (heap(span, tx.kind(&left))?, heap(span, tx.kind(&right))?) {
                (ValueKind::Number, ValueKind::Number) => {
                    let value =
                        heap(span, tx.read_number(&left))? + heap(span, tx.read_number(&right))?;
                    heap(span, tx.number(value))?
                }
                (ValueKind::String, ValueKind::String) => {
                    let mut value = heap(span, tx.read_text(&left))?.to_owned();
                    value.push_str(heap(span, tx.read_text(&right))?);
                    heap(span, tx.string(value))?
                }
                _ => {
                    return Err(fault(
                        span,
                        "mixed-type addition requires an explicit source coercion adapter",
                    ));
                }
            }
        }
    };
    heap(span, tx.kind(&value))?;
    Ok(value)
}

#[cfg(test)]
mod tests;
