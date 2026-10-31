//! Bounded pure request ledger. Native authority and OS calls belong to the adapter.
pub use crate::domain::window_actions::*;
use std::{collections::VecDeque, num::NonZeroU64, time::Duration};

/// Revalidate the original issued scope immediately before entering an external adapter.
pub fn issue_if_current(
    command: ActionRecord,
    current: impl FnOnce() -> Result<ActionScope, ActionError>,
    issue: impl FnOnce(WindowAction) -> Result<(), ActionError>,
) -> Result<(), ActionError> {
    if command.status != ActionStatus::Issued {
        return Err(ActionError::StaleRequest);
    }
    let scope = current()?;
    if scope.host != command.ticket.scope.host {
        return Err(ActionError::ForeignHost);
    }
    if scope != command.ticket.scope {
        return Err(ActionError::StaleWindow);
    }
    issue(command.action)
}

pub struct WindowActions {
    host: u64,
    capacity: usize,
    last_id: u64,
    snapshot: Option<ActionSnapshot>,
    records: VecDeque<ActionRecord>,
    terminal: Option<ActionError>,
}
impl WindowActions {
    pub fn new(host: u64, capacity: usize) -> Result<Self, ActionError> {
        if host == 0 {
            return Err(ActionError::ForeignHost);
        }
        if capacity == 0 || capacity > 64 {
            return Err(ActionError::Capacity);
        }
        Ok(Self {
            host,
            capacity,
            last_id: 0,
            snapshot: None,
            records: VecDeque::new(),
            terminal: None,
        })
    }
    pub fn snapshot(&self) -> Option<ActionSnapshot> {
        self.snapshot
    }
    fn current(&self, expected: ActionScope) -> Result<ActionSnapshot, ActionError> {
        if expected.host != self.host {
            return Err(ActionError::ForeignHost);
        }
        if let Some(error) = self.terminal {
            return Err(error);
        }
        let current = self.snapshot.ok_or(ActionError::NotReady)?;
        if current.scope != expected {
            return Err(ActionError::StaleWindow);
        }
        Ok(current)
    }
    pub fn admit(
        &mut self,
        expected: ActionScope,
        id: NonZeroU64,
        action: WindowAction,
    ) -> Result<ActionAdmission, ActionError> {
        let snapshot = self.current(expected)?;
        if let Some(record) = self.records.iter().find(|record| record.ticket.id == id) {
            if record.ticket.scope != expected {
                return Err(ActionError::StaleRequest);
            }
            if record.action != action {
                return Err(ActionError::ConflictingDuplicate);
            }
            return Ok(ActionAdmission {
                record: *record,
                duplicate: true,
            });
        }
        if id.get() <= self.last_id {
            return Err(ActionError::StaleRequest);
        }
        if !snapshot.capabilities.supports(action) {
            return Err(ActionError::Unsupported);
        }
        if self.records.len() == self.capacity {
            return Err(ActionError::Capacity);
        }
        let record = ActionRecord {
            ticket: ActionTicket {
                scope: expected,
                id,
            },
            action,
            status: ActionStatus::Queued,
            observation: None,
            observed_at: None,
        };
        self.records.push_back(record);
        self.last_id = id.get();
        Ok(ActionAdmission {
            record,
            duplicate: false,
        })
    }
    /// Only one command is outstanding at the OS boundary. Others remain queued.
    pub fn next(&mut self, expected: ActionScope) -> Result<Option<ActionRecord>, ActionError> {
        self.current(expected)?;
        if self
            .records
            .iter()
            .any(|record| record.status == ActionStatus::Issued)
        {
            return Ok(None);
        }
        let Some(record) = self
            .records
            .iter_mut()
            .find(|record| record.status == ActionStatus::Queued)
        else {
            return Ok(None);
        };
        record.status = ActionStatus::Issued;
        Ok(Some(*record))
    }
    pub fn observe(
        &mut self,
        scope: ActionScope,
        capabilities: ActionCapabilities,
        observed: WindowObservation,
        observed_at: Duration,
    ) -> Result<bool, ActionError> {
        if scope.host != self.host {
            return Err(ActionError::ForeignHost);
        }
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if self
            .snapshot
            .is_some_and(|old| observed_at < old.observed_at)
        {
            return Err(ActionError::StaleObservation);
        }
        let changed = self.snapshot.is_none_or(|old| {
            old.scope != scope || old.capabilities != capabilities || old.observed != observed
        });
        let revision = if changed {
            self.snapshot.map_or(Ok(1), |old| {
                old.revision
                    .checked_add(1)
                    .ok_or(ActionError::CounterExhausted)
            })?
        } else {
            self.snapshot.expect("existing unchanged snapshot").revision
        };
        self.snapshot = Some(ActionSnapshot {
            scope,
            revision,
            capabilities,
            observed,
            observed_at,
        });
        let mut transitioned = false;
        for record in &mut self.records {
            if record.status.terminal() {
                continue;
            }
            let status = if record.ticket.scope != scope {
                ActionStatus::Failed(ActionError::StaleWindow)
            } else if !capabilities.supports(record.action) {
                ActionStatus::Failed(ActionError::Unsupported)
            } else if record.status == ActionStatus::Issued && observed.matches(record.action) {
                ActionStatus::Observed
            } else {
                record.status
            };
            transitioned |= status != record.status;
            if status == ActionStatus::Observed {
                record.observation = Some(observed);
                record.observed_at = Some(observed_at);
            }
            record.status = status;
        }
        Ok(changed || transitioned)
    }
    /// Only cursor setters and resize starts have call-completion semantics instead of an OS observation.
    pub fn applied(&mut self, ticket: ActionTicket) -> Result<(), ActionError> {
        self.current(ticket.scope)?;
        let record = self
            .records
            .iter_mut()
            .find(|record| record.ticket == ticket)
            .ok_or(ActionError::StaleRequest)?;
        if record.status != ActionStatus::Issued
            || !matches!(
                record.action,
                WindowAction::SetCursorVisible(_) | WindowAction::BeginResize(_)
            )
        {
            return Err(ActionError::StaleRequest);
        }
        record.status = ActionStatus::Applied;
        Ok(())
    }
    pub fn fail(&mut self, ticket: ActionTicket, error: ActionError) -> Result<(), ActionError> {
        let record = self
            .records
            .iter_mut()
            .find(|record| record.ticket == ticket)
            .ok_or(ActionError::StaleRequest)?;
        if record.status.terminal() {
            return Err(ActionError::StaleRequest);
        }
        record.status = ActionStatus::Failed(error);
        Ok(())
    }
    pub fn invalidate(&mut self, error: ActionError) -> bool {
        if matches!(error, ActionError::Closing | ActionError::Closed) {
            self.terminal = Some(error);
        }
        self.snapshot = None;
        let mut changed = false;
        for record in &mut self.records {
            if !record.status.terminal() {
                record.status = ActionStatus::Failed(error);
                changed = true;
            }
        }
        changed
    }
    pub fn record(&self, id: NonZeroU64) -> Option<ActionRecord> {
        self.records
            .iter()
            .find(|record| record.ticket.id == id)
            .copied()
    }
    /// Removes terminal history only; an unobserved issued request is not a success.
    pub fn take_result(&mut self, id: NonZeroU64) -> Option<ActionRecord> {
        let index = self
            .records
            .iter()
            .position(|record| record.ticket.id == id && record.status.terminal())?;
        self.records.remove(index)
    }
}

#[cfg(test)]
mod tests;
