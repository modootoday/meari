use meari::{application::Language, parse};
use meari_native::{
    application::Renderer,
    domain::Viewport,
    infrastructure::{accessibility, document::InteractiveDocument},
};

#[test]
fn resize_preserves_input_focus_identity_and_rolls_back_invalid_screen() {
    let component=parse(r#"type Props={};export function Screen(props:Props){return <main><div id="screen"/><Input id="name" defaultValue="initial"/><Button id="save">Save</Button></main>;}"#,Language::Tsx).unwrap();
    let host = Viewport {
        width: 800,
        height: 600,
        dark: false,
    };
    let mut doc = InteractiveDocument::new(
        component,
        "#screen{width:50vw;height:50vh}input{width:100px;height:30px}",
        host.clone(),
    )
    .unwrap();
    let path = doc
        .elements()
        .iter()
        .find(|e| e.id == "name")
        .unwrap()
        .path
        .clone();
    doc.set_input_value(&path, "retained").unwrap();
    doc.focus_path(&path).unwrap();
    let node = doc.renderer.document.get_element_by_id("name").unwrap();
    doc.resize_screen(&host, "screen").unwrap();
    assert_eq!((doc.viewport.width, doc.viewport.height), (400, 300));
    assert_eq!(doc.renderer.document.get_element_by_id("name"), Some(node));
    assert_eq!(doc.session.focus.as_deref(), Some(path.as_str()));
    assert_eq!(doc.input_values[&path], "retained");
    assert!(!accessibility::tree(&doc).nodes.is_empty());
    assert!(doc.resize_screen(&host, "missing").is_err());
    assert_eq!(doc.renderer.document.get_viewport().window_size, (400, 300));
    doc.replace_stylesheet("#screen{width:0;height:0}").unwrap();
    assert!(doc.resize_screen(&host, "screen").is_err());
    assert_eq!((doc.viewport.width, doc.viewport.height), (400, 300));
    assert_eq!(
        doc.renderer.rasterize(&doc.viewport).unwrap().len(),
        400 * 300 * 4
    );
}

#[test]
fn host_commit_preserves_dpi_zoom_input_and_only_publishes_final_media_changes() {
    let component = parse(r#"type Props={};export function Screen(props:Props){return <main><Input id="name"/><Button id="save">Save</Button></main>;}"#, Language::Tsx).unwrap();
    let initial = Viewport {
        width: 800,
        height: 600,
        dark: false,
    };
    let mut document = InteractiveDocument::new(
        component,
        "@media(min-width:600px){#save{width:123px}}",
        initial,
    )
    .unwrap();
    let path = document
        .elements()
        .iter()
        .find(|element| element.id == "name")
        .unwrap()
        .path
        .clone();
    document.set_input_value(&path, "retained").unwrap();
    document.focus_path(&path).unwrap();
    let node = document
        .renderer
        .document
        .get_element_by_id("name")
        .unwrap();
    let mut environment: meari::domain::host::HostEnvironment = serde_json::from_value(serde_json::json!({
        "target":"native","layout":{"width":800,"height":600,"dark":false},
        "visual":{"width":800,"height":600,"offset_x":0,"offset_y":0,"scale":1},
        "device_scale":1.5,"layout_zoom":1.25,"safe_area":null,
        "capabilities":{"client_resize":"supported","preview_resize":"unsupported","reduced_motion":"unknown","contrast":"unknown","pointer_hover":"unknown","accessibility":"unknown"}
    })).unwrap();
    let first = document
        .commit_host_environment(environment.clone())
        .unwrap();
    let viewport = document.renderer.document.get_viewport();
    assert_eq!(viewport.window_size, (1500, 1125));
    assert_eq!(viewport.logical_size(), (800.0, 600.0));
    assert_eq!(viewport.hidpi_scale, 1.5);
    assert_eq!(viewport.zoom, 1.25);
    assert!(
        document
            .resize_screen(
                &Viewport {
                    width: 800,
                    height: 600,
                    dark: false
                },
                "name"
            )
            .is_err()
    );
    assert_eq!(document.renderer.document.get_viewport().zoom, 1.25);
    let subscription = document.subscribe_media("(min-width:600px)").unwrap();
    assert!(subscription.matches);
    use meari::{
        application::component_state::{ComponentState, media::MediaStateBindings},
        domain::component_state::{InstanceKey, StateSchema, StateValue},
    };
    let mut state = ComponentState::new("native-screen".into(), 1).unwrap();
    let schema: StateSchema =
        serde_json::from_value(serde_json::json!({"revision":"wide-v1","slots":{
            "wide":{"kind":"bool","initial":{"type":"bool","value":false}}
        }}))
        .unwrap();
    let owner = state
        .mount(None, InstanceKey::Position(0), "screen".into(), schema)
        .unwrap();
    let mut bindings = MediaStateBindings::new(owner, first.revision).unwrap();
    assert_eq!(
        bindings
            .bind(&mut state, &subscription, "wide".into())
            .unwrap()
            .values["wide"],
        StateValue::Bool(true)
    );
    environment.layout.width = 400;
    environment.visual.width = 400.0;
    let commit = document
        .commit_host_environment(environment.clone())
        .unwrap();
    assert_eq!(commit.media_changes.len(), 1);
    assert!(!commit.media_changes[0].matches);
    assert_eq!(
        bindings.apply(&mut state, &commit).unwrap().values["wide"],
        StateValue::Bool(false)
    );
    assert_eq!(document.viewport.width, 400);
    assert_eq!(
        document.renderer.document.get_element_by_id("name"),
        Some(node)
    );
    assert_eq!(document.input_values[&path], "retained");
    assert_eq!(document.session.focus.as_deref(), Some(path.as_str()));
    let save = document
        .elements()
        .iter()
        .find(|element| element.id == "save")
        .unwrap()
        .path
        .clone();
    document
        .dispatch(meari_native::domain::widgets::Action::Activate { path: save })
        .unwrap();
    assert_eq!(document.viewport.width, 400);
    environment.device_scale = 100.0;
    assert!(document.commit_host_environment(environment).is_err());
    assert_eq!(
        document.renderer.document.get_viewport().logical_size().0,
        400.0
    );
    assert_eq!(document.viewport.width, 400);
    assert_eq!(document.media_changes.len(), 1);
    assert!(document.unsubscribe_media(subscription.subscription));
    assert!(document.media_changes.is_empty());
}
