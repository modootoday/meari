use super::{
    build::{BuildPath, ContentHash, TargetKind},
    component_state::{InstanceHandle, identifier},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PLUGIN_API_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub targets: BTreeSet<TargetKind>,
    pub request_schema: ContentHash,
    pub reply_schema: ContentHash,
    pub error_schema: Option<ContentHash>,
    pub event_schema: Option<ContentHash>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub api_version: u32,
    pub id: String,
    pub version: String,
    pub contract: ContentHash,
    pub provider: ContentHash,
    pub capabilities: BTreeMap<String, CapabilityDescriptor>,
    pub assets: BTreeMap<BuildPath, ContentHash>,
}
impl PluginManifest {
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.api_version != PLUGIN_API_VERSION {
            return Err(PluginError::new(
                PluginErrorCode::IncompatibleApi,
                "plugin API version is unsupported",
            ));
        }
        if !name(&self.id)
            || !identifier(&self.version)
            || self.capabilities.is_empty()
            || self.capabilities.len() > 1024
            || self.assets.len() > 16384
        {
            return Err(PluginError::new(
                PluginErrorCode::InvalidManifest,
                "invalid plugin identity or inventory",
            ));
        }
        for (id, capability) in &self.capabilities {
            if !name(id) || capability.targets.is_empty() {
                return Err(PluginError::new(
                    PluginErrorCode::InvalidManifest,
                    "invalid plugin capability",
                ));
            }
        }
        Ok(())
    }
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-:".contains(&byte))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginScope {
    pub instance: InstanceHandle,
    pub source_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPayload {
    pub schema: ContentHash,
    pub value: serde_json::Value,
}
impl PluginPayload {
    pub fn validate(&self) -> Result<(), PluginError> {
        let mut stack = vec![(&self.value, 0usize)];
        let mut nodes = 0usize;
        let mut text_bytes = 0usize;
        while let Some((value, depth)) = stack.pop() {
            nodes += 1;
            if nodes > 65536 || depth > 64 {
                return Err(PluginError::new(
                    PluginErrorCode::InvalidPayload,
                    "plugin payload structure exceeds budget",
                ));
            }
            match value {
                serde_json::Value::String(value) => {
                    text_bytes = text_bytes.saturating_add(value.len())
                }
                serde_json::Value::Array(values) => {
                    stack.extend(values.iter().map(|value| (value, depth + 1)))
                }
                serde_json::Value::Object(values) => {
                    for (key, value) in values {
                        text_bytes = text_bytes.saturating_add(key.len());
                        stack.push((value, depth + 1));
                    }
                }
                _ => {}
            }
            if text_bytes > 1024 * 1024 || stack.len() > 65536 {
                return Err(PluginError::new(
                    PluginErrorCode::InvalidPayload,
                    "plugin payload exceeds budget",
                ));
            }
        }
        if serde_json::to_vec(&self.value)
            .map_err(|error| PluginError::new(PluginErrorCode::InvalidPayload, error.to_string()))?
            .len()
            > 1024 * 1024
        {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "plugin wire payload exceeds budget",
            ));
        }
        Ok(())
    }
    pub fn decode<T: serde::de::DeserializeOwned>(
        &self,
        schema: &ContentHash,
    ) -> Result<T, PluginError> {
        self.validate()?;
        if &self.schema != schema {
            return Err(PluginError::new(
                PluginErrorCode::InvalidPayload,
                "plugin payload schema differs",
            ));
        }
        serde_json::from_value(self.value.clone())
            .map_err(|error| PluginError::new(PluginErrorCode::InvalidPayload, error.to_string()))
    }
    pub fn encode<T: Serialize>(schema: ContentHash, value: &T) -> Result<Self, PluginError> {
        let payload = Self {
            schema,
            value: serde_json::to_value(value).map_err(|error| {
                PluginError::new(PluginErrorCode::InvalidPayload, error.to_string())
            })?,
        };
        payload.validate()?;
        Ok(payload)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginErrorCode {
    IncompatibleApi,
    InvalidManifest,
    Duplicate,
    UnsupportedTarget,
    PermissionDenied,
    MissingCapability,
    InvalidPayload,
    Stale,
    Busy,
    Closing,
    Adapter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginError {
    pub code: PluginErrorCode,
    pub message: String,
    pub details: Option<PluginPayload>,
}
impl PluginError {
    pub fn new(code: PluginErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }
}
impl std::fmt::Display for PluginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}
impl std::error::Error for PluginError {}
