//! Commit lineage is separate from input revisions and host turns.
use super::{HostAccessError, HostCommitReceipt, HostStamp, NativeDocumentHandle, handle::Shared};
use crate::domain::graphics::FrameTicket;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub(super) struct ObservationState {
    epoch: Cell<u64>,
    mutating: Cell<bool>,
    committed: RefCell<Option<HostCommitReceipt>>,
}
impl Default for ObservationState {
    fn default() -> Self {
        Self {
            epoch: Cell::new(1),
            mutating: Cell::new(false),
            committed: RefCell::new(None),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum MutationKind {
    Projection,
    Input,
}
pub(super) struct Mutation<'a> {
    state: &'a ObservationState,
    finished: bool,
}
impl ObservationState {
    pub(super) fn invalidate(&self) {
        self.epoch.set(self.epoch.get().checked_add(1).unwrap_or(0));
        self.committed.borrow_mut().take();
    }
    pub(super) fn begin(&self, kind: MutationKind) -> Result<Mutation<'_>, HostAccessError> {
        if self.mutating.get() {
            return Err(HostAccessError::Busy);
        }
        if self.epoch.get() == 0 || self.epoch.get() == u64::MAX {
            return Err(HostAccessError::CounterExhausted);
        }
        self.mutating.set(true);
        if matches!(kind, MutationKind::Projection) {
            self.invalidate();
        }
        Ok(Mutation {
            state: self,
            finished: false,
        })
    }
}
impl Mutation<'_> {
    pub(super) fn finish(mut self) {
        self.finished = true;
    }
}
impl Drop for Mutation<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.state.invalidate();
        }
        self.state.mutating.set(false);
    }
}

/// Opaque, UI-thread proof issued from this handle's successful projection commit.
/// ```compile_fail
/// use meari_native::infrastructure::native_host::RetainedCommitProof;
/// let forged = RetainedCommitProof {};
/// ```
#[derive(Clone)]
pub struct RetainedCommitProof {
    owner: Weak<Shared>,
    epoch: u64,
    receipt: HostCommitReceipt,
}
impl std::fmt::Debug for RetainedCommitProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedCommitProof")
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}
impl RetainedCommitProof {
    pub fn receipt(&self) -> &HostCommitReceipt {
        &self.receipt
    }
}

/// Actual checked submission provenance; public frame DTOs cannot construct this.
#[derive(Clone)]
pub struct RetainedFrameProof {
    owner: Weak<Shared>,
    epoch: u64,
    active: Weak<Cell<bool>>,
    stamp: HostStamp,
    ticket: FrameTicket,
}
impl std::fmt::Debug for RetainedFrameProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedFrameProof")
            .field("stamp", &self.stamp)
            .field("ticket", &self.ticket)
            .finish_non_exhaustive()
    }
}
impl RetainedFrameProof {
    pub fn stamp(&self) -> &HostStamp {
        &self.stamp
    }
    pub fn ticket(&self) -> &FrameTicket {
        &self.ticket
    }
}

impl NativeDocumentHandle {
    pub(super) fn note_projection_commit(&self, receipt: &HostCommitReceipt) {
        *self.0.observation.committed.borrow_mut() = Some(receipt.clone());
    }
    /// Capture immediately after commit, before fallible observer registration.
    pub fn retain_commit(
        &self,
        receipt: &HostCommitReceipt,
    ) -> Result<RetainedCommitProof, HostAccessError> {
        let current = self.observation_stamp()?;
        if receipt.after() != &current
            || self.0.observation.committed.borrow().as_ref() != Some(receipt)
        {
            return Err(HostAccessError::Stale);
        }
        Ok(RetainedCommitProof {
            owner: Rc::downgrade(&self.0),
            epoch: self.0.observation.epoch.get(),
            receipt: receipt.clone(),
        })
    }
    fn observation_stamp(&self) -> Result<HostStamp, HostAccessError> {
        self.require_open()?;
        if self.0.observation.mutating.get() {
            return Err(HostAccessError::Busy);
        }
        if self.0.observation.epoch.get() == 0 {
            return Err(HostAccessError::CounterExhausted);
        }
        if self.try_with(|document| document.source_input_poisoned)? {
            return Err(HostAccessError::Stale);
        }
        self.stamp()
    }
    pub fn validate_retained_commit(
        &self,
        proof: &RetainedCommitProof,
    ) -> Result<HostStamp, HostAccessError> {
        let current = self.observation_stamp()?;
        let original = proof.receipt.after();
        if !Weak::ptr_eq(&proof.owner, &Rc::downgrade(&self.0))
            || proof.epoch != self.0.observation.epoch.get()
            || original.host_id != current.host_id
            || original.owner != current.owner
            || original.definition_revision != current.definition_revision
        {
            return Err(HostAccessError::Stale);
        }
        Ok(current)
    }
    pub fn validate_retained_frame(
        &self,
        proof: &RetainedCommitProof,
        frame: &RetainedFrameProof,
    ) -> Result<(), HostAccessError> {
        self.validate_retained_commit(proof)?;
        if !Weak::ptr_eq(&frame.owner, &proof.owner)
            || frame.epoch != proof.epoch
            || !frame.active.upgrade().is_some_and(|active| active.get())
            || frame.stamp.owner != proof.receipt.after().owner
            || frame.stamp.definition_revision != proof.receipt.after().definition_revision
            || frame.stamp.document_revision < proof.receipt.after().document_revision
        {
            return Err(HostAccessError::Stale);
        }
        Ok(())
    }
    #[cfg(any(feature = "window", test))]
    pub(in crate::infrastructure) fn checked_frame_proof(
        &self,
        stamp: &HostStamp,
        ticket: &FrameTicket,
        active: &Rc<Cell<bool>>,
    ) -> Result<RetainedFrameProof, HostAccessError> {
        if &self.observation_stamp()? != stamp || !active.get() {
            return Err(HostAccessError::Stale);
        }
        Ok(RetainedFrameProof {
            owner: Rc::downgrade(&self.0),
            epoch: self.0.observation.epoch.get(),
            active: Rc::downgrade(active),
            stamp: stamp.clone(),
            ticket: ticket.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
