use super::PreparedDocument;
use crate::{
    application::{
        hook_values::ValueKind,
        state_program::{StateProgram, StateProjection},
    },
    domain::{
        Component, Diagnostic,
        component_state::{InstanceHandle, StateValue},
    },
};
use std::collections::BTreeMap;

impl PreparedDocument {
    pub(crate) fn project(
        &self,
        instance: &InstanceHandle,
        definition: &Component,
    ) -> Result<StateProjection, Diagnostic> {
        let fail = |message: &str| Diagnostic::unsupported(definition.root.span, message);
        let state = self
            .instances
            .get(instance)
            .ok_or_else(|| fail("hook projection instance is not mounted"))?;
        if state.bindings != definition.states {
            return Err(fail("hook projection definition changed"));
        }
        let tx = self.values();
        // These are ephemeral scalar render bindings, never an authoritative state snapshot.
        // Collection roots stay in the original candidate and are captured by bound handlers.
        let mut scalars = BTreeMap::new();
        for (name, root) in &state.roots {
            let value = match tx.kind(root).map_err(|e| fail(&e.to_string()))? {
                ValueKind::Boolean => {
                    StateValue::Bool(tx.read_boolean(root).map_err(|e| fail(&e.to_string()))?)
                }
                ValueKind::String => StateValue::String(
                    tx.read_text(root)
                        .map_err(|e| fail(&e.to_string()))?
                        .to_owned(),
                ),
                ValueKind::Number => StateValue::Number(
                    serde_json::Number::from_f64(
                        tx.read_number(root).map_err(|e| fail(&e.to_string()))?,
                    )
                    .ok_or_else(|| {
                        fail("nonfinite hook projection requires a numeric expression adapter")
                    })?,
                ),
                ValueKind::Array | ValueKind::Record => continue,
                ValueKind::Undefined | ValueKind::Null => {
                    return Err(fail(
                        "nullable hook projection requires a union expression adapter",
                    ));
                }
            };
            scalars.insert(name.clone(), value);
        }
        StateProgram::project_bindings(definition, &scalars, true).map_err(|message| fail(&message))
    }
}
