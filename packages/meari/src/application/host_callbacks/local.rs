//! Rust-owned local intent identity over the same revocable document callback queue.
use super::{Binding, BindingTarget, CallbackHandle, HostCallbacks};
use crate::domain::{
    host_callbacks::{CallbackArguments, CallbackSignature},
    plugins::{PluginError, PluginErrorCode, PluginScope},
};
use std::{fmt, sync::Arc};

/// No public constructor/serialization: a source prop is not a local action authority.
#[derive(Clone)]
pub struct LocalActionKey(Arc<()>);
impl PartialEq for LocalActionKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for LocalActionKey {}
impl fmt::Debug for LocalActionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LocalActionKey(..)")
    }
}

/// Consume immediately after document borrows are released; not a durable dispatch grant.
#[must_use]
#[derive(Debug)]
pub struct LocalActionInvocation {
    key: LocalActionKey,
    arguments: CallbackArguments,
    scope: PluginScope,
}
impl LocalActionInvocation {
    pub fn key(&self) -> &LocalActionKey {
        &self.key
    }
    pub fn matches_key(&self, key: &LocalActionKey) -> bool {
        self.key.eq(key)
    }
    pub fn arguments(&self) -> &CallbackArguments {
        &self.arguments
    }
    pub fn scope(&self) -> &PluginScope {
        &self.scope
    }
}

#[derive(Debug)]
pub enum LocalDispatchError {
    /// Common owner/scope/arguments validation succeeded, but the target is a plugin.
    /// Only this case permits the caller to try the existing plugin dispatch (which revalidates).
    NotLocal,
    /// Never retry through plugin dispatch on this error.
    Invalid(PluginError),
}
impl fmt::Display for LocalDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLocal => f.write_str("callback is not a local action"),
            Self::Invalid(error) => fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for LocalDispatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(error) => Some(error),
            Self::NotLocal => None,
        }
    }
}
impl From<PluginError> for LocalDispatchError {
    fn from(error: PluginError) -> Self {
        Self::Invalid(error)
    }
}

impl HostCallbacks {
    pub fn bind_local(
        &mut self,
        prop: String,
        signature: CallbackSignature,
    ) -> Result<LocalActionKey, PluginError> {
        self.admit_prop(&prop)?;
        if self.scope.source_generation == 0 {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "local action requires a positive source generation",
            ));
        }
        let key = LocalActionKey(Arc::new(()));
        self.bindings.insert(
            prop,
            Binding {
                signature,
                target: BindingTarget::Local(key.clone()),
            },
        );
        Ok(key)
    }
    /// Pass the current original document's scope, not source-supplied scope data.
    /// Native node/disabled/Closing/poison guards still apply before enqueue/processing.
    /// Keep the original PendingCallback on Busy and validate again when processing resumes.
    pub fn dispatch_local(
        &self,
        handle: &CallbackHandle,
        arguments: CallbackArguments,
        current_scope: &PluginScope,
    ) -> Result<LocalActionInvocation, LocalDispatchError> {
        let binding = self.validate_handle(handle, &arguments, current_scope)?;
        let BindingTarget::Local(key) = &binding.target else {
            return Err(LocalDispatchError::NotLocal);
        };
        if handle.lease.is_none() {
            return Err(PluginError::new(
                PluginErrorCode::Stale,
                "local action requires a document export lease",
            )
            .into());
        }
        Ok(LocalActionInvocation {
            key: key.clone(),
            arguments,
            scope: current_scope.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
