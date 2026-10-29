//! Source callback types contain intent, never host authority or executable code.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackSignature {
    Action,
    Text,
    Boolean,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CallbackArguments {
    Action,
    Text(String),
    Boolean(bool),
}
impl CallbackArguments {
    pub fn signature(&self) -> CallbackSignature {
        match self {
            Self::Action => CallbackSignature::Action,
            Self::Text(_) => CallbackSignature::Text,
            Self::Boolean(_) => CallbackSignature::Boolean,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if matches!(self, Self::Text(text) if text.len() > 1024 * 1024) {
            return Err("callback text exceeds byte budget".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackInvocation {
    pub prop: String,
    pub arguments: CallbackArguments,
    pub span: super::super::Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackSource {
    Action,
    CurrentTargetText,
    BooleanValue,
    /// A composite selection key read from the accepted owned item, never an input event.
    SelectionValue,
    /// A button activation carrying one string prop from the accepted render.
    CapturedText,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackHandler {
    pub prop: String,
    pub source: CallbackSource,
    pub span: super::super::Span,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_prop: Option<String>,
}

/// Serializable intent identity. Only a host-owned CallbackHandle grants dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallbackReference {
    pub binding: String,
    pub signature: CallbackSignature,
}
impl CallbackReference {
    pub fn validate(&self) -> Result<(), String> {
        if self.binding.is_empty()
            || self.binding.len() > 256
            || !self
                .binding
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("invalid callback reference name".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundCallback {
    pub reference: CallbackReference,
    pub source: CallbackSource,
    pub span: super::super::Span,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_text: Option<String>,
}
impl BoundCallback {
    pub fn validate(&self) -> Result<(), String> {
        self.reference.validate()?;
        let expected = match self.source {
            CallbackSource::Action => CallbackSignature::Action,
            CallbackSource::CurrentTargetText
            | CallbackSource::CapturedText
            | CallbackSource::SelectionValue => CallbackSignature::Text,
            CallbackSource::BooleanValue => CallbackSignature::Boolean,
        };
        if self.reference.signature != expected {
            return Err("bound callback signature differs from handler".into());
        }
        match (self.source, &self.captured_text) {
            (CallbackSource::CapturedText, Some(value)) if value.len() <= 1024 * 1024 => {}
            (CallbackSource::CapturedText, _) => {
                return Err("captured text requires a bounded accepted string value".into());
            }
            (_, Some(_)) => return Err("callback source does not accept captured text".into()),
            (_, None) => {}
        }
        Ok(())
    }
    pub fn invoke(&self, text: Option<&str>) -> Result<CallbackInvocation, String> {
        self.invoke_arguments(legacy_arguments(text)?)
    }
    pub fn invoke_arguments(
        &self,
        arguments: CallbackArguments,
    ) -> Result<CallbackInvocation, String> {
        self.validate()?;
        self.intent_bytes(&arguments)?;
        let (source, arguments) = if self.source == CallbackSource::CapturedText {
            (
                CallbackSource::CurrentTargetText,
                CallbackArguments::Text(
                    self.captured_text
                        .as_ref()
                        .expect("validated capture")
                        .clone(),
                ),
            )
        } else {
            (self.source, arguments)
        };
        CallbackHandler {
            prop: self.reference.binding.clone(),
            source,
            span: self.span,
            capture_prop: None,
        }
        .invoke_arguments(arguments)
    }
    pub(crate) fn intent_bytes(&self, arguments: &CallbackArguments) -> Result<usize, String> {
        self.validate()?;
        arguments.validate()?;
        if self.source == CallbackSource::CapturedText {
            if !matches!(arguments, CallbackArguments::Action) {
                return Err("captured text callback requires an activation intent".into());
            }
            return Ok(self
                .captured_text
                .as_ref()
                .expect("validated capture")
                .len());
        }
        if arguments.signature() != self.reference.signature {
            return Err("callback event payload differs from source handler".into());
        }
        Ok(match arguments {
            CallbackArguments::Action => 0,
            CallbackArguments::Text(value) => value.len(),
            CallbackArguments::Boolean(_) => 1,
        })
    }
}

pub(crate) fn contains_runtime(node: &super::super::Node) -> bool {
    fn runtime_value(value: &super::super::Value) -> bool {
        match value {
            super::super::Value::NodeSlot(
                super::super::node_slots::NodeSlot::Owned(_)
                | super::super::node_slots::NodeSlot::Captured(_),
            ) => true,
            super::super::Value::NodeSlot(super::super::node_slots::NodeSlot::Content(nodes)) => {
                nodes.iter().any(contains_runtime)
            }
            super::super::Value::CallbackReference(_)
            | super::super::Value::BoundCallback(_)
            | super::super::Value::Array(_)
            | super::super::Value::Record(_) => true,
            super::super::Value::Member(member) => runtime_value(&member.root),
            _ => false,
        }
    }
    match &node.kind {
        super::super::NodeKind::Text(super::super::Value::Repeat(repeat)) => {
            matches!(repeat.source, super::super::Value::Array(_))
                || contains_runtime(&repeat.template)
        }
        super::super::NodeKind::Text(value) => runtime_value(value),
        super::super::NodeKind::Element {
            attributes,
            children,
            ..
        } => {
            attributes
                .iter()
                .any(|attribute| runtime_value(&attribute.value))
                || children.iter().any(contains_runtime)
        }
    }
}
impl CallbackHandler {
    pub fn signature(&self) -> CallbackSignature {
        match self.source {
            CallbackSource::Action => CallbackSignature::Action,
            CallbackSource::CurrentTargetText
            | CallbackSource::CapturedText
            | CallbackSource::SelectionValue => CallbackSignature::Text,
            CallbackSource::BooleanValue => CallbackSignature::Boolean,
        }
    }
    pub fn invoke(&self, text: Option<&str>) -> Result<CallbackInvocation, String> {
        self.invoke_arguments(legacy_arguments(text)?)
    }
    pub fn invoke_arguments(
        &self,
        arguments: CallbackArguments,
    ) -> Result<CallbackInvocation, String> {
        if self.source == CallbackSource::CapturedText || self.capture_prop.is_some() {
            return Err("source capture requires accepted prop binding before invocation".into());
        }
        if arguments.signature() != self.signature() {
            return Err("callback event payload differs from source handler".into());
        }
        arguments.validate()?;
        Ok(CallbackInvocation {
            prop: self.prop.clone(),
            arguments,
            span: self.span,
        })
    }
}

fn legacy_arguments(text: Option<&str>) -> Result<CallbackArguments, String> {
    if text.is_some_and(|text| text.len() > 1024 * 1024) {
        return Err("callback text exceeds byte budget".into());
    }
    Ok(match text {
        Some(text) => CallbackArguments::Text(text.to_owned()),
        None => CallbackArguments::Action,
    })
}
