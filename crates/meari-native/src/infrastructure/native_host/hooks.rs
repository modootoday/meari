use super::{HostAccessError, HostCommitReceipt, NativeDocumentHandle};
use crate::{domain::Viewport, infrastructure::document::InteractiveDocument};
use meari::{
    application::{
        hook_document::HookDocument,
        hook_values::Budget,
        source_modules::{
            HookModuleFailure, HookModulePublicationFailure, ModuleProgram, ModuleProjection,
            PreparedHookModule,
        },
    },
    domain::{Value, source_modules::SourceModuleGraph},
};
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum NativeHookFailure {
    Prepare(Box<HookModuleFailure>),
    Publish(Box<HookModulePublicationFailure<String>>),
}
pub(super) struct HookRuntime {
    pub owner: HookDocument,
    pub failure: Option<NativeHookFailure>,
}

impl NativeDocumentHandle {
    pub fn mount_hook_modules(
        graph: SourceModuleGraph,
        props: BTreeMap<String, Value>,
        identity: (String, u64),
        css: &str,
        viewport: Viewport,
        source_generation: u64,
        budget: Budget,
    ) -> Result<Self, HostAccessError> {
        Self::mount_hook_modules_with_node_slots(
            graph,
            identity,
            css,
            viewport,
            source_generation,
            budget,
            |_| Ok(props),
        )
    }

    pub fn mount_hook_modules_with_node_slots(
        graph: SourceModuleGraph,
        identity: (String, u64),
        css: &str,
        viewport: Viewport,
        source_generation: u64,
        budget: Budget,
        prepare: impl FnOnce(
            &meari::application::source_modules::NodeSlotFactory<'_>,
        ) -> Result<BTreeMap<String, Value>, String>,
    ) -> Result<Self, HostAccessError> {
        let mut owner = HookDocument::new(source_generation, budget)
            .map_err(|message| HostAccessError::Invalid(message.into()))?;
        let prepared = ModuleProgram::prepare_hook_mount_with_node_slots(
            graph, identity.0, identity.1, &owner, prepare,
        )
        .map_err(|error| HostAccessError::Invalid(format!("{:?}", error.diagnostic)))?;
        let (_, handle) = prepared
            .publish_with(&mut owner, |program| {
                let document =
                    InteractiveDocument::from_prepared_hook_program(program.clone(), css, viewport)
                        .map_err(HostAccessError::Invalid)?;
                Self::new_hook_candidate(document, source_generation)
            })
            .map_err(|error| HostAccessError::Invalid(format!("{:?}", error.problem)))?;
        *handle
            .0
            .hooks
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)? = Some(HookRuntime {
            owner,
            failure: None,
        });
        Ok(handle)
    }

    pub fn with_hook_state<R>(
        &self,
        visit: impl FnOnce(&HookDocument, &ModuleProjection) -> R,
    ) -> Result<R, HostAccessError> {
        let runtime = self
            .0
            .hooks
            .try_borrow()
            .map_err(|_| HostAccessError::Busy)?;
        let owner = &runtime
            .as_ref()
            .ok_or_else(|| HostAccessError::Invalid("host has no hook heap".into()))?
            .owner;
        self.try_with(|document| {
            document
                .module_projection()
                .map(|projection| visit(owner, projection))
                .map_err(HostAccessError::Invalid)
        })?
    }

    pub fn with_hook_failure<R>(
        &self,
        visit: impl FnOnce(Option<&NativeHookFailure>) -> R,
    ) -> Result<R, HostAccessError> {
        let runtime = self
            .0
            .hooks
            .try_borrow()
            .map_err(|_| HostAccessError::Busy)?;
        Ok(visit(
            runtime
                .as_ref()
                .and_then(|runtime| runtime.failure.as_ref()),
        ))
    }

    pub fn update_hook_props(
        &self,
        props: BTreeMap<String, Value>,
    ) -> Result<Option<HostCommitReceipt>, HostAccessError> {
        self.require_open()?;
        if self.0.input_blocked.get() {
            return Err(HostAccessError::Busy);
        }
        let mut runtime = self
            .0
            .hooks
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?;
        let runtime = runtime
            .as_mut()
            .ok_or_else(|| HostAccessError::Invalid("host has no hook heap".into()))?;
        let program = self
            .try_with(|document| document.module_program.clone())?
            .ok_or_else(|| HostAccessError::Invalid("hook module program missing".into()))?;
        let prepared = match program.prepare_hook_props(&runtime.owner, props) {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = format!("{:?}", error.diagnostic);
                runtime.failure = Some(NativeHookFailure::Prepare(error));
                return Err(HostAccessError::Invalid(message));
            }
        };
        if !prepared.changed() {
            // A new explicit props publication cannot retain the previous observation.
            self.0.observation.invalidate();
        }
        self.apply_hook_candidate(
            runtime,
            prepared,
            super::retained_observation::MutationKind::Projection,
        )
    }

    pub(super) fn flush_hook_actions(&self) -> Result<Option<bool>, HostAccessError> {
        if self.try_with(|document| document.hook_actions.is_empty())? {
            return Ok(None);
        }
        let mut runtime = self
            .0
            .hooks
            .try_borrow_mut()
            .map_err(|_| HostAccessError::Busy)?;
        let runtime = runtime
            .as_mut()
            .ok_or_else(|| HostAccessError::Invalid("hook event has no owning heap".into()))?;
        let (program, actions) = self.with_internal_mut(|document| {
            (
                document.module_program.clone(),
                std::mem::take(&mut document.hook_actions),
            )
        })?;
        let program = program
            .ok_or_else(|| HostAccessError::Invalid("hook module program missing".into()))?;
        let prepared = match program.prepare_hook_actions(&runtime.owner, actions) {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = format!("{:?}", error.diagnostic);
                runtime.failure = Some(NativeHookFailure::Prepare(error));
                return Err(HostAccessError::Invalid(message));
            }
        };
        let changed = prepared.changed();
        self.apply_hook_candidate(
            runtime,
            prepared,
            super::retained_observation::MutationKind::Input,
        )?;
        Ok(Some(changed))
    }

    fn apply_hook_candidate(
        &self,
        runtime: &mut HookRuntime,
        prepared: PreparedHookModule,
        kind: super::retained_observation::MutationKind,
    ) -> Result<Option<HostCommitReceipt>, HostAccessError> {
        if !prepared.changed() {
            return Ok(None);
        }
        let expected = self.stamp()?;
        let previous = self
            .try_with(|document| document.module_program.clone())?
            .ok_or_else(|| HostAccessError::Invalid("hook module program missing".into()))?;
        let mut applied = false;
        let result = prepared.publish_with(&mut runtime.owner, |program| {
            let projection = program
                .project()
                .map_err(|error| format!("{error:?}"))?
                .clone();
            let result = self.try_reproject_kind(&expected, kind, |document| {
                document.commit_module_projection(program.clone(), projection)?;
                applied = true;
                Ok::<_, String>(())
            });
            match result {
                Ok(((), receipt))
                    if receipt.before == expected
                        && receipt.after.owner == expected.owner
                        && receipt.after.host_id == expected.host_id =>
                {
                    Ok(receipt)
                }
                result => {
                    let message = match result {
                        Err(error) => error.to_string(),
                        Ok(_) => "native hook host receipt changed owner".into(),
                    };
                    let poisoned = self
                        .try_with(|document| document.source_input_poisoned)
                        .unwrap_or(true);
                    if applied || poisoned {
                        // No whole-document rollback is implied by an invalid final host receipt.
                        self.0.input_blocked.set(true);
                        let _ = self.with_internal_mut(|document| {
                            document.module_program = Some(previous.clone());
                            document.source_input_poisoned = true;
                            document.last_error = Some(format!(
                                "{message}; hook renderer state is unconfirmed and requires remount"
                            ));
                        });
                    }
                    Err(message)
                }
            }
        });
        match result {
            Ok((_, receipt)) => {
                runtime.failure = None;
                Ok(Some(receipt))
            }
            Err(error) => {
                if applied {
                    self.0.observation.invalidate();
                }
                let message = format!("{:?}", error.problem);
                runtime.failure = Some(NativeHookFailure::Publish(error));
                Err(HostAccessError::Invalid(message))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_and_forced_close_release_hook_authority_before_a_retained_handle_is_dropped() {
        fn mounted() -> NativeDocumentHandle {
            let path = meari::domain::build::BuildPath::try_from("root.tsx".to_owned()).unwrap();
            let source = "import {useState} from 'react';export function Root(){const [state,setState]=useState({count:0});return <button onClick={()=>setState({count:1})}>set</button>;}";
            let graph =
                meari::parse_module_graph(&path, &BTreeMap::from([(path.clone(), source.into())]))
                    .unwrap();
            NativeDocumentHandle::mount_hook_modules(
                graph,
                BTreeMap::new(),
                ("closing-hooks".into(), 1),
                "",
                Viewport {
                    width: 320,
                    height: 240,
                    dark: false,
                },
                1,
                Budget::default(),
            )
            .unwrap()
        }
        for forced in [false, true] {
            let handle = mounted();
            let root = handle
                .with_hook_state(|owner, projection| {
                    owner
                        .snapshot(projection.instance("root").unwrap())
                        .unwrap()
                        .roots["state"]
                        .clone()
                })
                .unwrap();
            if forced {
                handle.finish_close(true).unwrap();
            } else {
                handle.begin_close(0).unwrap();
            }
            let runtime = handle.0.hooks.borrow();
            let owner = &runtime.as_ref().unwrap().owner;
            assert_eq!(
                owner.values().kind(&root),
                Err(meari::application::hook_values::Error::Closed)
            );
            assert!(
                handle
                    .dispatch_action(crate::domain::widgets::Action::Activate { path: "0".into() })
                    .is_err()
            );
        }
    }
}
