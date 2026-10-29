//! Common generated-app definition. Host adapters own window and browser operations.
use crate::domain::{
    Component, Value,
    build::{BuildTarget, ContentHash, TargetKind},
    viewport::Viewport,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppBuildStamp {
    pub session: ContentHash,
    pub generation: u64,
    pub graph_hash: ContentHash,
    pub target: BuildTarget,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationDefinition {
    pub component: Component,
    pub props: BTreeMap<String, Value>,
    pub title: String,
    pub native_viewport: Viewport,
    pub stamp: AppBuildStamp,
}
impl ApplicationDefinition {
    pub fn validate_successor(&self, previous: &Self) -> Result<(), String> {
        previous.validate()?;
        self.validate()?;
        if self.stamp.session != previous.stamp.session
            || self.stamp.target != previous.stamp.target
            || self.stamp.generation <= previous.stamp.generation
        {
            return Err(
                "application update requires the same session/target and a newer generation".into(),
            );
        }
        Ok(())
    }

    pub fn validate_manifest(
        &self,
        manifest: &crate::domain::build::BuildManifest,
    ) -> Result<(), String> {
        manifest.validate()?;
        self.validate()?;
        let stamp = &self.stamp;
        if stamp.session != manifest.session
            || stamp.generation != manifest.generation
            || stamp.graph_hash != manifest.graph_hash
            || stamp.target != manifest.target
        {
            return Err("runtime application stamp differs from its build manifest".into());
        }
        Ok(())
    }

    pub fn source_schema(&self) -> Result<crate::domain::source_schema::SourceSchema, String> {
        crate::domain::source_schema::SourceSchema::from_component(&self.component)
    }
    pub fn validate_target(&self, target: TargetKind) -> Result<(), String> {
        self.validate()?;
        if self.stamp.target.kind != target {
            return Err("application build target differs from its runtime host".into());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty()
            || self.title.len() > 256
            || self.title.chars().any(char::is_control)
            || self.stamp.generation == 0
        {
            return Err("app title/build stamp is invalid".into());
        }
        self.native_viewport.validate()?;
        self.stamp.target.validate()?;
        self.bound_component().map(|_| ())
    }
    pub fn bound_component(&self) -> Result<Component, String> {
        super::super::binding::bind(&self.component, &self.props)
    }
}
