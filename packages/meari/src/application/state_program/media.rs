use super::{StateProgram, StateProjection};
use crate::application::{
    component_state::{ComponentState, StateCommit, media::MediaStateBindings},
    media::MediaChange,
    responsive::ResponsiveCommit,
};

impl StateProgram {
    pub fn media_bindings(&self, responsive_revision: u64) -> Result<MediaStateBindings, String> {
        MediaStateBindings::new(self.owner.clone(), responsive_revision)
    }

    pub fn bind_media(
        &mut self,
        bindings: &mut MediaStateBindings,
        initial: &MediaChange,
        slot: String,
    ) -> Result<StateProjection, String> {
        self.stage_media(bindings, |bindings, state| {
            bindings.bind(state, initial, slot)
        })
    }

    pub fn apply_media(
        &mut self,
        bindings: &mut MediaStateBindings,
        commit: &ResponsiveCommit,
    ) -> Result<StateProjection, String> {
        self.stage_media(bindings, |bindings, state| bindings.apply(state, commit))
    }

    fn stage_media(
        &mut self,
        bindings: &mut MediaStateBindings,
        update: impl FnOnce(&mut MediaStateBindings, &mut ComponentState) -> Result<StateCommit, String>,
    ) -> Result<StateProjection, String> {
        let mut program = self.clone();
        let mut staged = bindings.clone();
        update(&mut staged, &mut program.state)?;
        let projection = program.project()?;
        *self = program;
        *bindings = staged;
        Ok(projection)
    }
}
