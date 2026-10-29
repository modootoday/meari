use super::{StateProgram, StateProjection};
use crate::domain::{
    component_state::InstanceHandle,
    input_event::{InputPayload, InputStateBinding},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputHandler {
    instance: InstanceHandle,
    definition_revision: u64,
    binding: InputStateBinding,
}

impl StateProgram {
    pub fn input_handler(&self, binding: InputStateBinding) -> Result<InputHandler, String> {
        binding.validate(&self.definition.states)?;
        Ok(InputHandler {
            instance: self.owner.clone(),
            definition_revision: self.definition_revision,
            binding,
        })
    }

    pub fn dispatch_input(
        &mut self,
        handler: &InputHandler,
        payload: &InputPayload,
    ) -> Result<StateProjection, String> {
        if handler.instance != self.owner || handler.definition_revision != self.definition_revision
        {
            return Err("stale or foreign source input handler".into());
        }
        let action = handler.binding.action(&self.definition.states, payload)?;
        let revision = self.snapshot()?.revision;
        let mut program = self.clone();
        program
            .state
            .dispatch(&program.owner, revision, &[action])?;
        let projection = program.project()?;
        *self = program;
        Ok(projection)
    }
}
