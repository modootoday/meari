use super::*;
use meari::application::app_bundle::{AppBuildStamp, ApplicationDefinition};
use meari::domain::viewport::Viewport;

pub(super) fn attach(bundle: &mut BuildBundle, definition: &ApplicationDefinition) {
    let bytes = serde_json::to_vec(definition).unwrap();
    bundle.manifest.outputs.insert(
        path("runtime/application.json"),
        OutputFile {
            role: OutputRole::Asset,
            content: ContentHash::of(&bytes),
            length: bytes.len() as u64,
        },
    );
    bundle.files.insert(path("runtime/application.json"), bytes);
}

#[test]
fn runtime_payload_checks_integrity_stamp_and_shared_hmr_identity() {
    let mut builder = BuildCoordinator::new(ContentHash::of(b"runtime-session"));
    let ticket = builder.request(graph(), targets()).unwrap();
    let mut artifacts = Vec::new();
    for kind in [TargetKind::Native, TargetKind::Web] {
        let mut bundle = bundle(&builder, &ticket, &target(kind));
        assert!(bundle.application_definition().is_err());
        let definition = ApplicationDefinition {
            component: meari::parse(source(), Language::Tsx).unwrap(),
            props: BTreeMap::new(),
            title: "Runtime".into(),
            native_viewport: Viewport {
                width: 640,
                height: 480,
                dark: false,
            },
            stamp: AppBuildStamp {
                session: ticket.session.clone(),
                generation: ticket.generation,
                graph_hash: ticket.graph_hash.clone(),
                target: target(kind),
            },
        };
        attach(&mut bundle, &definition);
        assert_eq!(
            bundle.application_definition().unwrap().component,
            definition.component
        );
        artifacts.push(bundle.hmr_artifact().unwrap());
        bundle
            .files
            .get_mut(&path("runtime/application.json"))
            .unwrap()
            .push(b' ');
        assert!(bundle.hmr_artifact().is_err());
        let mut stale = definition.clone();
        stale.stamp.generation += 1;
        attach(&mut bundle, &stale);
        assert!(bundle.application_definition().is_err());
        let mut wrong = definition;
        wrong.stamp.target = target(match kind {
            TargetKind::Native => TargetKind::Web,
            TargetKind::Web => TargetKind::Native,
        });
        attach(&mut bundle, &wrong);
        assert!(bundle.application_definition().is_err());
    }
    assert_eq!(artifacts[0].source_hash, artifacts[1].source_hash);
    assert_eq!(artifacts[0].state_schema, artifacts[1].state_schema);
    assert_ne!(artifacts[0].artifact_id, artifacts[1].artifact_id);
}
