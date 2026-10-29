//! Prepared typed array binding value; retain the declared element type for empty arrays.
use super::{PropType, Value};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayValue {
    pub element: PropType,
    pub items: Vec<Value>,
}
impl ArrayValue {
    pub fn strings(values: Vec<String>) -> Self {
        Self {
            element: PropType::String,
            items: values.into_iter().map(Value::String).collect(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        super::prop_values::validate_array(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_ordered_and_nested_arrays_preserve_declared_type_and_all_values() {
        let empty = ArrayValue::strings(Vec::new());
        empty.validate().unwrap();
        let values = ArrayValue::strings(vec!["cue-a".into(), "근거🦀".into()]);
        values.validate().unwrap();
        assert_eq!(values.items.len(), 2);
        let nested = ArrayValue {
            element: PropType::Array(Box::new(PropType::String)),
            items: vec![Value::Array(empty), Value::Array(values.clone())],
        };
        nested.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<ArrayValue>(&serde_json::to_string(&nested).unwrap()).unwrap(),
            nested
        );
        assert_eq!(values.items[1], Value::String("근거🦀".into()));
    }
    #[test]
    fn wrong_type_and_recursive_item_byte_limits_are_explicit_errors() {
        assert!(
            ArrayValue {
                element: PropType::Bool,
                items: vec![Value::String("false".into())]
            }
            .validate()
            .is_err()
        );
        assert!(
            ArrayValue::strings(vec![String::new(); 16385])
                .validate()
                .is_err()
        );
        assert!(
            ArrayValue::strings(vec!["x".repeat(1024 * 1024 + 1)])
                .validate()
                .is_err()
        );
    }
}
