use super::*;
use std::collections::BTreeSet;

fn path(value: &str) -> BuildPath {
    value.to_owned().try_into().unwrap()
}

fn fixture() -> (BuildGraph, BTreeMap<BuildPath, Vec<u8>>, StylesheetProject) {
    let css = b"main{background-image:url('./logo.svg')}".to_vec();
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec();
    let inputs = BTreeMap::from([(path("styles.css"), css), (path("logo.svg"), svg)]);
    let graph = BuildGraph {
        entry: path("styles.css"),
        files: inputs
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    InputFile {
                        role: if path.as_str().ends_with(".css") {
                            InputRole::Stylesheet
                        } else {
                            InputRole::Icon
                        },
                        content: ContentHash::of(bytes),
                        length: bytes.len() as u64,
                        dependencies: BTreeSet::new(),
                        license: Some("MIT".into()),
                    },
                )
            })
            .collect(),
    };
    let project = StylesheetProject::inline(
        meari::domain::design::stylesheet::StylesheetMode::Css,
        String::from_utf8(inputs[&path("styles.css")].clone()).unwrap(),
    );
    (graph, inputs, project)
}

#[test]
fn retains_original_css_and_maps_rewritten_resource_to_exact_bytes() {
    let (graph, inputs, project) = fixture();
    let result = prepare(&graph, &inputs, project).unwrap();
    let reference = &result.references[0];
    assert_eq!(reference.source, path("logo.svg"));
    assert_eq!(
        result.originals[&path("source/original/styles.css")],
        inputs[&path("styles.css")]
    );
    assert_eq!(
        result
            .verified
            .as_ref()
            .unwrap()
            .resource(&reference.output)
            .unwrap()
            .1,
        inputs[&path("logo.svg")]
    );
    assert!(result.project.files["styles.css"].contains(reference.output.as_str()));
    assert_eq!(result.mappings.len(), 2);
}

#[test]
fn rejects_changed_bytes_missing_declaration_and_unknown_url() {
    let (graph, mut inputs, project) = fixture();
    inputs.get_mut(&path("logo.svg")).unwrap().push(b' ');
    assert!(prepare(&graph, &inputs, project.clone()).is_err());
    let (mut graph, inputs, _) = fixture();
    graph.files.remove(&path("logo.svg"));
    assert!(prepare(&graph, &inputs, project).is_err());
}
