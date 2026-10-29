//! Admission for the controlled Boolean widget subset implemented by the Native adapter.
use meari::domain::{Attribute, Value};

pub(super) fn validate(tag: &str, event: &str, attributes: &[Attribute]) -> Result<(), String> {
    let controlled = match (tag, event) {
        ("Dialog" | "AlertDialog", "onOpenChange") => "open",
        ("Switch" | "Checkbox", "onCheckedChange") => "checked",
        _ => {
            return Err(format!(
                "native Boolean callback adapter unavailable: {tag}.{event}"
            ));
        }
    };
    match attributes
        .iter()
        .find(|attribute| attribute.name == controlled)
        .map(|attribute| &attribute.value)
    {
        Some(Value::Bool(_) | Value::Prop(_) | Value::NotProp(_) | Value::Predicate(_)) => Ok(()),
        _ => Err(format!(
            "native Boolean callbacks require explicit controlled {tag}.{controlled}"
        )),
    }
}
