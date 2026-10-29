use super::WebDocument;
use crate::{
    application::{
        binding, dom,
        state_program::{PreparedReplacement, ReloadPolicy},
    },
    domain::{Component, Value, source_schema::SourceSchema},
};
use std::collections::BTreeMap;

pub struct PreparedSourceReplacement {
    program: PreparedReplacement,
}

fn retained_controls(previous: &dom::DomNode, next: &dom::DomNode) -> Result<(), String> {
    if let (
        dom::DomNode::Element {
            attributes,
            children,
            ..
        },
        dom::DomNode::Element {
            attributes: next_attributes,
            children: next_children,
            ..
        },
    ) = (previous, next)
    {
        for identity in ["id", "type", "role"] {
            if attributes.get(identity) != next_attributes.get(identity) {
                return Err("Web control identity changed; remount is required".into());
            }
        }
        for (before, after) in children.iter().zip(next_children) {
            retained_controls(before, after)?;
        }
    }
    Ok(())
}

impl WebDocument {
    pub fn prepare_source_replacement(
        &self,
        source: &Component,
        props: &BTreeMap<String, Value>,
    ) -> Result<PreparedSourceReplacement, String> {
        let inner = self.inner.borrow();
        if inner.module_program.is_some() {
            return Err("module graph source replacement requires a graph transaction".into());
        }
        if inner.closed || inner.poisoned {
            return Err("Web document requires recovery or is closed".into());
        }
        dom::validate_source(source)?;
        if SourceSchema::from_component(source)? != inner.source_schema {
            return Err("original Web hook/props schema changed; remount is required".into());
        }
        let bound = binding::bind_with_scope(source, props, Some(&inner.scope))?;
        let program = inner
            .program
            .prepare_replacement(bound, ReloadPolicy::PreserveCompatible)?;
        let projection = program.project()?;
        if !inner.events.keys().eq(projection.events.keys()) {
            return Err("Web handler layout changed; remount is required".into());
        }
        let plan = dom::prepare(&projection.component)?;
        inner.frame.preflight(&plan, &inner.shell)?;
        retained_controls(&inner.frame.plan, &plan)?;
        Ok(PreparedSourceReplacement { program })
    }

    pub fn commit_source_replacement(
        &mut self,
        prepared: PreparedSourceReplacement,
    ) -> Result<(), String> {
        let mut inner = self.inner.borrow_mut();
        if inner.module_program.is_some() {
            return Err("module graph source replacement requires a graph transaction".into());
        }
        if inner.closed || inner.poisoned {
            return Err("Web document requires recovery or is closed".into());
        }
        let mut program = inner.program.clone();
        let projection = program.commit_replacement(prepared.program)?;
        if !inner.events.keys().eq(projection.events.keys()) {
            return Err("Web handler layout changed; remount is required".into());
        }
        let plan = dom::prepare(&projection.component)?;
        retained_controls(&inner.frame.plan, &plan)?;
        let shell = inner.shell.clone();
        if let Err((error, poisoned)) = inner.frame.patch(plan, &shell) {
            inner.poisoned = poisoned;
            inner.last_error = Some(error.clone());
            return Err(error);
        }
        inner.program = program;
        inner.events = projection.events;
        inner.last_error = None;
        Ok(())
    }
}
