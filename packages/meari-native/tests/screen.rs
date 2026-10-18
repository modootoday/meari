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
