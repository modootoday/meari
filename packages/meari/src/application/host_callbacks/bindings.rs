//! A document owns one revocable export; queued source handles share its lease.
use super::{CallbackHandle, CallbackLease, HostCallbacks};
use crate::domain::{
    host_callbacks::{CallbackReference, CallbackSignature},
    plugins::{PluginError, PluginErrorCode, PluginScope},
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct CallbackBindings {
    scope: PluginScope,
    lease: Arc<CallbackLease>,
    handles: BTreeMap<String, (CallbackSignature, CallbackHandle)>,
}
impl HostCallbacks {
    pub fn export_bindings(&self) -> Result<CallbackBindings, PluginError> {
        if self.closed || !self.owner.active.load(Ordering::Acquire) {
            return Err(PluginError::new(
                PluginErrorCode::Stale,
                "callback owner is closed",
            ));
        }
        let lease = Arc::new(CallbackLease {
            active: AtomicBool::new(true),
        });
        let handles = self
            .bindings
            .iter()
            .map(|(prop, binding)| {
                (
                    prop.clone(),
                    (
                        binding.signature,
                        CallbackHandle {
                            owner: self.owner.clone(),
                            lease: Some(lease.clone()),
                            revision: self.revision,
                            prop: prop.clone(),
                        },
                    ),
                )
            })
            .collect();
        Ok(CallbackBindings {
            scope: self.scope.clone(),
            lease,
            handles,
        })
    }
}
impl CallbackBindings {
    pub fn scope(&self) -> &PluginScope {
        &self.scope
    }
    pub fn resolve(&self, reference: &CallbackReference) -> Result<CallbackHandle, PluginError> {
        reference
            .validate()
            .map_err(|message| PluginError::new(PluginErrorCode::InvalidPayload, message))?;
        let (signature, handle) = self.handles.get(&reference.binding).ok_or_else(|| {
            PluginError::new(PluginErrorCode::Stale, "callback reference is not exported")
        })?;
        if !self.lease.active.load(Ordering::Acquire)
            || !handle.owner.active.load(Ordering::Acquire)
        {
            return Err(PluginError::new(
                PluginErrorCode::Stale,
                "document callback export is closed",
            ));
        }
        if *signature != reference.signature {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "exported callback signature differs",
            ));
        }
        Ok(handle.clone())
    }
    /// Revoke queued document invocations; existing registry tickets drain separately.
    pub fn invalidate(&self) {
        self.lease.active.store(false, Ordering::Release);
    }
}
impl Drop for CallbackBindings {
    fn drop(&mut self) {
        self.invalidate();
    }
}
