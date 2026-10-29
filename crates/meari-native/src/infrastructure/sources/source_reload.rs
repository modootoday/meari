use super::super::document::InteractiveDocument;
use crate::{
    application::{markup, state_program},
    domain::widgets::Session,
};
use meari::{
    application::state_program::{PreparedReplacement, ReloadPolicy},
    domain::{Component, Value, source_schema::SourceSchema},
};
use std::collections::BTreeMap;

pub struct PreparedSourceReplacement {
    program: PreparedReplacement,
}

impl InteractiveDocument {
    /// Retained DOM replacement. CSS and structural remount are separate operations.
    pub fn prepare_source_replacement(
        &self,
        source: Component,
    ) -> Result<PreparedSourceReplacement, String> {
        self.prepare_source_replacement_with_props(source, &BTreeMap::new())
    }

    pub fn prepare_source_replacement_with_props(
        &self,
        source: Component,
        props: &BTreeMap<String, Value>,
    ) -> Result<PreparedSourceReplacement, String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        if SourceSchema::from_component(&source)? != self.source_schema {
            return Err("original source hook/props schema changed; remount is required".into());
        }
        let source = meari::application::binding::bind(&source, props)?;
        state_program::validate(&source)?;
        let program = self
            .state_program
            .as_ref()
            .ok_or("source replacement requires an owned state program")?
            .prepare_replacement(source, ReloadPolicy::PreserveCompatible)?;
        let projection = program.project()?;
        self.validate_input_replacement(&projection.inputs)?;
        state_program::retained_shape(&self.source.root, &projection.component.root)?;
        let session = self
            .session
            .migrate_retained(Session::from_tree(&projection.component.root)?)?;
        markup::build(&projection.component, &session)?;
        Ok(PreparedSourceReplacement { program })
    }

    pub fn commit_source_replacement(
        &mut self,
        prepared: PreparedSourceReplacement,
    ) -> Result<(), String> {
        if self.source_input_poisoned {
            return Err("source input document requires remount after failed recovery".into());
        }
        let mut program = self
            .state_program
            .as_ref()
            .ok_or("source replacement runtime disappeared")?
            .clone();
        let projection = program.commit_replacement(prepared.program)?;
        self.validate_input_replacement(&projection.inputs)?;
        state_program::retained_shape(&self.source.root, &projection.component.root)?;
        let session = self
            .session
            .migrate_retained(Session::from_tree(&projection.component.root)?)?;
        let old_paths = session
            .widgets
            .keys()
            .map(|path| (path.clone(), path.clone()))
            .collect();
        let focus_plan = self.prepare_controlled_focus(&session, &old_paths);
        let old_source = std::mem::replace(&mut self.source, projection.component);
        let old_session = std::mem::replace(&mut self.session, session);
        if let Err(error) = self
            .patch()
            .and_then(|()| self.apply_controlled_focus(&focus_plan))
        {
            self.source = old_source;
            self.session = old_session;
            if let Err(recovery) = self
                .patch()
                .and_then(|()| self.restore_controlled_focus(&focus_plan))
            {
                self.source_input_poisoned = true;
                return Err(format!(
                    "{error}; controlled source recovery failed: {recovery}"
                ));
            }
            return Err(error);
        }
        self.state_program = Some(program);
        self.state_events = projection.events;
        self.state_inputs = projection.inputs;
        Ok(())
    }

    fn validate_input_replacement(
        &self,
        next: &BTreeMap<String, meari::domain::input_event::InputStateBinding>,
    ) -> Result<(), String> {
        if self.state_inputs.len() != next.len()
            || self
                .state_inputs
                .iter()
                .any(|(path, binding)| next.get(path).is_none_or(|next| next.kind != binding.kind))
        {
            return Err("source input ownership changed; document remount is required".into());
        }
        Ok(())
    }
}
