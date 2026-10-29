//! Bounded passive metadata from the original window events, never a product idle policy.
use super::{Application, access};
use crate::application::window_actions::*;
use std::time::Duration;
use winit::{event::WindowEvent, window::WindowId};

pub(super) struct InputActivity {
    host: u64,
    sequence: u64,
    latest: Option<WindowInputActivity>,
    terminal: Option<ActionError>,
    initial: bool,
    pending: Option<PendingActivity>,
}
struct PendingActivity {
    window: WindowId,
    sequence: u64,
    kind: WindowInputKind,
    observed_at: Duration,
    focused: Option<bool>,
    pointer_inside: Option<bool>,
}
impl InputActivity {
    pub(super) fn new(host: u64) -> Self {
        Self {
            host,
            sequence: 0,
            latest: None,
            terminal: None,
            initial: true,
            pending: None,
        }
    }
    pub(super) fn sequence(&self) -> u64 {
        self.sequence
    }
    pub(super) fn record_pending(
        &mut self,
        window: WindowId,
        kind: WindowInputKind,
        now: Duration,
    ) -> Result<(), ActionError> {
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if !self.initial {
            return Err(ActionError::NotReady);
        }
        if let Some(last) = &self.pending {
            if last.window != window {
                return Err(ActionError::StaleWindow);
            }
            if now < last.observed_at {
                return Err(ActionError::StaleObservation);
            }
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ActionError::CounterExhausted)?;
        let focused = match kind {
            WindowInputKind::FocusGained => Some(true),
            WindowInputKind::FocusLost => Some(false),
            _ => self.pending.as_ref().and_then(|last| last.focused),
        };
        let pointer_inside = match kind {
            WindowInputKind::PointerEntered => Some(true),
            WindowInputKind::PointerLeft => Some(false),
            _ => self.pending.as_ref().and_then(|last| last.pointer_inside),
        };
        self.pending = Some(PendingActivity {
            window,
            sequence,
            kind,
            observed_at: now,
            focused,
            pointer_inside,
        });
        self.sequence = sequence;
        Ok(())
    }
    /// Rebind only already-observed initial facts to the same original live window's first Ready.
    pub(super) fn publish_initial(
        &mut self,
        scope: ActionScope,
        window: WindowId,
    ) -> Result<bool, ActionError> {
        if scope.host != self.host {
            return Err(ActionError::ForeignHost);
        }
        if let Some(error) = self.terminal {
            return Err(error);
        }
        self.initial = false;
        let Some(pending) = self.pending.take() else {
            return Ok(false);
        };
        if pending.window != window {
            return Err(ActionError::StaleWindow);
        }
        self.latest = Some(WindowInputActivity {
            scope,
            sequence: pending.sequence,
            kind: pending.kind,
            observed_at: pending.observed_at,
            focused: pending.focused,
            pointer_inside: pending.pointer_inside,
        });
        Ok(true)
    }
    pub(super) fn retire(&mut self) {
        self.initial = false;
        self.pending = None;
        self.latest = None;
    }
    pub(super) fn latest(
        &self,
        scope: ActionScope,
    ) -> Result<Option<WindowInputActivity>, ActionError> {
        if scope.host != self.host {
            return Err(ActionError::ForeignHost);
        }
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if self.latest.is_some_and(|last| last.scope != scope) {
            return Err(ActionError::StaleWindow);
        }
        Ok(self.latest)
    }
    pub(super) fn record(
        &mut self,
        scope: ActionScope,
        kind: WindowInputKind,
        now: Duration,
    ) -> Result<(), ActionError> {
        if scope.host != self.host {
            return Err(ActionError::ForeignHost);
        }
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if self.latest.is_some_and(|last| now < last.observed_at) {
            return Err(ActionError::StaleObservation);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ActionError::CounterExhausted)?;
        let previous = self.latest.filter(|last| last.scope == scope);
        let focused = match kind {
            WindowInputKind::FocusGained => Some(true),
            WindowInputKind::FocusLost => Some(false),
            _ => previous.and_then(|last| last.focused),
        };
        let pointer_inside = match kind {
            WindowInputKind::PointerEntered => Some(true),
            WindowInputKind::PointerLeft => Some(false),
            _ => previous.and_then(|last| last.pointer_inside),
        };
        self.latest = Some(WindowInputActivity {
            scope,
            sequence,
            kind,
            observed_at: now,
            focused,
            pointer_inside,
        });
        self.sequence = sequence;
        self.initial = false;
        self.pending = None;
        Ok(())
    }
    pub(super) fn invalidate(&mut self, error: ActionError) {
        self.latest = None;
        if error != ActionError::NotReady || !self.initial {
            self.retire();
        }
        if matches!(error, ActionError::Closing | ActionError::Closed) {
            self.terminal = Some(error);
        }
    }
}
fn kind(event: &WindowEvent) -> Option<WindowInputKind> {
    Some(match event {
        WindowEvent::PointerMoved { .. } => WindowInputKind::PointerMotion,
        WindowEvent::PointerButton { .. } => WindowInputKind::PointerButton,
        WindowEvent::MouseWheel { .. } => WindowInputKind::Wheel,
        WindowEvent::KeyboardInput { .. } => WindowInputKind::Key,
        WindowEvent::Focused(true) => WindowInputKind::FocusGained,
        WindowEvent::Focused(false) => WindowInputKind::FocusLost,
        WindowEvent::PointerEntered { .. } => WindowInputKind::PointerEntered,
        WindowEvent::PointerLeft { .. } => WindowInputKind::PointerLeft,
        _ => return None,
    })
}
impl Application {
    pub(super) fn record_window_input(&mut self, id: WindowId, event: &WindowEvent) {
        let Some(kind) = kind(event) else {
            return;
        };
        let Some(view) = self.inner.windows.get(&id) else {
            return;
        };
        let Some(handle) = access::handle(view.doc.as_ref()) else {
            return;
        };
        let Some(binding) = self.actions.get(&handle.id()) else {
            return;
        };
        if let Err(error) = binding.record_input(view.window.as_ref(), kind)
            && !matches!(
                error,
                ActionError::NotReady
                    | ActionError::Closing
                    | ActionError::Closed
                    | ActionError::Busy
            )
        {
            self.report(error.to_string());
        }
    }
    pub(super) fn restore_window_cursors(&self) {
        for view in self.inner.windows.values() {
            if let Some(handle) = access::handle(view.doc.as_ref())
                && let Some(binding) = self.actions.get(&handle.id())
            {
                binding.restore_cursor(view.window.as_ref());
                binding.retire_input();
            }
        }
    }
}
#[cfg(test)]
mod tests;
