use super::WebDocument;
use crate::{
    application::{
        component_state::media::MediaStateBindings,
        dom,
        media::MediaChange,
        responsive::{ResponsiveCommit, ResponsiveSession},
        state_program::{StateProgram, StateProjection},
    },
    domain::host::HostEnvironment,
    infrastructure::{
        web_host::{self, observer::BrowserHostObserver},
        web_media::BrowserMedia,
    },
};

impl WebDocument {
    /// Measurements are drained outside browser event callbacks by the application scheduler.
    pub fn observe_host(&mut self) -> Result<(), String> {
        self.healthy_host()?;
        if self.host_observer.is_none() {
            self.host_observer = Some(BrowserHostObserver::new()?);
        }
        Ok(())
    }

    pub fn take_host_observation(&mut self) -> Option<Result<HostEnvironment, String>> {
        if self.inner.borrow().closed {
            return None;
        }
        if let Err(error) = self.healthy_host() {
            return Some(Err(error));
        }
        let observed = self.host_observer.as_mut()?.take_pending()?;
        self.pending_environment = observed.as_ref().ok().cloned();
        Some(observed)
    }

    pub fn stop_observing_host(&mut self) -> bool {
        self.pending_environment = None;
        self.host_observer.take().is_some()
    }

    /// Publish the last coalesced browser observation and all media changes atomically.
    pub fn poll_host_environment(&mut self) -> Option<Result<ResponsiveCommit, String>> {
        if let Some(observed) = self.take_host_observation() {
            match observed {
                Ok(environment) => self.pending_environment = Some(environment),
                Err(error) => {
                    self.pending_environment = None;
                    return Some(Err(error));
                }
            }
        }
        self.pending_environment.as_ref()?;
        Some(self.refresh_host_environment())
    }

    /// Explicit refresh also covers media preferences which do not resize a viewport.
    pub fn refresh_host_environment(&mut self) -> Result<ResponsiveCommit, String> {
        self.healthy_host()?;
        if let Some(observer) = &mut self.host_observer {
            let _ = observer.take_pending();
        }
        let environment = match web_host::snapshot() {
            Ok(environment) => environment,
            Err(error) => {
                self.pending_environment = None;
                return Err(error);
            }
        };
        let result = self.commit_observed_environment(environment.clone());
        self.pending_environment = result.is_err().then_some(environment);
        result
    }

    pub fn subscribe_media(&mut self, query: &str) -> Result<MediaChange, String> {
        self.healthy_host()?;
        self.host_responsive
            .as_mut()
            .ok_or("Web host has no committed observation")?
            .subscribe_observed(&mut BrowserMedia, query)
    }

    pub fn unsubscribe_media(&mut self, subscription: u64) -> bool {
        if let Some(bindings) = &mut self.media_bindings {
            bindings.unbind(subscription);
        }
        self.host_responsive
            .as_mut()
            .is_some_and(|session| session.unsubscribe(subscription))
    }

    pub fn subscribe_media_state(
        &mut self,
        query: &str,
        slot: String,
    ) -> Result<MediaChange, String> {
        let initial = self.subscribe_media(query)?;
        let result = (|| {
            let revision = self
                .host_responsive
                .as_ref()
                .ok_or("Web host session disappeared")?
                .revision();
            let mut program = self.inner.borrow().program.clone();
            let mut bindings = match &self.media_bindings {
                Some(bindings) => bindings.clone(),
                None => program.media_bindings(revision)?,
            };
            let projection = program.bind_media(&mut bindings, &initial, slot)?;
            self.commit_media_projection(program, bindings, projection)
        })();
        if result.is_err() {
            self.unsubscribe_media(initial.subscription);
        }
        result.map(|()| initial)
    }

    fn commit_media_projection(
        &mut self,
        program: StateProgram,
        bindings: MediaStateBindings,
        projection: StateProjection,
    ) -> Result<(), String> {
        let plan = dom::prepare(&projection.component)?;
        let mut inner = self.inner.borrow_mut();
        if !inner.events.keys().eq(projection.events.keys()) {
            return Err("media state changed Web handler layout".into());
        }
        let shell = inner.shell.clone();
        if let Err((error, poisoned)) = inner.frame.patch(plan, &shell) {
            inner.poisoned = poisoned;
            inner.last_error = Some(error.clone());
            return Err(error);
        }
        inner.program = program;
        inner.events = projection.events;
        inner.last_error = None;
        self.media_bindings = Some(bindings);
        Ok(())
    }

    fn healthy_host(&self) -> Result<(), String> {
        let inner = self.inner.borrow();
        if inner.closed || inner.poisoned {
            return Err("Web document requires recovery or is closed".into());
        }
        Ok(())
    }

    fn commit_observed_environment(
        &mut self,
        environment: HostEnvironment,
    ) -> Result<ResponsiveCommit, String> {
        self.healthy_host()?;
        let mut session = match &self.host_responsive {
            Some(session) => session.clone(),
            None => ResponsiveSession::new(environment.clone())?,
        };
        let commit = session.observe(&mut BrowserMedia, environment)?;
        if let Some(mut bindings) = self.media_bindings.clone() {
            let mut program = self.inner.borrow().program.clone();
            let projection = program.apply_media(&mut bindings, &commit)?;
            self.commit_media_projection(program, bindings, projection)?;
        }
        self.host_responsive = Some(session);
        Ok(commit)
    }
}
