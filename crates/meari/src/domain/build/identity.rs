use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentHash(String);
impl ContentHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ContentHash {
    type Error = String;
    fn try_from(value: String) -> Result<Self, String> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("canonical SHA256 required".into());
        }
        Ok(Self(value))
    }
}
impl From<ContentHash> for String {
    fn from(value: ContentHash) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BuildPath(String);
impl BuildPath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for BuildPath {
    type Error = String;
    fn try_from(value: String) -> Result<Self, String> {
        if value.is_empty()
            || value.len() > 4096
            || value
                .chars()
                .any(|ch| ch.is_control() || "\\:<>\"|?*".contains(ch))
        {
            return Err("normalized portable relative build path required".into());
        }
        for part in value.split('/') {
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let device = ["con", "prn", "aux", "nul"].contains(&stem.as_str())
                || ["com", "lpt"].iter().any(|prefix| {
                    stem.strip_prefix(prefix).is_some_and(|number| {
                        number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9')
                    })
                });
            if part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || device
            {
                return Err("unsafe build path segment".into());
            }
        }
        Ok(Self(value))
    }
}
impl From<BuildPath> for String {
    fn from(value: BuildPath) -> Self {
        value.0
    }
}

pub(super) fn unique_paths<'a>(paths: impl Iterator<Item = &'a BuildPath>) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for path in paths {
        if !seen.insert(path.as_str().to_ascii_lowercase()) {
            return Err("case-aliased build paths".into());
        }
    }
    Ok(())
}

pub(super) fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}
