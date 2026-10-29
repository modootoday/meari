//! Opt-in metadata delivery conformance over the production queue, without an OS window.
//!
//! Probes never contain a window stamp or frame and cannot attest route rendering.
//! This owner exposes no parent, registration, startup or window observation authority.
use super::{
    ForwardingSeed, NativeNoticeError, NativeWindowNoticeKind, NativeWindowObserver, ParentBinding,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataProbeKind {
    Created,
    GeometryChanged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataNoticeIdentity {
    pub host_id: u64,
    pub revision: u64,
    pub kind: NativeWindowNoticeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataDeliveryState {
    pub paused: bool,
    pub pending: Vec<MetadataNoticeIdentity>,
}

pub struct ObserverDeliveryHarness {
    binding: Rc<ParentBinding>,
}
impl ObserverDeliveryHarness {
    /// Inject the same observer factory used by the product, not a simulated delivery callback.
    pub fn new(host_id: u64, observer: NativeWindowObserver) -> Self {
        Self {
            binding: ParentBinding::new(host_id, ForwardingSeed::new(host_id), Some(observer)),
        }
    }

    /// Publishes metadata only. The production queue retains at most the latest per kind.
    pub fn publish_metadata(&self, kind: MetadataProbeKind) -> Result<(), NativeNoticeError> {
        let kind = match kind {
            MetadataProbeKind::Created => NativeWindowNoticeKind::Created,
            MetadataProbeKind::GeometryChanged => NativeWindowNoticeKind::GeometryChanged,
        };
        self.binding.publish(kind, None)
    }

    pub fn flush(&self) -> Result<(), NativeNoticeError> {
        self.binding.flush_notices()
    }

    /// Explicitly retries the same observer against its current latest-per-kind queue.
    pub fn retry_observer(&self) -> Result<(), NativeNoticeError> {
        self.binding.retry_observer()
    }

    /// Reads identities without draining or acknowledging any queued notice.
    pub fn state(&self) -> Result<MetadataDeliveryState, NativeNoticeError> {
        let state = self
            .binding
            .state
            .try_borrow()
            .map_err(|_| NativeNoticeError::Busy)?;
        Ok(MetadataDeliveryState {
            paused: self.binding.observer_failed.get(),
            pending: state
                .notices
                .iter()
                .map(|notice| MetadataNoticeIdentity {
                    host_id: notice.host_id,
                    revision: notice.revision,
                    kind: notice.kind,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
