//! Original stylesheet sources, independent of compiler, filesystem and renderer.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StylesheetMode {
    Css,
    Tailwind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylesheetProject {
    pub mode: StylesheetMode,
    pub entry: String,
    pub files: BTreeMap<String, String>,
    pub bridge_theme: bool,
}

impl StylesheetProject {
    pub fn inline(mode: StylesheetMode, css: impl Into<String>) -> Self {
        Self {
            mode,
            entry: "styles.css".into(),
            files: BTreeMap::from([("styles.css".into(), css.into())]),
            bridge_theme: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        fn path(value: &str) -> bool {
            !value.is_empty()
                && !value.contains(['\\', ':', '\0'])
                && value
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != "..")
        }
        if !path(&self.entry) || !self.files.contains_key(&self.entry) {
            return Err("stylesheet entry must name an included relative file".into());
        }
        if self.files.keys().any(|p| !path(p)) {
            return Err("stylesheet files must have normalized relative paths".into());
        }
        if self.mode == StylesheetMode::Css && self.bridge_theme {
            return Err("bridge theme injection requires Tailwind mode".into());
        }
        Ok(())
    }
}
