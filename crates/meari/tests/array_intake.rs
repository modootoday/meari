//! The unchanged consumer graphs are positive acceptance cases, including during the initial red gate.
use meari::domain::build::{BuildPath, ContentHash};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn sources() -> BTreeMap<BuildPath, String> {
    [
        (
            "studio/cards/proposal-panel.tsx",
            include_str!("../fixtures/array-callback-graph/studio/cards/proposal-panel.tsx"),
        ),
        (
            "studio/clips/highlight-proposal-panel.tsx",
            include_str!(
                "../fixtures/array-callback-graph/studio/clips/highlight-proposal-panel.tsx"
            ),
        ),
        (
            "components/proposal-source.tsx",
            include_str!("../fixtures/array-callback-graph/components/proposal-source.tsx"),
        ),
        (
            "components/proposal-actions.tsx",
            include_str!("../fixtures/array-callback-graph/components/proposal-actions.tsx"),
        ),
    ]
    .into_iter()
    .map(|(path, source)| (path.to_owned().try_into().unwrap(), source.into()))
    .collect()
}

#[test]
fn original_array_graphs_parse_without_rewriting_sources() {
    let output = std::env::var_os("MEARI_ARRAY_INTAKE_PROOF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("meari-array-intake-{}", std::process::id()))
        });
    fs::create_dir_all(&output).unwrap();
    let sources = sources();
    let inventory: Vec<_> = sources
        .iter()
        .map(|(path, source)| {
            let destination = output.join("original").join(path.as_str());
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, source.as_bytes()).unwrap();
            serde_json::json!({"path": path, "sha256": ContentHash::of(source.as_bytes())})
        })
        .collect();
    fs::write(
        output.join("inventory.json"),
        serde_json::to_vec_pretty(&inventory).unwrap(),
    )
    .unwrap();
    let mut failures = Vec::new();
    for (name, entry) in [
        ("cards", "studio/cards/proposal-panel.tsx"),
        ("highlights", "studio/clips/highlight-proposal-panel.tsx"),
    ] {
        let entry = BuildPath::try_from(entry.to_owned()).unwrap();
        let folder = output.join(name);
        fs::create_dir_all(&folder).unwrap();
        let case_sources = sources
            .iter()
            .filter(|(path, _)| **path == entry || path.as_str().starts_with("components/"))
            .map(|(path, source)| (path.clone(), source.clone()))
            .collect::<BTreeMap<_, _>>();
        fs::write(
            folder.join("input-modules.json"),
            serde_json::to_vec_pretty(&case_sources.keys().collect::<Vec<_>>()).unwrap(),
        )
        .unwrap();
        match meari::parse_module_graph(&entry, &case_sources) {
            Ok(graph) => {
                fs::write(
                    folder.join("graph.json"),
                    serde_json::to_vec_pretty(&graph).unwrap(),
                )
                .unwrap();
                assert_eq!(graph.modules.len(), 3);
            }
            Err(diagnostic) => {
                fs::write(
                    folder.join("first-diagnostic.json"),
                    serde_json::to_vec_pretty(&diagnostic).unwrap(),
                )
                .unwrap();
                failures.push(format!(
                    "{name}: {}:{}..{} {}",
                    diagnostic.file.as_str(),
                    diagnostic.diagnostic.span.start,
                    diagnostic.diagnostic.span.end,
                    diagnostic.diagnostic.message
                ));
            }
        }
    }
    fs::write(output.join("result.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "original_preserved": true, "parser_verified": failures.is_empty(), "native_verified": false,
        "web_verified": false, "failures": failures,
    })).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "Original graph acceptance remains incomplete: {}",
        failures.join("; ")
    );
}
