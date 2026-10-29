use super::{PreparedSourceReplacement, document::InteractiveDocument};
use meari::application::app_bundle::{AppBuildStamp, ApplicationDefinition};

pub struct PreparedApplicationReplacement {
    baseline: AppBuildStamp,
    definition: ApplicationDefinition,
    source: PreparedSourceReplacement,
}

pub struct CommittedApplicationReplacement {
    stamp: AppBuildStamp,
    previous: ApplicationDefinition,
    instance: meari::domain::component_state::InstanceHandle,
    definition_revision: u64,
}
impl CommittedApplicationReplacement {
    pub fn stamp(&self) -> &AppBuildStamp {
        &self.stamp
    }
}

impl InteractiveDocument {
    pub fn application_stamp(&self) -> Option<&AppBuildStamp> {
        self.application
            .as_ref()
            .map(|application| &application.stamp)
    }

    pub fn prepare_application_replacement(
        &self,
        definition: ApplicationDefinition,
    ) -> Result<PreparedApplicationReplacement, String> {
        let current = self
            .application
            .as_ref()
            .ok_or("document has no application build identity")?;
        definition.validate_successor(current)?;
        if definition.stamp.generation <= self.application_generation {
            return Err("application generation was already committed".into());
        }
        if definition.title != current.title
            || definition.native_viewport != current.native_viewport
        {
            return Err(
                "application title/viewport update requires a host configuration adapter".into(),
            );
        }
        let source = self.prepare_source_replacement_with_props(
            definition.component.clone(),
            &definition.props,
        )?;
        Ok(PreparedApplicationReplacement {
            baseline: current.stamp.clone(),
            definition,
            source,
        })
    }

    /// Returns after the retained DOM commit; a host must separately acknowledge its rendered frame.
    pub fn commit_application_replacement(
        &mut self,
        prepared: PreparedApplicationReplacement,
    ) -> Result<AppBuildStamp, String> {
        Ok(self
            .commit_reversible_application_replacement(prepared)?
            .stamp)
    }

    pub fn commit_reversible_application_replacement(
        &mut self,
        prepared: PreparedApplicationReplacement,
    ) -> Result<CommittedApplicationReplacement, String> {
        if self.application_stamp() != Some(&prepared.baseline) {
            return Err("stale application replacement".into());
        }
        let previous = self
            .application
            .as_ref()
            .ok_or("application disappeared")?
            .clone();
        let program = self
            .state_program
            .as_ref()
            .ok_or("application state disappeared")?;
        let instance = program.snapshot()?.instance;
        let definition_revision = program
            .definition_revision()
            .checked_add(1)
            .ok_or("source definition revision exhausted")?;
        self.commit_source_replacement(prepared.source)?;
        let stamp = prepared.definition.stamp.clone();
        self.application_generation = stamp.generation;
        self.application_definition_revision = definition_revision;
        self.application = Some(prepared.definition);
        Ok(CommittedApplicationReplacement {
            stamp,
            previous,
            instance,
            definition_revision,
        })
    }

    /// Restore the previous source while preserving the latest compatible user state and DOM.
    pub fn rollback_application_replacement(
        &mut self,
        committed: CommittedApplicationReplacement,
    ) -> Result<AppBuildStamp, String> {
        let program = self
            .state_program
            .as_ref()
            .ok_or("application state disappeared")?;
        if self.application_stamp() != Some(&committed.stamp)
            || program.definition_revision() != committed.definition_revision
            || program.snapshot()?.instance != committed.instance
        {
            return Err("stale or foreign application rollback".into());
        }
        let restored_revision = program
            .definition_revision()
            .checked_add(1)
            .ok_or("source definition revision exhausted")?;
        let source = self.prepare_source_replacement_with_props(
            committed.previous.component.clone(),
            &committed.previous.props,
        )?;
        self.commit_source_replacement(source)?;
        let stamp = committed.previous.stamp.clone();
        self.application = Some(committed.previous);
        self.application_definition_revision = restored_revision;
        Ok(stamp)
    }
}
