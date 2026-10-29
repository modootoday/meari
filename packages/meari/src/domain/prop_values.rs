//! One budget across all immutable prop containers, including alternating records/arrays.
use super::{PropType, Value, array_value::ArrayValue, record_value::RecordValue};
use std::collections::BTreeSet;

#[derive(Default)]
struct Budget {
    items: usize,
    bytes: usize,
}

impl Budget {
    fn item(&mut self) -> Result<(), String> {
        self.items = self
            .items
            .checked_add(1)
            .ok_or("prop item budget overflow")?;
        if self.items > 16384 {
            return Err("prop exceeds total item budget".into());
        }
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .ok_or("prop byte budget overflow")?;
        if self.bytes > 1024 * 1024 {
            return Err("prop exceeds total byte budget".into());
        }
        Ok(())
    }
}

fn type_visit(kind: &PropType, depth: usize, budget: &mut Budget) -> Result<(), String> {
    if depth > 32 {
        return Err("prop type exceeds container depth budget".into());
    }
    budget.item()?;
    match kind {
        PropType::Array(element) => {
            if **element == PropType::Node {
                return Err("node slots do not support iterable array props".into());
            }
            type_visit(element, depth + 1, budget)?;
        }
        PropType::Record(fields) => {
            let mut names = BTreeSet::new();
            for field in fields {
                if !super::validation::identifier(&field.name) || !names.insert(&field.name) {
                    return Err("record fields require unique portable identifiers".into());
                }
                if field.kind == PropType::Node {
                    return Err("node slots are not immutable record fields".into());
                }
                budget.text(&field.name)?;
                type_visit(&field.kind, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_type(kind: &PropType) -> Result<(), String> {
    type_visit(kind, 0, &mut Budget::default())
}

fn value_visit(
    kind: &PropType,
    value: &Value,
    depth: usize,
    budget: &mut Budget,
) -> Result<(), String> {
    if depth > 32 {
        return Err("prop value exceeds container depth budget".into());
    }
    budget.item()?;
    match (kind, value) {
        (PropType::String, Value::String(text)) => budget.text(text),
        (PropType::Bool, Value::Bool(_)) => Ok(()),
        (PropType::Callback(signature), Value::CallbackReference(reference))
            if *signature == reference.signature =>
        {
            reference.validate()?;
            budget.text(&reference.binding)
        }
        (PropType::Array(element), Value::Array(array)) if element.as_ref() == &array.element => {
            for item in &array.items {
                value_visit(element, item, depth + 1, budget)?;
            }
            Ok(())
        }
        (PropType::Record(schema), Value::Record(record))
            if super::record_value::same_schema(schema, record.schema()) =>
        {
            record_visit(record, depth, budget)
        }
        _ => Err("prop value differs from its declared type".into()),
    }
}

fn record_visit(record: &RecordValue, depth: usize, budget: &mut Budget) -> Result<(), String> {
    if record.fields().len() != record.schema().len() {
        return Err("record requires exact declared fields".into());
    }
    for field in record.schema() {
        budget.text(&field.name)?;
        let value = record
            .get(&field.name)
            .ok_or("record is missing a declared field")?;
        value_visit(&field.kind, value, depth + 1, budget)?;
    }
    Ok(())
}

pub(super) fn validate_record(record: &RecordValue) -> Result<(), String> {
    validate_type(&record.kind())?;
    record_visit(record, 0, &mut Budget::default())
}

pub(super) fn validate_array(array: &ArrayValue) -> Result<(), String> {
    type_visit(&array.element, 1, &mut Budget::default())?;
    let mut budget = Budget::default();
    for item in &array.items {
        value_visit(&array.element, item, 1, &mut budget)?;
    }
    Ok(())
}
