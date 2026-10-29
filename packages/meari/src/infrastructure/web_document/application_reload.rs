use super::{PreparedSourceReplacement, WebDocument, frame::error};
use crate::application::app_bundle::{AppBuildStamp, ApplicationDefinition};

pub struct PreparedApplicationReplacement {
    baseline: AppBuildStamp,
    definition: ApplicationDefinition,
    source: PreparedSourceReplacement,
}
pub struct CommittedApplicationReplacement {
    stamp: AppBuildStamp,
    previous: ApplicationDefinition,
    instance: crate::domain::component_state::InstanceHandle,
    definition_revision: u64,
}
impl CommittedApplicationReplacement {
    pub fn stamp(&self) -> &AppBuildStamp {
        &self.stamp
    }
}

impl WebDocument {
    pub fn application_stamp(&self) -> Option<AppBuildStamp> {
        self.inner
            .borrow()
            .application
            .as_ref()
            .map(|application| application.stamp.clone())
    }

    pub fn prepare_application_replacement(
        &self,
        definition: ApplicationDefinition,
    ) -> Result<PreparedApplicationReplacement, String> {
        let baseline = {
            let inner = self.inner.borrow();
            let current = inner
                .application
                .as_ref()
                .ok_or("Web document has no application build identity")?;
            definition.validate_successor(current)?;
            if definition.stamp.generation <= inner.application_generation {
                return Err("Web application generation was already committed".into());
            }
            if definition.title != current.title
                || definition.native_viewport != current.native_viewport
            {
                return Err(
                    "application title/viewport update requires a host configuration adapter"
                        .into(),
                );
            }
            current.stamp.clone()
        };
        let source = self.prepare_source_replacement(&definition.component, &definition.props)?;
        Ok(PreparedApplicationReplacement {
            baseline,
            definition,
            source,
        })
    }

    /// DOM commit only. A browser frame acknowledgement belongs to the host scheduler.
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
        if self.application_stamp().as_ref() != Some(&prepared.baseline) {
            return Err("stale Web application replacement".into());
        }
        let (previous, instance, definition_revision) = {
            let inner = self.inner.borrow();
            (
                inner
                    .application
                    .as_ref()
                    .ok_or("Web application disappeared")?
                    .clone(),
                inner.program.snapshot()?.instance,
                inner
                    .program
                    .definition_revision()
                    .checked_add(1)
                    .ok_or("source definition revision exhausted")?,
            )
        };
        self.commit_source_replacement(prepared.source)?;
        let stamp = self.publish_application(prepared.definition)?;
        Ok(CommittedApplicationReplacement {
            stamp,
            previous,
            instance,
            definition_revision,
        })
    }

    pub fn rollback_application_replacement(
        &mut self,
        committed: CommittedApplicationReplacement,
    ) -> Result<AppBuildStamp, String> {
        {
            let inner = self.inner.borrow();
            if inner
                .application
                .as_ref()
                .map(|application| &application.stamp)
                != Some(&committed.stamp)
                || inner.program.definition_revision() != committed.definition_revision
                || inner.program.snapshot()?.instance != committed.instance
            {
                return Err("stale or foreign Web application rollback".into());
            }
        }
        let source = self
            .prepare_source_replacement(&committed.previous.component, &committed.previous.props)?;
        self.commit_source_replacement(source)?;
        self.publish_application(committed.previous)
    }

    fn publish_application(
        &mut self,
        definition: ApplicationDefinition,
    ) -> Result<AppBuildStamp, String> {
        let mut inner = self.inner.borrow_mut();
        let stamp = definition.stamp.clone();
        let result = inner
            .shell
            .set_attribute("data-meari-graph", stamp.graph_hash.as_str())
            .and_then(|()| {
                inner
                    .shell
                    .set_attribute("data-meari-generation", &stamp.generation.to_string())
            });
        if let Err(cause) = result {
            let message = error(cause);
            inner.poisoned = true;
            inner.last_error = Some(message.clone());
            return Err(message);
        }
        inner.application = Some(definition);
        inner.application_generation = inner.application_generation.max(stamp.generation);
        Ok(stamp)
    }
}
