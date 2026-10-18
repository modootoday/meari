//! Shared Native/Web publication state machine. Hosts acknowledge real commits.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum Target {
    Native,
    Web,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Artifact {
    pub source_hash: String,
    pub artifact_id: String,
    pub state_schema: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Publication {
    pub revision: u64,
    pub artifacts: BTreeMap<Target, Artifact>,
}

struct Pending {
    revision: u64,
    source_hash: String,
    artifacts: BTreeMap<Target, Artifact>,
    published: bool,
    rollback: bool,
    acknowledgements: BTreeSet<Target>,
}

#[derive(Default)]
pub struct Coordinator {
    sequence: u64,
    pending: Option<Pending>,
    last_good: Option<Publication>,
}

impl Coordinator {
    pub fn request(&mut self, source_hash: String) -> Result<u64, String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.published)
        {
            return Err(
                "HMR commit/rollback in flight; coalesce edits until both hosts acknowledge".into(),
            );
        }
        if source_hash.is_empty() {
            return Err("source hash required".into());
        }
        let revision = self
            .sequence
            .checked_add(1)
            .ok_or("HMR revision exhausted")?;
        self.sequence = revision;
        self.pending = Some(Pending {
            revision,
            source_hash,
            artifacts: BTreeMap::new(),
            published: false,
            rollback: false,
            acknowledgements: BTreeSet::new(),
        });
        Ok(revision)
    }

    fn pending(&mut self, revision: u64) -> Result<&mut Pending, String> {
        self.pending
            .as_mut()
            .filter(|pending| pending.revision == revision)
            .ok_or_else(|| "stale HMR revision".into())
    }

    pub fn prepare(
        &mut self,
        revision: u64,
        target: Target,
        artifact: Artifact,
    ) -> Result<(), String> {
        let pending = self.pending(revision)?;
        if pending.published
            || pending.artifacts.contains_key(&target)
            || artifact.source_hash != pending.source_hash
            || artifact.artifact_id.is_empty()
            || artifact.state_schema.is_empty()
        {
            return Err("invalid, duplicate or late target preparation".into());
        }
        pending.artifacts.insert(target, artifact);
        Ok(())
    }

    pub fn publish(&mut self, revision: u64) -> Result<Publication, String> {
        let pending = self.pending(revision)?;
        if pending.published || pending.artifacts.len() != 2 {
            return Err("both target preparations required exactly once".into());
        }
        pending.published = true;
        Ok(Publication {
            revision,
            artifacts: pending.artifacts.clone(),
        })
    }

    /// False means the other host has not acknowledged yet.
    pub fn acknowledge(&mut self, revision: u64, target: Target) -> Result<bool, String> {
        let pending = self.pending(revision)?;
        if !pending.published || pending.rollback || !pending.acknowledgements.insert(target) {
            return Err("unexpected/duplicate frame acknowledgement".into());
        }
        if pending.acknowledgements.len() != 2 {
            return Ok(false);
        }
        self.last_good = Some(Publication {
            revision,
            artifacts: pending.artifacts.clone(),
        });
        self.pending = None;
        Ok(true)
    }

    /// A published failure requires BOTH hosts to restore this last-good frame.
    /// None means restore the initial empty host when there is no previous frame.
    pub fn fail(&mut self, revision: u64) -> Result<Option<Publication>, String> {
        let pending = self.pending(revision)?;
        if pending.rollback {
            return Err("rollback already pending".into());
        }
        if pending.published {
            pending.rollback = true;
            pending.acknowledgements.clear();
        } else {
            self.pending = None;
        }
        Ok(self.last_good.clone())
    }

    pub fn acknowledge_rollback(&mut self, revision: u64, target: Target) -> Result<bool, String> {
        let pending = self.pending(revision)?;
        if !pending.rollback || !pending.acknowledgements.insert(target) {
            return Err("unexpected/duplicate rollback acknowledgement".into());
        }
        if pending.acknowledgements.len() != 2 {
            return Ok(false);
        }
        self.pending = None;
        Ok(true)
    }

    pub fn last_good(&self) -> Option<&Publication> {
        self.last_good.as_ref()
    }
}
