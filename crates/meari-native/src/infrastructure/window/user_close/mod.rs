//! One original-window user intent, validated again on the owning loop.
use crate::application::window_actions::{ActionError, ActionScope};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(super) struct Requests {
    pending: Mutex<Option<ActionScope>>,
}
impl Requests {
    pub fn admit(
        &self,
        host: u64,
        closed: &AtomicBool,
        expected: ActionScope,
    ) -> Result<bool, ActionError> {
        if expected.host != host {
            return Err(ActionError::ForeignHost);
        }
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| ActionError::BackendFailed)?;
        if closed.load(Ordering::Acquire) {
            return Err(ActionError::Closed);
        }
        if let Some(original) = *pending {
            return if original == expected {
                Ok(false)
            } else {
                Err(ActionError::Busy)
            };
        }
        *pending = Some(expected);
        Ok(true)
    }
    pub fn take(&self) -> Option<ActionScope> {
        self.pending.lock().ok()?.take()
    }
    pub fn clear(&self) {
        self.pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
    }
}
pub(super) fn validate(
    expected: Option<ActionScope>,
    current: Result<ActionScope, ActionError>,
) -> Result<(), ActionError> {
    let expected = expected.ok_or(ActionError::StaleRequest)?;
    let current = current?;
    if expected.host != current.host {
        return Err(ActionError::ForeignHost);
    }
    if expected != current {
        return Err(ActionError::StaleWindow);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// User veto/invalid scope preserves all coalesced work in the same original turn.
/// Accepted close follows its coordinator instead; never repost a veto busy loop.
pub(super) fn continuation(bits: u8, owner_open: bool) -> u8 {
    if owner_open && bits & super::wake::CLOSE == 0 {
        bits & !super::wake::USER_CLOSE
    } else {
        0
    }
}
