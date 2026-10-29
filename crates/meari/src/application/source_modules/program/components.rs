//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::node_slots::{self};
use super::super::{ComponentReference, ModuleDiagnostic};
use super::*;
use crate::{
    application::{
        binding,
        state_program::{ReloadPolicy, StateProgram},
    },
    domain::{Node, Span, Value, build::ContentHash},
};
use std::{collections::BTreeMap, rc::Rc};

impl ProjectionContext<'_> {
    pub(super) fn component(
        &mut self,
        reference: &ComponentReference,
        props: &BTreeMap<String, Value>,
        instance: &str,
        output: &str,
        parent: Option<&str>,
        depth: usize,
    ) -> Result<Node, ModuleDiagnostic> {
        let module = Rc::new(
            self.graph
                .definition(reference)
                .map_err(|message| failure(&reference.module, Span::default(), message))?
                .clone(),
        );
        if depth > 128 || self.next.len() >= 16384 {
            return Err(failure(
                &reference.module,
                module.component.root.span,
                "component instance/depth budget exceeded",
            ));
        }
        let scope = ContentHash::of(
            &serde_json::to_vec(&(self.document, self.epoch, instance, reference))
                .map_err(|error| failure(&reference.module, Span::default(), error.to_string()))?,
        );
        let bound = binding::bind_with_scope(&module.component, props, Some(scope.as_str()))
            .map_err(|message| failure(&reference.module, module.component.root.span, message))?;
        let (state, projection) = if let Some(pass) = &mut self.hooks {
            let parent = parent
                .map(|parent| {
                    match &self
                        .next
                        .get(parent)
                        .ok_or_else(|| {
                            failure(
                                &reference.module,
                                Span::default(),
                                "hook parent instance missing",
                            )
                        })?
                        .state
                    {
                        MountedState::Hook(handle) => Ok(handle.clone()),
                        _ => Err(failure(
                            &reference.module,
                            Span::default(),
                            "mixed hook/scalar module ownership",
                        )),
                    }
                })
                .transpose()?;
            let identity =
                ContentHash::of(&serde_json::to_vec(reference).map_err(|error| {
                    failure(&reference.module, Span::default(), error.to_string())
                })?);
            let handle = pass
                .registry
                .mount(
                    parent.as_ref(),
                    crate::domain::component_state::InstanceKey::Explicit(
                        ContentHash::of(instance.as_bytes()).as_str().into(),
                    ),
                    identity.as_str().into(),
                    crate::domain::component_state::StateSchema {
                        revision: "hook-module-instance-v1".into(),
                        slots: BTreeMap::new(),
                    },
                )
                .map_err(|message| failure(&reference.module, bound.root.span, message))?;
            pass.prepared
                .ensure_instance(crate::application::hook_document::Mount {
                    instance: handle.clone(),
                    definition: bound.clone(),
                    definition_revision: 1,
                })
                .map_err(|diagnostic| ModuleDiagnostic {
                    file: reference.module.clone(),
                    diagnostic,
                })?;
            let projection = pass
                .prepared
                .project(&handle, &bound)
                .map_err(|diagnostic| ModuleDiagnostic {
                    file: reference.module.clone(),
                    diagnostic,
                })?;
            if !projection.inputs.is_empty() {
                return Err(failure(
                    &reference.module,
                    bound.root.span,
                    "hook module inputs require prepared payload dispatch",
                ));
            }
            (MountedState::Hook(handle), Rc::new(projection))
        } else {
            let program = if let Some(previous) = self
                .previous
                .get(instance)
                .filter(|previous| previous.reference == *reference)
            {
                let MountedState::Scalar(previous_program) = &previous.state else {
                    return Err(failure(
                        &reference.module,
                        Span::default(),
                        "hook module requires prepared projection",
                    ));
                };
                let mut program = previous_program.as_ref().clone();
                if !previous.bound.semantic_eq(&bound) {
                    let replacement = program
                        .prepare_replacement(bound.clone(), ReloadPolicy::PreserveCompatible)
                        .map_err(|message| {
                            failure(&reference.module, module.component.root.span, message)
                        })?;
                    program.commit_replacement(replacement).map_err(|message| {
                        failure(&reference.module, module.component.root.span, message)
                    })?;
                }
                program
            } else {
                StateProgram::mount(
                    bound.clone(),
                    format!(
                        "module-{}-{}",
                        scope.as_str(),
                        node_slots::identity().map_err(|message| {
                            failure(&reference.module, module.component.root.span, message)
                        })?
                    ),
                    self.epoch,
                )
                .map_err(|message| {
                    failure(&reference.module, module.component.root.span, message)
                })?
            };
            let projection = Rc::new(program.project().map_err(|message| {
                failure(&reference.module, module.component.root.span, message)
            })?);
            (MountedState::Scalar(Box::new(program)), projection)
        };
        self.next.insert(
            instance.into(),
            MountedModule {
                reference: reference.clone(),
                bound,
                state,
            },
        );
        self.render_stack.push(instance.into());
        let nodes = self.node(
            projection.component.root.clone(),
            &module,
            reference,
            instance,
            "0",
            "0",
            output,
            &projection,
            depth,
        );
        self.render_stack.pop();
        let mut nodes = nodes?;
        if nodes.len() != 1 {
            return Err(failure(
                &reference.module,
                module.component.root.span,
                "component must project exactly one root node",
            ));
        }
        Ok(nodes.remove(0))
    }
}
