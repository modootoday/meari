use super::{ComponentState, StateCommit};
use crate::{
    application::{media::MediaChange, responsive::ResponsiveCommit},
    domain::component_state::{InstanceHandle, StateAction, StateValue},
};
use std::collections::{BTreeMap, BTreeSet};

/// One component's useMediaQuery-style boolean slots, tied to responsive revisions.
#[derive(Clone)]
pub struct MediaStateBindings {
    instance: InstanceHandle,
    bindings: BTreeMap<u64, String>,
    last_revision: u64,
}

impl MediaStateBindings {
    pub fn new(instance: InstanceHandle, responsive_revision: u64) -> Result<Self, String> {
        if responsive_revision == 0 {
            return Err("media state requires a published responsive revision".into());
        }
        Ok(Self {
            instance,
            bindings: BTreeMap::new(),
            last_revision: responsive_revision,
        })
    }

    pub fn bind(
        &mut self,
        state: &mut ComponentState,
        initial: &MediaChange,
        slot: String,
    ) -> Result<StateCommit, String> {
        let snapshot = state.snapshot(&self.instance)?;
        let subscription = initial.subscription;
        if subscription == 0
            || self.bindings.contains_key(&subscription)
            || self.bindings.values().any(|value| value == &slot)
            || !matches!(snapshot.values.get(&slot), Some(StateValue::Bool(_)))
        {
            return Err("media binding requires unique subscription/boolean slot".into());
        }
        let commit = state.dispatch(
            &self.instance,
            snapshot.revision,
            &[StateAction::Set {
                slot: slot.clone(),
                value: StateValue::Bool(initial.matches),
            }],
        )?;
        self.bindings.insert(subscription, slot);
        Ok(commit)
    }

    pub fn unbind(&mut self, subscription: u64) -> bool {
        self.bindings.remove(&subscription).is_some()
    }

    pub fn apply(
        &mut self,
        state: &mut ComponentState,
        commit: &ResponsiveCommit,
    ) -> Result<StateCommit, String> {
        if commit.revision <= self.last_revision {
            return Err("stale responsive state commit".into());
        }
        commit.environment.validate()?;
        let mut seen = BTreeSet::new();
        let mut actions = Vec::new();
        for change in &commit.media_changes {
            if change.subscription == 0 || !seen.insert(change.subscription) {
                return Err("invalid/duplicate media change".into());
            }
            if let Some(slot) = self.bindings.get(&change.subscription) {
                actions.push(StateAction::Set {
                    slot: slot.clone(),
                    value: StateValue::Bool(change.matches),
                });
            }
        }
        let revision = state.snapshot(&self.instance)?.revision;
        let result = state.dispatch(&self.instance, revision, &actions)?;
        self.last_revision = commit.revision;
        Ok(result)
    }
}
