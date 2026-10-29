use super::{
    BuildGraph, BuildPath, ContentHash,
    identity::{nonempty, unique_paths},
};
use crate::domain::Span;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Native,
    Web,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildTarget {
    pub kind: TargetKind,
    pub triple: String,
}
impl BuildTarget {
    pub fn validate(&self) -> Result<(), String> {
        if self.triple.len() > 128
            || self.triple.split('-').count() < 3
            || self.triple.split('-').any(str::is_empty)
            || !self
                .triple
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            || match self.kind {
                TargetKind::Web => self.triple != "wasm32-unknown-unknown",
                TargetKind::Native => self.triple.starts_with("wasm"),
            }
        {
            return Err("invalid or mismatched target triple".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPin {
    pub version: String,
    pub revision: String,
    pub content: ContentHash,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildToolchain {
    pub rustc_release: String,
    pub rustc_commit: String,
    pub cargo_release: String,
    pub providers: BTreeMap<String, ProviderPin>,
    pub settings: BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputRole {
    NativeExecutable,
    NativeLibrary,
    WasmModule,
    WebModule,
    Bootstrap,
    Stylesheet,
    Asset,
    GeneratedSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputFile {
    pub role: OutputRole,
    pub content: ContentHash,
    pub length: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildMapping {
    pub source: BuildPath,
    pub generated: BuildPath,
    pub source_span: Span,
    pub generated_span: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildSeverity {
    Warning,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildDiagnostic {
    pub severity: BuildSeverity,
    pub message: String,
    pub source: Option<BuildPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildManifest {
    pub schema_version: u32,
    pub session: ContentHash,
    pub generation: u64,
    pub graph_hash: ContentHash,
    pub graph: BuildGraph,
    pub target: BuildTarget,
    pub toolchain: BuildToolchain,
    pub outputs: BTreeMap<BuildPath, OutputFile>,
    pub source_maps: Vec<BuildMapping>,
    pub available_capabilities: BTreeSet<String>,
    pub required_capabilities: BTreeSet<String>,
    pub diagnostics: Vec<BuildDiagnostic>,
}
impl BuildManifest {
    pub fn validate(&self) -> Result<(), String> {
        self.target.validate()?;
        unique_paths(self.outputs.keys())?;
        if self.schema_version != 1
            || self.generation == 0
            || self.graph_hash != self.graph.fingerprint()?
            || self.outputs.is_empty()
            || self.outputs.len() > 16384
        {
            return Err("invalid build manifest identity/inventory".into());
        }
        if ![
            &self.toolchain.rustc_release,
            &self.toolchain.rustc_commit,
            &self.toolchain.cargo_release,
        ]
        .into_iter()
        .all(|value| nonempty(value))
            || self.toolchain.providers.is_empty()
            || self.toolchain.providers.iter().any(|(name, pin)| {
                !nonempty(name) || !nonempty(&pin.version) || !nonempty(&pin.revision)
            })
        {
            return Err("Rust and backend/compiler provider pins required".into());
        }
        let roles: Vec<_> = self.outputs.values().map(|file| file.role).collect();
        let native = roles.iter().any(|role| {
            matches!(
                role,
                OutputRole::NativeExecutable | OutputRole::NativeLibrary
            )
        });
        let web = roles.contains(&OutputRole::WasmModule) && roles.contains(&OutputRole::WebModule);
        if match self.target.kind {
            TargetKind::Native => {
                !native
                    || roles.iter().any(|role| {
                        matches!(
                            role,
                            OutputRole::WasmModule | OutputRole::WebModule | OutputRole::Bootstrap
                        )
                    })
            }
            TargetKind::Web => !web || native,
        } {
            return Err("target bundle lacks required outputs or mixes executable targets".into());
        }
        if !self
            .required_capabilities
            .is_subset(&self.available_capabilities)
            || self
                .available_capabilities
                .iter()
                .any(|value| !nonempty(value))
        {
            return Err("invalid required capability".into());
        }
        for diagnostic in &self.diagnostics {
            if diagnostic.severity == BuildSeverity::Error
                || !nonempty(&diagnostic.message)
                || diagnostic
                    .source
                    .as_ref()
                    .is_some_and(|path| !self.graph.files.contains_key(path))
            {
                return Err("error or invalid diagnostic cannot be published".into());
            }
            if diagnostic.code.as_ref().is_some_and(|code| !nonempty(code))
                || diagnostic.span.is_some_and(|span| {
                    span.start > span.end
                        || diagnostic
                            .source
                            .as_ref()
                            .and_then(|path| self.graph.files.get(path))
                            .is_none_or(|file| span.end as u64 > file.length)
                })
            {
                return Err("invalid diagnostic source span/code".into());
            }
        }
        for mapping in &self.source_maps {
            let source = self
                .graph
                .files
                .get(&mapping.source)
                .ok_or("source mapping input is missing")?;
            let generated = self
                .outputs
                .get(&mapping.generated)
                .ok_or("source mapping output is missing")?;
            if generated.role != OutputRole::GeneratedSource
                || mapping.source_span.start > mapping.source_span.end
                || mapping.source_span.end as u64 > source.length
                || mapping.generated_span.start > mapping.generated_span.end
                || mapping.generated_span.end as u64 > generated.length
            {
                return Err("source mapping exceeds declared byte bounds".into());
            }
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<ContentHash, String> {
        self.validate()?;
        Ok(ContentHash::of(
            &serde_json::to_vec(self).map_err(|error| error.to_string())?,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub target: BuildTarget,
    pub identity: ContentHash,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPublication {
    pub session: ContentHash,
    pub generation: u64,
    pub graph_hash: ContentHash,
    pub artifacts: Vec<ArtifactRef>,
}
impl BuildPublication {
    pub fn validate(&self) -> Result<(), String> {
        if self.generation == 0
            || self.artifacts.is_empty()
            || self
                .artifacts
                .windows(2)
                .any(|pair| pair[0].target >= pair[1].target)
        {
            return Err("canonical unique publication targets required".into());
        }
        for artifact in &self.artifacts {
            artifact.target.validate()?;
        }
        Ok(())
    }
}
