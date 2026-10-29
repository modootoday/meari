use crate::domain::component_state::{
    InstanceHandle, InstanceKey, StateAction, StateSchema, StateValue, identifier,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
pub mod media;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StateCommit {
    pub instance: InstanceHandle,
    pub revision: u64,
    pub changed: bool,
    pub values: BTreeMap<String, StateValue>,
}

#[derive(Clone)]
struct Instance {
    parent: Option<u64>,
    key: InstanceKey,
    definition: String,
    schema: StateSchema,
    revision: u64,
    values: BTreeMap<String, StateValue>,
}

/// The document host owns a unique scope/epoch; mounts never reuse instance IDs.
#[derive(Clone)]
pub struct ComponentState {
    document: String,
    epoch: u64,
    sequence: Arc<AtomicU64>,
    instances: BTreeMap<u64, Instance>,
}

impl ComponentState {
    pub fn new(document: String, epoch: u64) -> Result<Self, String> {
        if !identifier(&document) || epoch == 0 {
            return Err("component document scope/epoch required".into());
        }
        Ok(Self {
            document,
            epoch,
            sequence: Arc::new(AtomicU64::new(0)),
            instances: BTreeMap::new(),
        })
    }

    fn id(&self, handle: &InstanceHandle) -> Result<u64, String> {
        if handle.document != self.document
            || handle.epoch != self.epoch
            || !self.instances.contains_key(&handle.id)
        {
            return Err("stale or foreign component instance".into());
        }
        Ok(handle.id)
    }

    fn handle(&self, id: u64) -> InstanceHandle {
        InstanceHandle {
            document: self.document.clone(),
            epoch: self.epoch,
            id,
        }
    }

    pub fn mount(
        &mut self,
        parent: Option<&InstanceHandle>,
        key: InstanceKey,
        definition: String,
        schema: StateSchema,
    ) -> Result<InstanceHandle, String> {
        schema.validate()?;
        if !identifier(&definition)
            || matches!(&key, InstanceKey::Explicit(value) if value.len() > 1024)
        {
            return Err("invalid component definition or key".into());
        }
        let parent = parent.map(|handle| self.id(handle)).transpose()?;
        let mut ancestor = parent;
        let mut depth = 0;
        while let Some(id) = ancestor {
            depth += 1;
            if depth >= 256 {
                return Err("component depth exceeds 256".into());
            }
            ancestor = self.instances[&id].parent;
        }
        let existing = self
            .instances
            .iter()
            .find(|(_, instance)| instance.parent == parent && instance.key == key)
            .map(|(id, _)| *id);
        if let Some(id) = existing {
            let instance = &self.instances[&id];
            if instance.definition == definition {
                if !instance.schema.same_layout(&schema) {
                    return Err("state schema changed; explicit remount/migration required".into());
                }
                return Ok(self.handle(id));
            }
        }
        // Identity reservations survive discarded staging clones; state values do not.
        let id = self
            .sequence
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "component instance IDs exhausted")?
            + 1;
        if let Some(existing) = existing {
            self.remove_tree(existing);
        }
        self.instances.insert(
            id,
            Instance {
                parent,
                key,
                definition,
                values: schema
                    .slots
                    .iter()
                    .map(|(name, slot)| (name.clone(), slot.initial.clone()))
                    .collect(),
                schema,
                revision: 0,
            },
        );
        Ok(self.handle(id))
    }

    fn remove_tree(&mut self, id: u64) {
        let children: Vec<_> = self
            .instances
            .iter()
            .filter(|(_, instance)| instance.parent == Some(id))
            .map(|(id, _)| *id)
            .collect();
        for child in children {
            self.remove_tree(child);
        }
        self.instances.remove(&id);
    }

    pub fn unmount(&mut self, handle: &InstanceHandle) -> Result<(), String> {
        let id = self.id(handle)?;
        self.remove_tree(id);
        Ok(())
    }

    pub fn snapshot(&self, handle: &InstanceHandle) -> Result<StateCommit, String> {
        let instance = &self.instances[&self.id(handle)?];
        Ok(StateCommit {
            instance: handle.clone(),
            revision: instance.revision,
            changed: false,
            values: instance.values.clone(),
        })
    }

    pub(crate) fn retain_instances(
        &mut self,
        active: &std::collections::BTreeSet<InstanceHandle>,
    ) -> Result<(), String> {
        let ids = active
            .iter()
            .map(|handle| self.id(handle))
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        if ids.iter().any(|id| {
            self.instances[id]
                .parent
                .is_some_and(|parent| !ids.contains(&parent))
        }) {
            return Err("active component census omitted an ancestor".into());
        }
        self.instances.retain(|id, _| ids.contains(id));
        Ok(())
    }

    /// Updates are evaluated against staged latest values; a failed batch commits nothing.
    pub fn dispatch(
        &mut self,
        handle: &InstanceHandle,
        expected_revision: u64,
        actions: &[StateAction],
    ) -> Result<StateCommit, String> {
        let id = self.id(handle)?;
        let instance = &self.instances[&id];
        if instance.revision != expected_revision {
            return Err("stale component state revision".into());
        }
        if actions.len() > 1024 {
            return Err("component action batch exceeds 1024".into());
        }
        let mut values = instance.values.clone();
        for action in actions {
            let value = values
                .get_mut(action.slot())
                .ok_or_else(|| format!("unknown state slot: {}", action.slot()))?;
            match action {
                StateAction::Set { value: next, .. } if value.kind() == next.kind() => {
                    *value = next.clone()
                }
                StateAction::Toggle { .. } => match value {
                    StateValue::Bool(value) => *value = !*value,
                    _ => return Err("toggle requires boolean state".into()),
                },
                StateAction::Add { delta, .. } => match value {
                    StateValue::Number(number) => *number = add(number, delta)?,
                    _ => return Err("add requires numeric state".into()),
                },
                _ => return Err("state value does not match declared type".into()),
            }
        }
        let changed = values != instance.values;
        let revision = if changed {
            instance
                .revision
                .checked_add(1)
                .ok_or("component revision exhausted")?
        } else {
            instance.revision
        };
        let instance = self.instances.get_mut(&id).ok_or("component disappeared")?;
        instance.values = values.clone();
        instance.revision = revision;
        Ok(StateCommit {
            instance: handle.clone(),
            revision,
            changed,
            values,
        })
    }
}

fn add(
    left: &serde_json::Number,
    right: &serde_json::Number,
) -> Result<serde_json::Number, String> {
    let integer = |number: &serde_json::Number| {
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
    };
    if let (Some(left), Some(right)) = (integer(left), integer(right)) {
        let sum = left + right;
        if let Ok(value) = i64::try_from(sum) {
            return Ok(value.into());
        }
        if let Ok(value) = u64::try_from(sum) {
            return Ok(value.into());
        }
        return Err("integer state overflow".into());
    }
    serde_json::Number::from_f64(
        left.as_f64().ok_or("invalid numeric state")?
            + right.as_f64().ok_or("invalid numeric delta")?,
    )
    .ok_or_else(|| "non-finite numeric state".into())
}
