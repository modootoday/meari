//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::{
    hook_expression::Bindings,
    hook_values::{Transaction, Value},
};
use super::*;
use crate::domain::{
    Diagnostic, Span,
    component_state::InstanceHandle,
    state_program::{StateEvent, hook_literal as literal},
};
use std::{cell::Cell, rc::Rc};

impl HookDocument {
    /// Hold this guard across the independent renderer apply and all receipt validation.
    pub fn prepare_publication(
        &mut self,
        prepared: PreparedDocument,
    ) -> Result<Publication<'_>, Box<PublicationFailure>> {
        if self.closed || prepared.baseline != self.stamp() {
            return Err(Box::new(PublicationFailure {
                prepared,
                cause: None,
                reason: if self.closed {
                    "hook document is closed"
                } else {
                    "stale or foreign hook publication"
                },
            }));
        }
        let PreparedDocument {
            baseline,
            next,
            candidate,
            instances,
            epoch,
            changed,
        } = prepared;
        let heap = match self
            .owner
            .prepare_publish(candidate.expect("owned transaction"))
        {
            Ok(heap) => heap,
            Err(failure) => {
                return Err(Box::new(PublicationFailure {
                    cause: Some(failure.error),
                    reason: "hook heap rejected publication",
                    prepared: PreparedDocument {
                        baseline,
                        next,
                        candidate: Some(failure.candidate),
                        instances,
                        epoch,
                        changed,
                    },
                }));
            }
        };
        Ok(Publication {
            heap,
            instances_target: &mut self.instances,
            revision_target: &mut self.revision,
            epoch_target: &mut self.epoch,
            baseline,
            next,
            instances,
            epoch,
            changed,
        })
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.epoch.active.set(false);
        self.instances.clear();
        self.owner.close();
    }
}

impl Drop for HookDocument {
    fn drop(&mut self) {
        self.close();
    }
}

impl PreparedDocument {
    pub(crate) fn reject(&mut self) {
        self.candidate
            .as_mut()
            .expect("owned transaction")
            .reject_expression();
    }

    pub(crate) fn stamp(&self) -> DocumentStamp {
        self.next
    }

    pub(crate) fn advance_frame(&mut self) -> Result<(), Diagnostic> {
        if !self.changed {
            self.next.revision = self.baseline.revision.checked_add(1).ok_or_else(|| {
                Diagnostic::unsupported(Span::default(), "hook document revision exhausted")
            })?;
            self.epoch = Rc::new(Epoch {
                active: Cell::new(false),
            });
            self.changed = true;
        }
        Ok(())
    }

    pub(crate) fn ensure_instance(&mut self, mount: Mount) -> Result<(), Diagnostic> {
        crate::domain::validation::validate(&mount.definition)?;
        if mount.definition_revision == 0 {
            return Err(Diagnostic::unsupported(
                mount.definition.root.span,
                "positive hook definition revision required",
            ));
        }
        if let Some(current) = self.instances.get(&mount.instance) {
            if current.definition_revision != mount.definition_revision
                || current.bindings != mount.definition.states
            {
                return Err(Diagnostic::unsupported(
                    mount.definition.root.span,
                    "hook definition replacement requires a prepared migration",
                ));
            }
            return Ok(());
        }
        self.advance_frame()?;
        if self.instances.len() >= 16_384 {
            self.reject();
            return Err(Diagnostic::unsupported(
                mount.definition.root.span,
                "hook instance budget exceeded",
            ));
        }
        let mut roots = Roots::new();
        for binding in &mount.definition.states {
            let expression = literal(&binding.initial, binding.span)?;
            let root = evaluate(
                &mut self.candidate,
                &expression,
                Bindings {
                    states: &roots,
                    previous: None,
                },
            )
            .map_err(|(diagnostic, _)| diagnostic)?;
            roots.insert(binding.name.clone(), root);
        }
        self.instances.insert(
            mount.instance,
            Instance {
                bindings: mount.definition.states,
                definition_revision: mount.definition_revision,
                roots,
            },
        );
        Ok(())
    }

    pub(crate) fn retain_instances(
        &mut self,
        active: &std::collections::BTreeSet<InstanceHandle>,
    ) -> Result<(), Diagnostic> {
        if self
            .instances
            .keys()
            .any(|instance| !active.contains(instance))
        {
            self.advance_frame()?;
            self.instances
                .retain(|instance, _| active.contains(instance));
        }
        Ok(())
    }

    pub(crate) fn handler(
        &self,
        instance: &InstanceHandle,
        event: StateEvent,
    ) -> Result<BoundHandler, Diagnostic> {
        let BoundAction {
            stamp,
            instance,
            definition_revision,
            event,
            captured,
            epoch,
        } = self.capture(instance, event)?;
        Ok(BoundHandler {
            stamp,
            instance,
            definition_revision,
            event,
            captured,
            epoch,
        })
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn values(&self) -> &Transaction {
        self.candidate.as_ref().expect("owned transaction")
    }

    pub fn root(&self, instance: &InstanceHandle, slot: &str) -> Option<&Value> {
        self.instances.get(instance)?.roots.get(slot)
    }

    pub fn capture(
        &self,
        instance: &InstanceHandle,
        event: StateEvent,
    ) -> Result<BoundAction, Diagnostic> {
        if !self.changed {
            return Err(Diagnostic::unsupported(
                Span::default(),
                "unchanged candidate cannot issue a new frame action",
            ));
        }
        bind(self.next, &self.instances, &self.epoch, instance, event)
    }
}

impl std::fmt::Debug for PublicationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookPublicationFailure")
            .field("reason", &self.reason)
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
