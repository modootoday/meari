use super::{StateCommit, StateProgram, StateProjection};
use crate::domain::{Component, component_state::InstanceKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadPolicy {
    PreserveCompatible,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReloadDisposition {
    Preserved,
    Reset,
}

/// An unpublished candidate tied to the exact original definition/state revisions.
#[derive(Clone)]
pub struct PreparedReplacement {
    baseline: StateCommit,
    base_definition_revision: u64,
    candidate: StateProgram,
    disposition: ReloadDisposition,
}
impl PreparedReplacement {
    pub fn disposition(&self) -> ReloadDisposition {
        self.disposition
    }
    pub fn definition_revision(&self) -> u64 {
        self.candidate.definition_revision
    }
    pub fn snapshot(&self) -> Result<StateCommit, String> {
        self.candidate.snapshot()
    }
    pub fn project(&self) -> Result<StateProjection, String> {
        self.candidate.project()
    }
}

impl StateProgram {
    pub fn definition_revision(&self) -> u64 {
        self.definition_revision
    }

    pub fn prepare_replacement(
        &self,
        definition: Component,
        policy: ReloadPolicy,
    ) -> Result<PreparedReplacement, String> {
        crate::domain::validation::validate(&definition).map_err(|error| error.message)?;
        let compatible = self.definition.name == definition.name
            && self.definition.hook_order == definition.hook_order
            && self
                .definition
                .states
                .iter()
                .map(|state| (&state.name, state.initial.kind()))
                .eq(definition
                    .states
                    .iter()
                    .map(|state| (&state.name, state.initial.kind())));
        if policy == ReloadPolicy::PreserveCompatible && !compatible {
            return Err("source state layout changed; explicit reset is required".into());
        }
        let mut candidate = self.clone();
        candidate.definition_revision = self
            .definition_revision
            .checked_add(1)
            .ok_or("source definition revision exhausted")?;
        let schema = Self::schema(&definition);
        let disposition = match policy {
            ReloadPolicy::PreserveCompatible => ReloadDisposition::Preserved,
            ReloadPolicy::Reset => {
                candidate.state.unmount(&candidate.owner)?;
                candidate.owner = candidate.state.mount(
                    None,
                    InstanceKey::Position(0),
                    definition.name.clone(),
                    schema,
                )?;
                ReloadDisposition::Reset
            }
        };
        candidate.definition = definition;
        candidate.project()?;
        Ok(PreparedReplacement {
            baseline: self.snapshot()?,
            base_definition_revision: self.definition_revision,
            candidate,
            disposition,
        })
    }

    pub fn commit_replacement(
        &mut self,
        prepared: PreparedReplacement,
    ) -> Result<StateProjection, String> {
        if self.definition_revision != prepared.base_definition_revision
            || self.snapshot()? != prepared.baseline
        {
            return Err(
                "stale source replacement; prepare against the latest state/definition".into(),
            );
        }
        let projection = prepared.candidate.project()?;
        *self = prepared.candidate;
        Ok(projection)
    }
}
