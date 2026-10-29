use super::super::hook_values::{Error, Transaction, Value, ValueKind};
use crate::domain::hook_expression::LiteralShape;

pub(super) fn accepts(tx: &Transaction, root: &Value, shape: &LiteralShape) -> Result<bool, Error> {
    fn visit(
        tx: &Transaction,
        root: &Value,
        shape: &LiteralShape,
        depth: usize,
        work: &mut usize,
    ) -> Result<bool, Error> {
        if depth > 64 || *work == 0 {
            return Err(Error::BudgetExceeded);
        }
        *work -= 1;
        Ok(match (tx.kind(root)?, shape) {
            (ValueKind::Null, LiteralShape::Null)
            | (ValueKind::Boolean, LiteralShape::Boolean)
            | (ValueKind::Number, LiteralShape::Number)
            | (ValueKind::String, LiteralShape::Text) => true,
            (ValueKind::Record, LiteralShape::Record(expected)) => {
                let mut count = 0;
                for (key, value) in tx.fields(root)? {
                    count += 1;
                    let Some(shape) = expected.get(key) else {
                        return Ok(false);
                    };
                    if !visit(tx, &value, shape, depth + 1, work)? {
                        return Ok(false);
                    }
                }
                count == expected.len()
            }
            (ValueKind::Array, LiteralShape::Array(allowed)) => {
                let mut index = 0;
                while let Some(value) = tx.element(root, index)? {
                    let mut matched = false;
                    for shape in allowed {
                        if visit(tx, &value, shape, depth + 1, work)? {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        return Ok(false);
                    }
                    index += 1;
                }
                true
            }
            _ => false,
        })
    }
    visit(tx, root, shape, 0, &mut 65_536)
}
