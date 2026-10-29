use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    pub sources: Vec<Source>,
    pub pages: Vec<Page>,
    pub examples: Vec<Example>,
}

#[derive(Serialize, Deserialize)]
pub struct Source {
    pub project: String,
    pub revision: String,
    pub repository: String,
    pub license: String,
}

#[derive(Serialize, Deserialize)]
pub struct Page {
    pub project: String,
    pub path: String,
    pub url: String,
    pub sha256: String,
    pub provider: String,
}

#[derive(Serialize, Deserialize)]
pub struct Example {
    pub id: usize,
    pub page: usize,
    pub line: usize,
    pub kind: String,
    pub language: String,
    pub name: String,
    pub source: Option<String>,
    pub sha256: Option<String>,
    pub collection: String,
}

#[derive(Serialize, Deserialize)]
pub struct Receipt {
    pub id: usize,
    pub status: String,
    pub detail: String,
    pub screenshot: Option<String>,
    pub interaction: String,
    pub accessibility: String,
    #[serde(default)]
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckStatus {
    Passed,
    Failed,
    NotRun,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub gate: String,
    pub status: CheckStatus,
    pub detail: String,
}
