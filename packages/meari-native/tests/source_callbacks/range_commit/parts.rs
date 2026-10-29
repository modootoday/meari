use super::*;
use meari_native::application::Renderer;
#[path = "../../../../meari/fixtures/range-parts/support.rs"]
mod support;
fn fixture() -> (InteractiveDocument, Host, String) {
    let (mut document, host) = mount_program(support::program("native-parts"), support::bindings());
    document.replace_stylesheet(".meari-parts-compact{position:fixed;left:20px;top:20px;width:72px;height:32px;color:#ff0000;--meari-range-thumb-color:#00ff00;--meari-range-fill-color:#ff0000;--meari-range-track-color:#0000ff;outline:0}").unwrap();
    let path = document
        .elements()
        .iter()
        .find(|element| {
            element.attributes.get("aria-label").map(String::as_str) == Some("parts-range")
        })
        .unwrap()
        .path
        .clone();
    (document, host, path)
}
fn update(document: &mut InteractiveDocument, value: &str, paths: &BTreeSet<String>) {
    let slot = document
        .node_slot(
            "workspace",
            support::reference(),
            support::reader_props(value),
        )
        .unwrap();
    document
        .update_module_range_props_with_text(support::root_props(slot), paths)
        .unwrap();
}
fn pixel(bytes: &[u8], width: u32, x: usize, y: usize) -> [u8; 4] {
    let index = (y * width as usize + x) * 4;
    bytes[index..index + 4].try_into().unwrap()
}
#[test]
fn native_compact_parts_share_nested_control_geometry_paint_and_retained_callback_owner() {
    let (mut document, mut host, path) = fixture();
    let identities = support::instances(document.module_projection().unwrap());
    assert_eq!(identities.len(), 3);
    let paths = support::display_paths(&document.module_projection().unwrap().component.root);
    let handlers = document.module_projection().unwrap().callback_bindings();
    let node = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&path).unwrap())
        .unwrap();
    let rect = document
        .renderer
        .document
        .get_client_bounding_rect(node)
        .unwrap();
    assert_eq!(rect.width, 72.0);
    assert_eq!(rect.height, 32.0);
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        56.0,
        MouseEventButtons::Primary,
    )));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("50".into()))]
    );
    update(&mut document, "50", &paths);
    assert_eq!(
        support::instances(document.module_projection().unwrap()),
        identities
    );
    assert_eq!(
        document.module_projection().unwrap().callback_bindings(),
        handlers
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    let viewport = document.viewport.clone();
    let bytes = document.renderer.rasterize(&viewport).unwrap();
    let fill = pixel(&bytes, viewport.width, 36, 36);
    assert!(
        fill[0] > 200 && fill[1] < 40 && fill[2] < 40,
        "fill {fill:?}"
    );
    let thumb = pixel(&bytes, viewport.width, 56, 36);
    assert!(
        thumb[1] > 200 && thumb[0] < 40 && thumb[2] < 40,
        "thumb {thumb:?}"
    );
    let track = pixel(&bytes, viewport.width, 76, 36);
    assert!(
        track[2] > 200 && track[0] < 40 && track[1] < 40,
        "track {track:?}"
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(56.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("50".into()))]
    );
    document.handle_ui_event(UiEvent::PointerDown(pointer(
        87.0,
        MouseEventButtons::Primary,
    )));
    assert!(document.last_error.is_none(), "{:?}", document.last_error);
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("100".into()))]
    );
    update(&mut document, "100", &paths);
    let bytes = document.renderer.rasterize(&viewport).unwrap();
    let fill = pixel(&bytes, viewport.width, 76, 36);
    assert!(
        fill[0] > 200 && fill[1] < 40 && fill[2] < 40,
        "updated fill {fill:?}"
    );
    document.handle_ui_event(UiEvent::PointerUp(pointer(87.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("100".into()))]
    );
}
#[test]
fn native_compact_hit_area_and_endpoints_use_ten_pixel_thumb_without_loosening_props_guard() {
    let (mut document, mut host, path) = fixture();
    let paths = support::display_paths(&document.module_projection().unwrap().component.root);
    let node = document
        .renderer
        .document
        .get_element_by_id(document.path_id(&path).unwrap())
        .unwrap();
    let mut down = pointer(25.0, MouseEventButtons::Primary);
    down.coords.client_y = 51.0;
    down.coords.page_y = 51.0;
    down.coords.screen_y = 51.0;
    document.handle_ui_event(UiEvent::PointerDown(down));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onDraft".into(), CallbackArguments::Text("0".into()))]
    );
    update(&mut document, "0", &paths);
    document.handle_ui_event(UiEvent::PointerUp(pointer(25.0, MouseEventButtons::None)));
    assert_eq!(
        deliver(&mut document, &mut host),
        vec![("onCommit".into(), CallbackArguments::Text("0".into()))]
    );
    let mut disabled = support::reader_props("0");
    disabled.insert("disabled".into(), Value::Bool(true));
    let slot = document
        .node_slot("workspace", support::reference(), disabled)
        .unwrap();
    assert!(
        document
            .update_module_range_props_with_text(support::root_props(slot), &paths)
            .is_err()
    );
    assert_eq!(
        document.renderer.document.get_focussed_node_id(),
        Some(node)
    );
    assert!(!document.has_pending_callbacks());
}
