//! Native internal scroll containers: wheel scrolling, clamping, `overflow: hidden` and the
//! overlay scrollbars, on the real blitz DOM behind InteractiveDocument (synthetic UI events).
use blitz_dom::Document;
use blitz_traits::events::*;
use keyboard_types::Modifiers;
use meari::application::Language;
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::document::InteractiveDocument,
};

const VIEWPORT: Viewport = Viewport {
    width: 400,
    height: 200,
    dark: false,
};
const CSS: &str = "body{margin:0}.box{position:absolute;top:0;width:100px;height:100px}\
#auto{left:0;overflow-y:auto}#hidden{left:200px;overflow:hidden}.tall{height:300px}";

fn document() -> InteractiveDocument {
    let source = meari::parse(
        "type Props={};export function S(props:Props){return <main><div id=\"auto\" className=\"box\"><div className=\"tall\">a</div></div><div id=\"hidden\" className=\"box\"><div className=\"tall\">b</div></div></main>}",
        Language::Tsx,
    )
    .unwrap();
    let mut doc = InteractiveDocument::new(source, CSS, VIEWPORT).unwrap();
    doc.renderer.rasterize(&VIEWPORT).unwrap();
    doc
}
fn coords(x: f32, y: f32) -> PointerCoords {
    PointerCoords {
        page_x: x,
        page_y: y,
        client_x: x,
        client_y: y,
        screen_x: x,
        screen_y: y,
    }
}
fn wheel(doc: &mut InteractiveDocument, x: f32, y: f32, dy: f64) {
    doc.handle_ui_event(UiEvent::PointerMove(BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords: coords(x, y),
        button: MouseEventButton::Main,
        buttons: MouseEventButtons::None,
        mods: Modifiers::empty(),
        details: Default::default(),
        element: Default::default(),
        active_pointers: Default::default(),
    }));
    doc.handle_ui_event(UiEvent::Wheel(BlitzWheelEvent {
        delta: BlitzWheelDelta::Pixels(0.0, dy),
        coords: coords(x, y),
        buttons: MouseEventButtons::None,
        mods: Modifiers::empty(),
        element: Default::default(),
    }));
}
fn offset_y(doc: &InteractiveDocument, id: &str) -> f64 {
    let node = doc.renderer.document.get_element_by_id(id).unwrap();
    doc.renderer
        .document
        .get_node(node)
        .unwrap()
        .scroll_offset()
        .y
}

#[test]
fn wheel_scrolls_auto_container_and_clamps_to_scrollable_extent() {
    let mut doc = document();
    wheel(&mut doc, 50.0, 50.0, -40.0);
    assert_eq!(offset_y(&doc, "auto"), 40.0);
    let node = doc.renderer.document.get_element_by_id("auto").unwrap();
    assert!(
        doc.renderer.document.scrollbar_opacity(node) > 0.0,
        "overlay scrollbar flashes"
    );
    wheel(&mut doc, 50.0, 50.0, -1000.0);
    assert_eq!(offset_y(&doc, "auto"), 200.0, "clamped to 300 - 100");
    wheel(&mut doc, 50.0, 50.0, 1000.0);
    assert_eq!(offset_y(&doc, "auto"), 0.0);
}

#[test]
fn overflow_hidden_clips_but_ignores_user_wheel() {
    let mut doc = document();
    wheel(&mut doc, 250.0, 50.0, -40.0);
    assert_eq!(offset_y(&doc, "hidden"), 0.0);
}
