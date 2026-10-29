use super::*;
use crate::domain::build::TargetKind;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct Registered {
    manifest: PluginManifest,
    handle: RegistrationHandle,
    adapter: Box<dyn PluginAdapter>,
    grants: BTreeSet<String>,
    closing: bool,
}
struct Pending {
    ticket: PluginTicket,
    cancelling: bool,
}

pub struct PluginRegistry {
    id: ContentHash,
    owner: Arc<()>,
    target: TargetKind,
    scope: PluginScope,
    next_epoch: u64,
    next_request: u64,
    limit: usize,
    plugins: BTreeMap<String, Registered>,
    pending: BTreeMap<u64, Pending>,
    terminal: VecDeque<PluginTicket>,
}

fn failure(code: PluginErrorCode, message: &str) -> PluginError {
    PluginError::new(code, message)
}

impl PluginRegistry {
    pub fn new(
        id: ContentHash,
        target: TargetKind,
        scope: PluginScope,
        max_pending: usize,
    ) -> Result<Self, PluginError> {
        if scope.source_generation == 0 || !(1..=4096).contains(&max_pending) {
            return Err(failure(
                PluginErrorCode::InvalidManifest,
                "plugin scope or pending budget is invalid",
            ));
        }
        Ok(Self {
            id,
            owner: Arc::new(()),
            target,
            scope,
            next_epoch: 1,
            next_request: 1,
            limit: max_pending,
            plugins: BTreeMap::new(),
            pending: BTreeMap::new(),
            terminal: VecDeque::new(),
        })
    }

    pub fn register(
        &mut self,
        adapter: Box<dyn PluginAdapter>,
        grants: BTreeSet<String>,
    ) -> Result<RegistrationHandle, PluginError> {
        let manifest = adapter.manifest().clone();
        manifest.validate()?;
        if self.plugins.contains_key(&manifest.id) {
            return Err(failure(
                PluginErrorCode::Duplicate,
                "plugin ID is already registered",
            ));
        }
        if self.plugins.len() >= 128
            || grants
                .iter()
                .any(|capability| !manifest.capabilities.contains_key(capability))
        {
            return Err(failure(
                PluginErrorCode::InvalidManifest,
                "plugin registration budget or grants are invalid",
            ));
        }
        let epoch = self.next_epoch;
        self.next_epoch = epoch
            .checked_add(1)
            .ok_or_else(|| failure(PluginErrorCode::Busy, "plugin registration epoch exhausted"))?;
        let handle = RegistrationHandle {
            owner: self.owner.clone(),
            registry: self.id.clone(),
            plugin: manifest.id.clone(),
            epoch,
        };
        self.plugins.insert(
            manifest.id.clone(),
            Registered {
                manifest,
                handle: handle.clone(),
                adapter,
                grants,
                closing: false,
            },
        );
        Ok(handle)
    }

    fn registered(&self, handle: &RegistrationHandle) -> Result<&Registered, PluginError> {
        if !Arc::ptr_eq(&self.owner, &handle.owner) || handle.registry != self.id {
            return Err(failure(
                PluginErrorCode::Stale,
                "foreign plugin registration",
            ));
        }
        self.plugins
            .get(&handle.plugin)
            .filter(|plugin| plugin.handle == *handle)
            .ok_or_else(|| failure(PluginErrorCode::Stale, "stale plugin registration"))
    }

    pub fn manifest(&self, handle: &RegistrationHandle) -> Result<&PluginManifest, PluginError> {
        Ok(&self.registered(handle)?.manifest)
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn scope(&self) -> &PluginScope {
        &self.scope
    }

    pub fn dispatch(
        &mut self,
        handle: &RegistrationHandle,
        capability: &str,
        payload: PluginPayload,
    ) -> Result<DispatchReceipt, PluginError> {
        let plugin = self.registered(handle)?;
        if plugin.closing {
            return Err(failure(PluginErrorCode::Closing, "plugin is closing"));
        }
        let descriptor = plugin
            .manifest
            .capabilities
            .get(capability)
            .ok_or_else(|| {
                failure(
                    PluginErrorCode::MissingCapability,
                    "plugin capability missing",
                )
            })?;
        if !descriptor.targets.contains(&self.target) {
            return Err(failure(
                PluginErrorCode::UnsupportedTarget,
                "plugin capability is unsupported on this target",
            ));
        }
        if !plugin.grants.contains(capability) {
            return Err(failure(
                PluginErrorCode::PermissionDenied,
                "plugin capability was not granted by the host",
            ));
        }
        if payload.schema != descriptor.request_schema {
            return Err(failure(
                PluginErrorCode::InvalidPayload,
                "plugin request schema differs",
            ));
        }
        payload.validate()?;
        plugin
            .adapter
            .validate_payload(capability, PayloadDirection::Request, &payload)?;
        if self.pending.len() >= self.limit {
            return Err(failure(
                PluginErrorCode::Busy,
                "plugin pending budget exceeded",
            ));
        }
        let request = self.next_request;
        self.next_request = request
            .checked_add(1)
            .ok_or_else(|| failure(PluginErrorCode::Busy, "plugin request IDs exhausted"))?;
        let ticket = PluginTicket {
            registration: handle.clone(),
            request,
            capability: capability.into(),
            scope: self.scope.clone(),
        };
        let invocation = PluginInvocation {
            ticket: ticket.clone(),
            payload,
        };
        let plugin = self
            .plugins
            .get_mut(&handle.plugin)
            .ok_or_else(|| failure(PluginErrorCode::Stale, "plugin disappeared"))?;
        let outcome = plugin.adapter.dispatch(&invocation)?;
        match &outcome {
            PluginDispatch::Immediate(result) => {
                Self::validate_result(plugin, capability, result)?;
                self.remember(ticket.clone());
            }
            PluginDispatch::Pending => {
                self.pending.insert(
                    request,
                    Pending {
                        ticket: ticket.clone(),
                        cancelling: false,
                    },
                );
            }
        }
        Ok(DispatchReceipt { ticket, outcome })
    }

    fn validate_payload(
        plugin: &Registered,
        capability: &str,
        direction: PayloadDirection,
        payload: &PluginPayload,
    ) -> Result<(), PluginError> {
        let descriptor = plugin
            .manifest
            .capabilities
            .get(capability)
            .ok_or_else(|| {
                failure(
                    PluginErrorCode::MissingCapability,
                    "plugin capability missing",
                )
            })?;
        let expected = match direction {
            PayloadDirection::Request => Some(&descriptor.request_schema),
            PayloadDirection::Reply => Some(&descriptor.reply_schema),
            PayloadDirection::Error => descriptor.error_schema.as_ref(),
            PayloadDirection::Event => descriptor.event_schema.as_ref(),
        };
        if expected != Some(&payload.schema) {
            return Err(failure(
                PluginErrorCode::InvalidPayload,
                "plugin output schema differs",
            ));
        }
        payload.validate()?;
        plugin
            .adapter
            .validate_payload(capability, direction, payload)
    }

    fn validate_result(
        plugin: &Registered,
        capability: &str,
        result: &Result<PluginPayload, PluginError>,
    ) -> Result<(), PluginError> {
        match result {
            Ok(payload) => {
                Self::validate_payload(plugin, capability, PayloadDirection::Reply, payload)
            }
            Err(error) => match &error.details {
                Some(payload) => {
                    Self::validate_payload(plugin, capability, PayloadDirection::Error, payload)
                }
                None => Ok(()),
            },
        }
    }

    fn current(&self, ticket: &PluginTicket) -> Result<&Pending, PluginError> {
        self.registered(&ticket.registration)?;
        if ticket.scope != self.scope {
            return Err(failure(
                PluginErrorCode::Stale,
                "plugin ticket belongs to another source scope",
            ));
        }
        self.pending
            .get(&ticket.request)
            .filter(|pending| pending.ticket == *ticket)
            .ok_or_else(|| {
                failure(
                    PluginErrorCode::Stale,
                    "plugin request is no longer pending",
                )
            })
    }

    pub fn accept_event(
        &mut self,
        ticket: &PluginTicket,
        event: PluginEventKind,
    ) -> Result<PluginEvent, PluginError> {
        let cancelling = self.current(ticket)?.cancelling;
        let plugin = self.registered(&ticket.registration)?;
        match &event {
            PluginEventKind::Progress(payload) => Self::validate_payload(
                plugin,
                &ticket.capability,
                PayloadDirection::Event,
                payload,
            )?,
            PluginEventKind::Completed(payload) => Self::validate_payload(
                plugin,
                &ticket.capability,
                PayloadDirection::Reply,
                payload,
            )?,
            PluginEventKind::Failed(error) => {
                Self::validate_result(plugin, &ticket.capability, &Err(error.clone()))?
            }
            PluginEventKind::Cancelled => {}
        }
        if !matches!(event, PluginEventKind::Progress(_)) {
            self.pending.remove(&ticket.request);
            self.remember(ticket.clone());
        }
        Ok(PluginEvent {
            ticket: ticket.clone(),
            event,
            cancellation_requested: cancelling,
        })
    }

    pub fn cancel(&mut self, ticket: &PluginTicket) -> Result<CancelReceipt, PluginError> {
        self.registered(&ticket.registration)?;
        if ticket.scope != self.scope {
            return Err(failure(
                PluginErrorCode::Stale,
                "plugin ticket belongs to another source scope",
            ));
        }
        if self.terminal.iter().any(|finished| finished == ticket) {
            return Ok(CancelReceipt::AlreadyTerminal);
        }
        if self.current(ticket)?.cancelling {
            return Ok(CancelReceipt::Requested);
        }
        let plugin = self
            .plugins
            .get_mut(&ticket.registration.plugin)
            .ok_or_else(|| failure(PluginErrorCode::Stale, "plugin disappeared"))?;
        let receipt = plugin.adapter.cancel(ticket)?;
        match receipt {
            CancelReceipt::Requested => {
                self.pending
                    .get_mut(&ticket.request)
                    .ok_or_else(|| failure(PluginErrorCode::Stale, "plugin request disappeared"))?
                    .cancelling = true;
            }
            CancelReceipt::Completed | CancelReceipt::AlreadyTerminal => {
                self.pending.remove(&ticket.request);
                self.remember(ticket.clone());
            }
        }
        Ok(receipt)
    }

    fn remember(&mut self, ticket: PluginTicket) {
        self.terminal.push_back(ticket);
        while self.terminal.len() > self.limit * 2 {
            self.terminal.pop_front();
        }
    }

    pub fn unregister(&mut self, handle: &RegistrationHandle) -> Result<(), PluginError> {
        self.registered(handle)?;
        let plugin = self
            .plugins
            .get_mut(&handle.plugin)
            .ok_or_else(|| failure(PluginErrorCode::Stale, "plugin disappeared"))?;
        plugin.closing = true;
        if self
            .pending
            .values()
            .any(|pending| pending.ticket.registration == *handle)
        {
            return Err(failure(
                PluginErrorCode::Busy,
                "plugin has pending requests; cancel or drain them before cleanup",
            ));
        }
        plugin.adapter.shutdown()?;
        self.plugins.remove(&handle.plugin);
        Ok(())
    }

    pub fn rebind_scope(&mut self, scope: PluginScope) -> Result<(), PluginError> {
        if scope.source_generation == 0 {
            return Err(failure(
                PluginErrorCode::InvalidManifest,
                "plugin source generation is invalid",
            ));
        }
        if !self.pending.is_empty() {
            return Err(failure(
                PluginErrorCode::Busy,
                "plugin requests must quiesce before source scope replacement",
            ));
        }
        self.scope = scope;
        self.terminal.clear();
        Ok(())
    }
}
