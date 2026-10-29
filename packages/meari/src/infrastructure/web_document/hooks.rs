use super::{Inner, WebDocument};
use crate::{
    application::{
        dom,
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
use web_sys::Element;

#[derive(Debug)]
pub enum WebHookFailure {
    Prepare(Box<HookModuleFailure>),
    Publish(Box<HookModulePublicationFailure<String>>),
}

impl Inner {
    pub(super) fn dispatch_hook_module(&mut self, path: &str) -> Result<bool, String> {
        let program = self
            .module_program
            .as_ref()
            .ok_or("Web hook module program missing")?;
        let hooks = self.hook_document.as_ref().ok_or("Web hook heap missing")?;
        let prepared = match program.prepare_hook_action(hooks, path) {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = format!("{:?}", error.diagnostic);
                self.hook_failure = Some(WebHookFailure::Prepare(error));
                return Err(message);
            }
        };
        self.apply_hook_module(prepared)
    }

    pub(super) fn update_hook_props(
        &mut self,
        props: BTreeMap<String, Value>,
    ) -> Result<(), String> {
        if self.closed || self.poisoned {
            return Err("Web hook document is closed or requires recovery".into());
        }
        let program = self
            .module_program
            .as_ref()
            .ok_or("Web hook module program missing")?;
        let hooks = self.hook_document.as_ref().ok_or("Web hook heap missing")?;
        let prepared = match program.prepare_hook_props(hooks, props) {
            Ok(prepared) => prepared,
            Err(error) => {
                let message = format!("{:?}", error.diagnostic);
                self.hook_failure = Some(WebHookFailure::Prepare(error));
                return Err(message);
            }
        };
        self.apply_hook_module(prepared).map(|_| ())
    }

    fn apply_hook_module(&mut self, prepared: PreparedHookModule) -> Result<bool, String> {
        if !prepared.changed() {
            return Ok(false);
        }
        let hooks = self.hook_document.as_mut().ok_or("Web hook heap missing")?;
        let result = prepared.publish_with(hooks, |program| {
            let projection = program.project().map_err(|error| format!("{error:?}"))?;
            if self.callbacks.is_some() || !projection.callback_bindings().is_empty() {
                return Err("Web hook callback exports require an atomic callback adapter".into());
            }
            let plan = dom::prepare(&projection.component)?;
            self.frame
                .patch(plan, &self.shell)
                .map_err(|(error, poisoned)| {
                    self.poisoned = poisoned;
                    error
                })?;
            Ok::<_, String>(())
        });
        match result {
            Ok((program, ())) => {
                self.module_program = Some(program);
                self.hook_failure = None;
                self.last_error = None;
                Ok(true)
            }
            Err(error) => {
                let message = format!("{:?}", error.problem);
                self.hook_failure = Some(WebHookFailure::Publish(error));
                self.last_error = Some(message.clone());
                Err(message)
            }
        }
    }
}

impl WebDocument {
    pub fn mount_hook_modules(
        container: &Element,
        graph: SourceModuleGraph,
        props: BTreeMap<String, Value>,
        css: &str,
        identity: (String, u64),
        source_generation: u64,
        budget: Budget,
    ) -> Result<Self, String> {
        Self::mount_hook_modules_with_node_slots(
            container,
            graph,
            css,
            identity,
            source_generation,
            budget,
            |_| Ok(props),
        )
    }

    pub fn mount_hook_modules_with_node_slots(
        container: &Element,
        graph: SourceModuleGraph,
        css: &str,
        identity: (String, u64),
        source_generation: u64,
        budget: Budget,
        prepare: impl FnOnce(
            &crate::application::source_modules::NodeSlotFactory<'_>,
        ) -> Result<BTreeMap<String, Value>, String>,
    ) -> Result<Self, String> {
        let mut hooks = HookDocument::new(source_generation, budget).map_err(str::to_owned)?;
        let prepared = ModuleProgram::prepare_hook_mount_with_node_slots(
            graph,
            identity.0.clone(),
            identity.1,
            &hooks,
            prepare,
        )
        .map_err(|error| format!("{:?}", error.diagnostic))?;
        let (program, document) = prepared
            .publish_with(&mut hooks, |program| {
                let projection = program.project().map_err(|error| format!("{error:?}"))?;
                if !projection.callback_bindings().is_empty() {
                    return Err(
                        "Web hook callback exports require an atomic callback adapter".into(),
                    );
                }
                Self::mount(
                    container,
                    &projection.component,
                    &BTreeMap::new(),
                    css,
                    identity.0,
                    identity.1,
                )
            })
            .map_err(|error| format!("{:?}", error.problem))?;
        {
            let mut inner = document.inner.borrow_mut();
            inner.module_program = Some(program);
            inner.hook_document = Some(hooks);
        }
        Ok(document)
    }

    pub fn with_hook_state<R>(
        &self,
        visit: impl FnOnce(&HookDocument, &ModuleProjection) -> R,
    ) -> Result<R, String> {
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| "Web hook document is borrowed")?;
        if inner.closed || inner.poisoned {
            return Err("Web hook document is closed or requires recovery".into());
        }
        let hooks = inner
            .hook_document
            .as_ref()
            .ok_or("Web hook heap missing")?;
        let projection = inner
            .module_program
            .as_ref()
            .ok_or("Web hook module program missing")?
            .project()
            .map_err(|error| format!("{error:?}"))?;
        Ok(visit(hooks, projection))
    }

    pub fn with_hook_failure<R>(
        &self,
        visit: impl FnOnce(Option<&WebHookFailure>) -> R,
    ) -> Result<R, String> {
        let inner = self
            .inner
            .try_borrow()
            .map_err(|_| "Web hook document is borrowed")?;
        Ok(visit(inner.hook_failure.as_ref()))
    }
}
