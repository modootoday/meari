//! Shared bounded document queue. Platform adapters admit owned, current events first.
use super::{CallbackBindings, CallbackEventOwner, CallbackHandle};
use crate::domain::{
    host_callbacks::{BoundCallback, CallbackArguments, CallbackInvocation, CallbackSource},
    plugins::PluginScope,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
mod retained_range;

#[derive(Debug)]
pub struct PendingCallback {
    pub handle: CallbackHandle,
    pub invocation: CallbackInvocation,
}
struct Entry {
    handler: BoundCallback,
    handle: CallbackHandle,
}

pub struct DocumentCallbacks {
    bindings: CallbackBindings,
    entries: BTreeMap<String, Entry>,
    pending: VecDeque<PendingCallback>,
    pending_bytes: usize,
}
impl DocumentCallbacks {
    pub fn new(
        bindings: CallbackBindings,
        expected: &PluginScope,
        handlers: BTreeMap<String, BoundCallback>,
    ) -> Result<Self, String> {
        if expected.source_generation == 0 || bindings.scope() != expected {
            return Err("callback export belongs to a different document scope".into());
        }
        let entries = handlers
            .into_iter()
            .map(|(path, handler)| {
                handler.validate()?;
                let handle = bindings
                    .resolve(&handler.reference)
                    .map_err(|error| error.message)?;
                Ok((path, Entry { handler, handle }))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            bindings,
            entries,
            pending: VecDeque::new(),
            pending_bytes: 0,
        })
    }
    pub fn scope(&self) -> &PluginScope {
        self.bindings.scope()
    }
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }
    pub fn source(&self, path: &str) -> Option<CallbackSource> {
        self.entries.get(path).map(|entry| entry.handler.source)
    }
    /// Event-slot intent key only; original binding/node guards remain mandatory.
    pub fn value_commit_key(node_path: &str) -> String {
        format!("{node_path}/@value-commit")
    }
    pub fn node_path<'a>(&self, key: &'a str) -> Option<&'a str> {
        self.entries.get(key)?;
        Some(key.strip_suffix("/@value-commit").unwrap_or(key))
    }
    pub fn event_owner(&self, key: &str) -> Option<CallbackEventOwner> {
        let handle = &self.entries.get(key)?.handle;
        Some(CallbackEventOwner {
            owner: handle.owner.clone(),
            lease: handle.lease.clone(),
            prop: handle.prop.clone(),
        })
    }
    pub fn text_paths(&self) -> BTreeSet<String> {
        self.paths()
            .filter(|path| {
                !path.ends_with("/@value-commit")
                    && self.source(path) == Some(CallbackSource::CurrentTargetText)
            })
            .cloned()
            .collect()
    }
    /// Trusted adapters must check current owned node/event kind and disabled/hidden state.
    /// This method does not invoke the plugin registry or a payload producer.
    pub fn enqueue_guarded(&mut self, path: &str, text: Option<&str>) -> Result<(), String> {
        if text.is_some_and(|text| text.len() > 1024 * 1024) {
            return Err("callback text exceeds byte budget".into());
        }
        self.admitted_entry(path)?;
        self.enqueue_arguments_guarded(
            path,
            match text {
                Some(text) => CallbackArguments::Text(text.to_owned()),
                None => CallbackArguments::Action,
            },
        )
    }
    /// Adapters supply a typed intent after their owned-node and event-kind guards.
    /// Boolean values are requests; this queue never mutates controlled widget state.
    pub fn enqueue_arguments_guarded(
        &mut self,
        path: &str,
        arguments: CallbackArguments,
    ) -> Result<(), String> {
        arguments.validate()?;
        let entry = self.admitted_entry(path)?;
        let bytes = entry.handler.intent_bytes(&arguments)?;
        let total = self
            .pending_bytes
            .checked_add(bytes)
            .ok_or("callback queue byte budget overflow")?;
        if total > 4 * 1024 * 1024 {
            return Err("callback intent queue exceeds byte budget".into());
        }
        let invocation = entry.handler.invoke_arguments(arguments)?;
        let handle = entry.handle.clone();
        self.pending
            .push_back(PendingCallback { handle, invocation });
        self.pending_bytes = total;
        Ok(())
    }
    fn admitted_entry(&self, path: &str) -> Result<&Entry, String> {
        if self.pending.len() >= 1024 {
            return Err("callback intent queue exceeds count budget".into());
        }
        let entry = self
            .entries
            .get(path)
            .ok_or("source callback path is not registered")?;
        self.bindings
            .resolve(&entry.handler.reference)
            .map_err(|error| error.message)?;
        Ok(entry)
    }
    pub fn drain(&mut self) -> Vec<PendingCallback> {
        self.pending_bytes = 0;
        self.pending.drain(..).collect()
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn close(&mut self) {
        self.bindings.invalidate();
        self.pending.clear();
        self.pending_bytes = 0;
    }
}
impl Drop for DocumentCallbacks {
    fn drop(&mut self) {
        self.close();
    }
}
