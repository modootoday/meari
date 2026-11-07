use super::super::document::InteractiveDocument;
use meari::{
    application::{
        media::MediaChange,
        responsive::{ResponsiveCommit, ResponsiveSession},
    },
    domain::host::HostEnvironment,
};

impl InteractiveDocument {
    pub fn subscribe_media_state(
        &mut self,
        query: &str,
        slot: String,
    ) -> Result<MediaChange, String> {
        let initial = self.subscribe_media(query)?;
        let result = (|| {
            let revision = self
                .responsive
                .as_ref()
                .ok_or("responsive host disappeared")?
                .revision();
            let mut program = self
                .state_program
                .as_ref()
                .ok_or("document has no source state")?
                .clone();
            let mut bindings = match &self.media_bindings {
                Some(bindings) => bindings.clone(),
                None => program.media_bindings(revision)?,
            };
            let projection = program.bind_media(&mut bindings, &initial, slot)?;
            self.commit_state_projection(program, projection)?;
            self.media_bindings = Some(bindings);
            Ok(())
        })();
        if result.is_err() {
            self.unsubscribe_media(initial.subscription);
        }
        result.map(|()| initial)
    }

    /// Refresh an observed window layout without reparsing/recompiling the source.
    /// Retains DOM node identities, focus, input state and click/AX metadata. Media-bound
    /// state may still project changes; this API does not acknowledge a requested OS resize.
    pub fn refresh_window_layout(
        &mut self,
        environment: HostEnvironment,
    ) -> Result<ResponsiveCommit, String> {
        self.commit_host_environment(environment)
    }

    pub fn commit_host_environment(
        &mut self,
        environment: HostEnvironment,
    ) -> Result<ResponsiveCommit, String> {
        let previous = self.responsive.clone();
        let previous_viewport = self.viewport.clone();
        let mut session = match &previous {
            Some(session) => session.clone(),
            None => ResponsiveSession::new(environment.clone())?,
        };
        let layout_start = std::time::Instant::now();
        let commit = match session.apply(&mut self.renderer, environment) {
            Ok(commit) => commit,
            Err(error) => {
                if previous.is_some() {
                    self.responsive = Some(session);
                }
                return Err(error);
            }
        };
        let layout = layout_start.elapsed();
        let source_start = std::time::Instant::now();
        if let Err(error) = self.apply_source_media(&commit) {
            let mut rollback = previous.ok_or("media state has no host rollback baseline")?;
            let restored = rollback.recover(&mut self.renderer);
            self.responsive = Some(rollback);
            return match restored {
                Ok(()) => {
                    self.viewport = previous_viewport;
                    Err(error)
                }
                Err(restore) => Err(format!(
                    "media state update failed: {error}; host rollback failed: {restore}"
                )),
            };
        }
        self.environment_timings = (layout, source_start.elapsed());
        self.responsive = Some(session);
        self.viewport = commit.environment.layout.clone();
        self.media_changes.extend(commit.media_changes.clone());
        Ok(commit)
    }

    fn apply_source_media(&mut self, commit: &ResponsiveCommit) -> Result<(), String> {
        // A size-only change with unchanged queries needs layout, not an entire
        // retained-source projection/DOM patch. Bindings accept revision gaps.
        if commit.media_changes.is_empty() {
            return Ok(());
        }
        let Some(mut bindings) = self.media_bindings.clone() else {
            return Ok(());
        };
        let mut program = self
            .state_program
            .as_ref()
            .ok_or("media state program disappeared")?
            .clone();
        let projection = program.apply_media(&mut bindings, commit)?;
        self.commit_state_projection(program, projection)?;
        self.media_bindings = Some(bindings);
        Ok(())
    }
}
