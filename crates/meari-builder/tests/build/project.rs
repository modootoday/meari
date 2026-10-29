use meari::{
    application::builder::{BuildFailure, BuildInputs},
    domain::build::*,
};
use meari_builder::application::project::{ProjectFile, ProjectRequest};
use std::collections::{BTreeMap, BTreeSet};

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}
struct Inputs(BTreeMap<BuildPath, Vec<u8>>);
impl BuildInputs for Inputs {
    fn read(&self, name: &BuildPath) -> Result<Vec<u8>, String> {
        self.0
            .get(name)
            .cloned()
            .ok_or_else(|| "missing source".into())
    }
}
fn request() -> ProjectRequest {
    ProjectRequest {
        entry: path("counter.tsx"),
        configuration: path("app.json"),
        files: [
            ("counter.tsx", InputRole::Component),
            ("app.json", InputRole::Runtime),
            ("styles.css", InputRole::Stylesheet),
        ]
        .into_iter()
        .map(|(name, role)| {
            (
                path(name),
                ProjectFile {
                    role,
                    dependencies: BTreeSet::new(),
                    license: None,
                },
            )
        })
        .collect(),
    }
}
#[test]
fn configuration_and_styles_are_graph_dependencies() {
    let request = request();
    let inputs = Inputs(
        request
            .files
            .keys()
            .map(|name| (name.clone(), name.as_str().as_bytes().to_vec()))
            .collect(),
    );
    let loaded = request.load(&inputs).unwrap();
    assert_eq!(
        loaded.graph.files[&request.entry].dependencies,
        BTreeSet::from([path("app.json"), path("styles.css")])
    );
    let before = loaded.graph.fingerprint().unwrap();
    let mut changed = inputs;
    changed
        .0
        .insert(path("styles.css"), b"p {color:red}".to_vec());
    assert_ne!(
        before,
        request.load(&changed).unwrap().graph.fingerprint().unwrap()
    );
}
#[test]
fn missing_input_has_file_diagnostic_without_invented_span() {
    let error = request().load(&Inputs(BTreeMap::new())).err().unwrap();
    assert_eq!(error.diagnostics.len(), 1);
    assert!(error.diagnostics[0].source.is_some());
    assert!(error.diagnostics[0].span.is_none());
}
#[test]
fn frontend_failure_retains_original_span_and_code() {
    let diagnostic = meari::parse("@@@", meari::application::Language::Tsx).unwrap_err();
    let expected = serde_json::to_value(&diagnostic).unwrap();
    let failure = BuildFailure::source(path("counter.tsx"), diagnostic);
    let actual = serde_json::to_value(&failure.diagnostics[0]).unwrap();
    assert_eq!(actual["span"], expected["span"]);
    assert_eq!(actual["code"], expected["code"]);
    assert_eq!(actual["source"], "counter.tsx");
}
