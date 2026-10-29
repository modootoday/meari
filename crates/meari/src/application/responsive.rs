use super::media::{MediaChange, MediaQueryEvaluator, MediaSubscriptions};
use crate::domain::host::HostEnvironment;

pub trait ResponsiveRenderer: MediaQueryEvaluator {
    /// Apply final CSS viewport/style before media subscriptions are evaluated.
    fn apply_environment(&mut self, environment: &HostEnvironment) -> Result<(), String>;
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ResponsiveCommit {
    pub revision: u64,
    pub environment: HostEnvironment,
    pub media_changes: Vec<MediaChange>,
}

/// Owns the last successfully rendered environment and media notification baseline.
#[derive(Clone)]
pub struct ResponsiveSession {
    committed: HostEnvironment,
    revision: u64,
    media: MediaSubscriptions,
    poisoned: bool,
}

impl ResponsiveSession {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn new(environment: HostEnvironment) -> Result<Self, String> {
        environment.validate()?;
        Ok(Self {
            committed: environment,
            revision: 0,
            media: MediaSubscriptions::default(),
            poisoned: false,
        })
    }

    pub fn subscribe(
        &mut self,
        renderer: &mut dyn ResponsiveRenderer,
        query: &str,
    ) -> Result<MediaChange, String> {
        self.subscribe_observed(renderer, query)
    }

    pub fn subscribe_observed(
        &mut self,
        evaluator: &mut dyn MediaQueryEvaluator,
        query: &str,
    ) -> Result<MediaChange, String> {
        if self.poisoned || self.revision == 0 {
            return Err("responsive renderer has no healthy commit".into());
        }
        self.media.subscribe(evaluator, query)
    }

    pub fn unsubscribe(&mut self, subscription: u64) -> bool {
        self.media.unsubscribe(subscription)
    }

    pub fn apply(
        &mut self,
        renderer: &mut dyn ResponsiveRenderer,
        observed: HostEnvironment,
    ) -> Result<ResponsiveCommit, String> {
        let revision = self.check_environment(&observed)?;
        let mut media = self.media.clone();
        let update = renderer
            .apply_environment(&observed)
            .and_then(|()| media.refresh(renderer));
        let changes = match update {
            Ok(changes) => changes,
            Err(error) => {
                if let Err(rollback) = renderer.apply_environment(&self.committed) {
                    self.poisoned = true;
                    return Err(format!(
                        "responsive update failed: {error}; rollback failed: {rollback}"
                    ));
                }
                return Err(error);
            }
        };
        Ok(self.publish(observed, revision, media, changes))
    }

    /// Publish an externally applied host viewport, without attempting to resize or restore it.
    pub fn observe(
        &mut self,
        evaluator: &mut dyn MediaQueryEvaluator,
        observed: HostEnvironment,
    ) -> Result<ResponsiveCommit, String> {
        let revision = self.check_environment(&observed)?;
        let mut media = self.media.clone();
        let changes = media.refresh(evaluator)?;
        Ok(self.publish(observed, revision, media, changes))
    }

    fn check_environment(&self, observed: &HostEnvironment) -> Result<u64, String> {
        if self.poisoned {
            return Err("responsive renderer requires recovery".into());
        }
        observed.validate()?;
        if observed.target != self.committed.target {
            return Err("responsive host target changed".into());
        }
        self.revision
            .checked_add(1)
            .ok_or_else(|| "responsive revision exhausted".into())
    }

    fn publish(
        &mut self,
        observed: HostEnvironment,
        revision: u64,
        media: MediaSubscriptions,
        changes: Vec<MediaChange>,
    ) -> ResponsiveCommit {
        self.revision = revision;
        self.media = media;
        self.committed = observed.clone();
        ResponsiveCommit {
            revision,
            environment: observed,
            media_changes: changes,
        }
    }

    pub fn recover(&mut self, renderer: &mut dyn ResponsiveRenderer) -> Result<(), String> {
        self.poisoned = true;
        renderer.apply_environment(&self.committed)?;
        self.poisoned = false;
        Ok(())
    }
}
