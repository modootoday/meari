//! Explicit host-owned plugin ports; concrete SDK/OS policy stays in the adapter.
use crate::domain::build::ContentHash;
pub use crate::domain::plugins::*;
use std::sync::Arc;
mod registry;
pub use registry::PluginRegistry;

#[derive(Clone, Debug, serde::Serialize)]
pub struct RegistrationHandle {
    #[serde(skip)]
    pub(super) owner: Arc<()>,
    pub(super) registry: ContentHash,
    pub(super) plugin: String,
    pub(super) epoch: u64,
}
impl PartialEq for RegistrationHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.registry == other.registry
            && self.plugin == other.plugin
            && self.epoch == other.epoch
    }
}
impl Eq for RegistrationHandle {}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PluginTicket {
    pub(super) registration: RegistrationHandle,
    pub(super) request: u64,
    pub(super) capability: String,
    pub(super) scope: PluginScope,
}
impl PluginTicket {
    pub fn request_id(&self) -> u64 {
        self.request
    }
    pub fn scope(&self) -> &PluginScope {
        &self.scope
    }
    pub fn capability(&self) -> &str {
        &self.capability
    }
}

#[derive(Clone, Debug)]
pub struct PluginInvocation {
    pub ticket: PluginTicket,
    pub payload: PluginPayload,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadDirection {
    Request,
    Reply,
    Error,
    Event,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginDispatch {
    Immediate(Result<PluginPayload, PluginError>),
    Pending,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchReceipt {
    pub ticket: PluginTicket,
    pub outcome: PluginDispatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReceipt {
    Requested,
    Completed,
    AlreadyTerminal,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginEventKind {
    Progress(PluginPayload),
    Completed(PluginPayload),
    Failed(PluginError),
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginEvent {
    pub ticket: PluginTicket,
    pub event: PluginEventKind,
    pub cancellation_requested: bool,
}

/// Called on the owning host thread. Pending adapters return events through `accept_event`.
pub trait PluginAdapter {
    fn manifest(&self) -> &PluginManifest;
    fn validate_payload(
        &self,
        capability: &str,
        direction: PayloadDirection,
        payload: &PluginPayload,
    ) -> Result<(), PluginError>;
    fn dispatch(&mut self, invocation: &PluginInvocation) -> Result<PluginDispatch, PluginError>;
    fn cancel(&mut self, ticket: &PluginTicket) -> Result<CancelReceipt, PluginError>;
    /// Return Busy until owned resources are released; Ok means cleanup is complete.
    fn shutdown(&mut self) -> Result<(), PluginError>;
}
