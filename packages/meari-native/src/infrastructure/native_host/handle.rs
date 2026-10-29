use super::types::*;
use crate::infrastructure::document::InteractiveDocument;
use meari::domain::plugins::PluginScope;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_HOST: AtomicU64 = AtomicU64::new(1);

pub(super) struct Shared {
    pub(super) observation: super::retained_observation::ObservationState,
    pub(super) hooks: RefCell<Option<super::hooks::HookRuntime>>,
    pub document: RefCell<Option<InteractiveDocument>>,
    pub lifecycle: Cell<HostLifecycle>,
    pub input_blocked: Cell<bool>,
    input_dispatched: Cell<bool>,
    id: u64,
    generation: u64,
    standalone_scope: Option<PluginScope>,
    revision: Cell<u64>,
    turn: Cell<u64>,
    close: RefCell<Option<CallbackCloseReceipt>>,
}

/// Rc deliberately keeps the document and its renderer on their owning UI thread.
#[derive(Clone)]
pub struct NativeDocumentHandle(pub(super) Rc<Shared>);

fn scope(
    document: &InteractiveDocument,
    generation: u64,
    standalone: Option<&PluginScope>,
) -> Result<PluginScope, HostAccessError> {
    if let Some(scope) = document.callback_scope() {
        if scope.source_generation != generation {
            return Err(HostAccessError::Stale);
        }
        return Ok(scope.clone());
    }
    let instance = if document.module_program.is_some() {
        document
            .module_projection()
            .map_err(HostAccessError::Invalid)?
            .instance("root")
            .ok_or_else(|| HostAccessError::Invalid("native module root missing".into()))?
            .clone()
    } else if document.state_program.is_some() {
        document
            .component_state()
            .map_err(HostAccessError::Invalid)?
            .instance
    } else {
        return standalone
            .cloned()
            .ok_or_else(|| HostAccessError::Invalid("native standalone owner missing".into()));
    };
    Ok(PluginScope {
        instance,
        source_generation: generation,
    })
}

impl NativeDocumentHandle {
    pub fn new(
        document: InteractiveDocument,
        source_generation: u64,
    ) -> Result<Self, HostAccessError> {
        Self::new_internal(document, source_generation, false)
    }
    pub(super) fn new_hook_candidate(
        document: InteractiveDocument,
        source_generation: u64,
    ) -> Result<Self, HostAccessError> {
        Self::new_internal(document, source_generation, true)
    }
    fn new_internal(
        document: InteractiveDocument,
        source_generation: u64,
        hook_candidate: bool,
    ) -> Result<Self, HostAccessError> {
        if document.module_program.as_ref().is_some_and(|program| {
            program
                .project()
                .is_ok_and(|projection| projection.uses_hook_document())
        }) && !hook_candidate
        {
            return Err(HostAccessError::Invalid(
                "prepared hook modules require their owning host constructor".into(),
            ));
        }
        if source_generation == 0 {
            return Err(HostAccessError::Invalid(
                "positive source generation required".into(),
            ));
        }
        let id = NEXT_HOST
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| HostAccessError::CounterExhausted)?;
        let standalone_scope = if document.module_program.is_none()
            && document.state_program.is_none()
            && document.callback_scope().is_none()
        {
            use meari::{
                application::component_state::ComponentState,
                domain::component_state::{InstanceKey, StateSchema},
            };
            let mut owner = ComponentState::new("native-host-document".into(), id)
                .map_err(HostAccessError::Invalid)?;
            Some(PluginScope {
                instance: owner
                    .mount(
                        None,
                        InstanceKey::Position(0),
                        "standalone".into(),
                        StateSchema {
                            revision: "1".into(),
                            slots: Default::default(),
                        },
                    )
                    .map_err(HostAccessError::Invalid)?,
                source_generation,
            })
        } else {
            None
        };
        scope(&document, source_generation, standalone_scope.as_ref())?;
        Ok(Self(Rc::new(Shared {
            observation: Default::default(),
            hooks: RefCell::new(None),
            document: RefCell::new(Some(document)),
            lifecycle: Cell::new(HostLifecycle::Open),
            input_blocked: Cell::new(false),
            input_dispatched: Cell::new(false),
            id,
            generation: source_generation,
            standalone_scope,
            revision: Cell::new(1),
            turn: Cell::new(0),
            close: RefCell::new(None),
        })))
    }
    pub fn id(&self) -> u64 {
        self.0.id
    }
    pub fn lifecycle(&self) -> HostLifecycle {
        self.0.lifecycle.get()
    }
    pub(super) fn require_open(&self) -> Result<(), HostAccessError> {
        match self.lifecycle() {
            HostLifecycle::Open => Ok(()),
            HostLifecycle::Closed | HostLifecycle::ForcedIncomplete => Err(HostAccessError::Closed),
            _ => Err(HostAccessError::Closing),
        }
    }
    pub fn try_with<R>(
        &self,
        visit: impl for<'doc> FnOnce(&'doc InteractiveDocument) -> R,
    ) -> Result<R, HostAccessError> {
        let document = self
            .0
            .document
            .try_borrow()
            .map_err(|_| HostAccessError::Busy)?;
        Ok(visit(document.as_ref().ok_or(HostAccessError::Closed)?))
    }
    /// Synchronous apply/commit profile: the consumer can commit its staged bindings
    /// immediately after the document update succeeds, before returning this receipt.
    pub fn try_reproject<R, E>(
        &self,
        expected: &HostStamp,
        update: impl for<'doc> FnOnce(&'doc mut InteractiveDocument) -> Result<R, E>,
    ) -> Result<(R, HostCommitReceipt), HostUpdateError<E>> {
        self.try_reproject_kind(
            expected,
            super::retained_observation::MutationKind::Projection,
            update,
        )
    }
    pub(super) fn try_reproject_kind<R, E>(
        &self,
        expected: &HostStamp,
        kind: super::retained_observation::MutationKind,
        update: impl for<'doc> FnOnce(&'doc mut InteractiveDocument) -> Result<R, E>,
    ) -> Result<(R, HostCommitReceipt), HostUpdateError<E>> {
        self.require_open().map_err(HostUpdateError::Access)?;
        if self.0.input_blocked.get() {
            return Err(HostUpdateError::Access(HostAccessError::Busy));
        }
        let before = self.stamp().map_err(HostUpdateError::Access)?;
        if &before != expected {
            return Err(HostUpdateError::Access(HostAccessError::Stale));
        }
        let next = self
            .0
            .revision
            .get()
            .checked_add(1)
            .ok_or(HostUpdateError::Access(HostAccessError::CounterExhausted))?;
        let mut borrowed = self
            .0
            .document
            .try_borrow_mut()
            .map_err(|_| HostUpdateError::Access(HostAccessError::Busy))?;
        let document = borrowed
            .as_mut()
            .ok_or(HostUpdateError::Access(HostAccessError::Closed))?;
        let mutation = self
            .0
            .observation
            .begin(kind)
            .map_err(HostUpdateError::Access)?;
        let result = update(document).map_err(HostUpdateError::Apply)?;
        self.0.revision.set(next);
        let after = self.stamp_for(document).map_err(HostUpdateError::Access)?;
        drop(borrowed);
        mutation.finish();
        let after = if self
            .flush_hook_actions()
            .map_err(HostUpdateError::Access)?
            .is_some()
        {
            self.stamp().map_err(HostUpdateError::Access)?
        } else {
            after
        };
        let receipt = HostCommitReceipt { before, after };
        if matches!(kind, super::retained_observation::MutationKind::Projection) {
            self.note_projection_commit(&receipt);
        }
        Ok((result, receipt))
    }
    pub fn stamp(&self) -> Result<HostStamp, HostAccessError> {
        self.try_with(|document| self.stamp_for(document))?
    }
    pub fn dispatch_ui_event(
        &self,
        event: blitz_traits::events::UiEvent,
    ) -> Result<(), HostAccessError> {
        use blitz_dom::Document;
        self.require_open()?;
        if self.0.input_blocked.get() {
            return Err(HostAccessError::Busy);
        }
        let revision = self
            .0
            .revision
            .get()
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        self.with_internal_mut(|document| {
            let mutation = self
                .0
                .observation
                .begin(super::retained_observation::MutationKind::Input)?;
            document.handle_ui_event(event);
            mutation.finish();
            Ok::<_, HostAccessError>(())
        })??;
        self.flush_input_hooks()?;
        self.0.revision.set(revision);
        self.0.input_dispatched.set(true);
        Ok(())
    }
    pub fn dispatch_action(
        &self,
        action: crate::domain::widgets::Action,
    ) -> Result<bool, HostAccessError> {
        self.require_open()?;
        if self.0.input_blocked.get() {
            return Err(HostAccessError::Busy);
        }
        let revision = self
            .0
            .revision
            .get()
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        let changed = self
            .with_internal_mut(|document| {
                let mutation = self
                    .0
                    .observation
                    .begin(super::retained_observation::MutationKind::Input)
                    .map_err(|error| error.to_string())?;
                let changed = document.dispatch(action)?;
                mutation.finish();
                Ok::<_, String>(changed)
            })?
            .map_err(HostAccessError::Invalid)?;
        let changed = self.flush_input_hooks()?.unwrap_or(changed);
        self.0.revision.set(revision);
        self.0.input_dispatched.set(true);
        Ok(changed)
    }
    pub(crate) fn take_dispatched_input(&self) -> bool {
        self.0.input_dispatched.replace(false)
    }
    fn flush_input_hooks(&self) -> Result<Option<bool>, HostAccessError> {
        self.flush_hook_actions()
            .inspect_err(|_| self.0.observation.invalidate())
    }
    #[cfg(feature = "window")]
    pub(in crate::infrastructure) fn with_native_input<R>(
        &self,
        visit: impl FnOnce(&mut InteractiveDocument) -> Result<R, String>,
    ) -> Result<R, HostAccessError> {
        self.require_open()?;
        if self.0.input_blocked.get() {
            return Err(HostAccessError::Busy);
        }
        self.with_internal_mut(|document| {
            let mutation = self
                .0
                .observation
                .begin(super::retained_observation::MutationKind::Input)?;
            let result = visit(document).map_err(HostAccessError::Invalid)?;
            mutation.finish();
            Ok(result)
        })?
    }
    #[cfg(feature = "window")]
    pub(crate) fn can_enter_engine(&self) -> bool {
        self.0
            .document
            .try_borrow_mut()
            .is_ok_and(|document| document.is_some())
    }
    #[cfg(feature = "window")]
    pub(crate) fn can_dispatch_input(&self) -> bool {
        self.lifecycle() == HostLifecycle::Open
            && !self.0.input_blocked.get()
            && self.0.document.try_borrow_mut().is_ok()
    }
    #[cfg(feature = "window")]
    pub(crate) fn note_native_action(&self) -> Result<(), HostAccessError> {
        self.flush_input_hooks()?;
        let revision = self
            .0
            .revision
            .get()
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        self.0.revision.set(revision);
        self.0.input_dispatched.set(true);
        Ok(())
    }
    fn stamp_for(&self, document: &InteractiveDocument) -> Result<HostStamp, HostAccessError> {
        Ok(HostStamp {
            host_id: self.id(),
            owner: scope(
                document,
                self.0.generation,
                self.0.standalone_scope.as_ref(),
            )?,
            definition_revision: document
                .state_program
                .as_ref()
                .map_or(1, |program| program.definition_revision()),
            document_revision: self.0.revision.get(),
            turn_sequence: self.0.turn.get(),
        })
    }
    pub(super) fn next_turn(&self) -> Result<HostStamp, HostAccessError> {
        self.require_open()?;
        let next = self
            .0
            .turn
            .get()
            .checked_add(1)
            .ok_or(HostAccessError::CounterExhausted)?;
        self.0.turn.set(next);
        self.stamp()
    }
    pub(super) fn with_internal_mut<R>(
        &self,
        visit: impl FnOnce(&mut InteractiveDocument) -> R,
    ) -> Result<R, HostAccessError> {
        let mut document = self
            .0
            .document
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?;
        Ok(visit(document.as_mut().ok_or(HostAccessError::Closed)?))
    }
    pub(super) fn begin_close(
        &self,
        pending: usize,
    ) -> Result<CallbackCloseReceipt, HostAccessError> {
        if let Some(receipt) = self
            .0
            .close
            .try_borrow()
            .map_err(|_| HostAccessError::Busy)?
            .clone()
        {
            return Ok(receipt);
        }
        self.require_open()?;
        let mut hooks = self
            .0
            .hooks
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?;
        let stamp = self.stamp()?;
        let discarded = self.with_internal_mut(|document| {
            let count = document.drain_callbacks().len();
            document.close_callbacks();
            document.hook_actions.clear();
            count
        })?;
        let receipt = CallbackCloseReceipt {
            stamp,
            discarded_callbacks: pending.saturating_add(discarded),
        };
        *self
            .0
            .close
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)? = Some(receipt.clone());
        self.0.lifecycle.set(HostLifecycle::Closing);
        self.0.input_blocked.set(true);
        if let Some(hooks) = hooks.as_mut() {
            hooks.failure = None;
            hooks.owner.close();
        }
        Ok(receipt)
    }
    pub(super) fn finish_close(&self, forced: bool) -> Result<(), HostAccessError> {
        if self.lifecycle() == HostLifecycle::Closed {
            return Ok(());
        }
        if !forced && self.lifecycle() != HostLifecycle::HostCleaned {
            return Err(HostAccessError::Closing);
        }
        let mut hooks = self
            .0
            .hooks
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?;
        let document = self
            .0
            .document
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?
            .take();
        drop(document);
        if let Some(hooks) = hooks.as_mut() {
            hooks.failure = None;
            hooks.owner.close();
        }
        self.0.lifecycle.set(if forced {
            HostLifecycle::ForcedIncomplete
        } else {
            HostLifecycle::Closed
        });
        Ok(())
    }
}
