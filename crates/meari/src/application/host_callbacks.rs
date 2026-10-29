//! Host-owned callback bindings feed the existing plugin registry.
use super::plugins::{DispatchReceipt, PluginRegistry, RegistrationHandle};
use crate::domain::{
    host_callbacks::{CallbackArguments, CallbackSignature},
    plugins::{PluginError, PluginErrorCode, PluginPayload, PluginScope},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
mod bindings;
pub use bindings::CallbackBindings;
mod document;
pub use document::{DocumentCallbacks, PendingCallback};
mod local;
pub use local::{LocalActionInvocation, LocalActionKey, LocalDispatchError};

#[derive(Debug)]
struct CallbackOwner {
    active: AtomicBool,
}
#[derive(Debug)]
struct CallbackLease {
    active: AtomicBool,
}

/// Not deserializable: source strings alone cannot manufacture a host binding.
#[derive(Clone, Debug)]
pub struct CallbackHandle {
    owner: Arc<CallbackOwner>,
    lease: Option<Arc<CallbackLease>>,
    revision: u64,
    prop: String,
}
/// Original event-owner comparison only. This token cannot dispatch or manufacture a binding.
#[derive(Clone, Debug)]
pub struct CallbackEventOwner {
    owner: Arc<CallbackOwner>,
    lease: Option<Arc<CallbackLease>>,
    prop: String,
}
impl PartialEq for CallbackEventOwner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.prop == other.prop
            && match (&self.lease, &other.lease) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}
impl Eq for CallbackEventOwner {}
impl CallbackEventOwner {
    pub fn is_active(&self) -> bool {
        self.owner.active.load(Ordering::Acquire)
            && self
                .lease
                .as_ref()
                .is_none_or(|lease| lease.active.load(Ordering::Acquire))
    }
}
type PayloadProducer = Box<dyn Fn(&CallbackArguments) -> Result<PluginPayload, PluginError>>;
struct Binding {
    signature: CallbackSignature,
    target: BindingTarget,
}
enum BindingTarget {
    Plugin {
        plugin: RegistrationHandle,
        capability: String,
        produce: PayloadProducer,
    },
    Local(LocalActionKey),
}
pub struct HostCallbacks {
    owner: Arc<CallbackOwner>,
    scope: PluginScope,
    revision: u64,
    bindings: BTreeMap<String, Binding>,
    closed: bool,
}
impl Drop for HostCallbacks {
    fn drop(&mut self) {
        self.owner.active.store(false, Ordering::Release);
    }
}
impl HostCallbacks {
    pub fn new(scope: PluginScope) -> Self {
        Self {
            owner: Arc::new(CallbackOwner {
                active: AtomicBool::new(true),
            }),
            scope,
            revision: 0,
            bindings: BTreeMap::new(),
            closed: false,
        }
    }
    fn admit_prop(&self, prop: &str) -> Result<(), PluginError> {
        if self.closed
            || !self.owner.active.load(Ordering::Acquire)
            || self.bindings.len() >= 1024
            || prop.is_empty()
            || prop.len() > 256
            || !prop
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || self.bindings.contains_key(prop)
        {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "invalid, duplicate or closed callback binding",
            ));
        }
        Ok(())
    }
    pub fn bind(
        &mut self,
        prop: String,
        signature: CallbackSignature,
        plugin: RegistrationHandle,
        capability: String,
        produce: impl Fn(&CallbackArguments) -> Result<PluginPayload, PluginError> + 'static,
    ) -> Result<CallbackHandle, PluginError> {
        self.admit_prop(&prop)?;
        self.bindings.insert(
            prop.clone(),
            Binding {
                signature,
                target: BindingTarget::Plugin {
                    plugin,
                    capability,
                    produce: Box::new(produce),
                },
            },
        );
        Ok(CallbackHandle {
            owner: self.owner.clone(),
            lease: None,
            revision: self.revision,
            prop,
        })
    }
    fn validate_handle(
        &self,
        handle: &CallbackHandle,
        arguments: &CallbackArguments,
        scope: &PluginScope,
    ) -> Result<&Binding, PluginError> {
        if self.closed
            || !self.owner.active.load(Ordering::Acquire)
            || handle
                .lease
                .as_ref()
                .is_some_and(|lease| !lease.active.load(Ordering::Acquire))
            || !Arc::ptr_eq(&self.owner, &handle.owner)
            || self.revision != handle.revision
            || scope != &self.scope
        {
            return Err(PluginError::new(
                PluginErrorCode::Stale,
                "callback scope or binding is stale",
            ));
        }
        let binding = self
            .bindings
            .get(&handle.prop)
            .ok_or_else(|| PluginError::new(PluginErrorCode::Stale, "callback binding missing"))?;
        arguments
            .validate()
            .map_err(|message| PluginError::new(PluginErrorCode::InvalidPayload, message))?;
        if arguments.signature() != binding.signature {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "callback argument signature differs",
            ));
        }
        Ok(binding)
    }
    /// Renderer node/event guards and registry grants/schema validation remain mandatory.
    pub fn dispatch(
        &self,
        handle: &CallbackHandle,
        arguments: CallbackArguments,
        registry: &mut PluginRegistry,
    ) -> Result<DispatchReceipt, PluginError> {
        let binding = self.validate_handle(handle, &arguments, registry.scope())?;
        let BindingTarget::Plugin {
            plugin,
            capability,
            produce,
        } = &binding.target
        else {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "local action cannot use plugin dispatch",
            ));
        };
        registry.dispatch(plugin, capability, produce(&arguments)?)
    }
    /// Invalidate every old handle before source replacement or unmount.
    /// Existing plugin tickets still require registry cancellation/drain separately.
    pub fn invalidate(&mut self) -> Result<(), PluginError> {
        self.closed = true;
        self.owner.active.store(false, Ordering::Release);
        self.bindings.clear();
        self.revision = self.revision.checked_add(1).ok_or_else(|| {
            PluginError::new(PluginErrorCode::Stale, "callback revision exhausted")
        })?;
        Ok(())
    }
}
