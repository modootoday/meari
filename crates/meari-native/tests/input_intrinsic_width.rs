//! Native text inputs take Chromium's `size=20` intrinsic width (avgCharWidth × 20 +
//! maxCharWidth − avgCharWidth, snapped up), and an author width still wins.
use meari_native::{
    application::Renderer, domain::Viewport, infrastructure::engine::NativeRenderer,
};

const VIEWPORT: Viewport = Viewport {
    width: 480,
    height: 120,
    dark: false,
};

fn installed(family: &str) -> bool {
    let mut collection = parley::fontique::Collection::new(parley::fontique::CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    collection.family_id(family).is_some()
}

fn layout(body: &str) -> NativeRenderer {
    let html = format!(
        "<html><head><style>body{{margin:0;font:16px 'Noto Sans'}}input{{font:inherit;\
         padding:0 12px;border:1px solid #000;box-sizing:border-box}}</style></head>\
         <body>{body}</body></html>"
    );
    let mut engine = NativeRenderer::from_html(&html);
    engine.layout(&VIEWPORT, 0.0).unwrap();
    engine
}

#[test]
fn auto_width_input_matches_chromium_and_does_not_flex_shrink() {
    if !installed("Noto Sans") {
        return; // the Chrome-measured widths below are for Noto Sans
    }
    // Chrome 2026-10-30: content 229px (Noto Sans 16px) + 24px padding + 2px border.
    let engine = layout(
        "<div style='display:flex;width:300px'><input id=a value=focus>\
         <button style='width:200px'>b</button></div><input id=b>",
    );
    assert_eq!(engine.bounds("a").unwrap().width, 255.0);
    assert_eq!(engine.bounds("b").unwrap().width, 255.0);
}

#[test]
fn author_width_and_min_width_override_the_intrinsic_size() {
    let engine = layout(
        "<input id=a style='width:100px'><div style='display:flex;width:120px'>\
         <input id=b style='min-width:0'><span style='width:100px;flex:none'></span></div>",
    );
    assert_eq!(engine.bounds("a").unwrap().width, 100.0);
    let b = engine.bounds("b").unwrap();
    // min-width:0 lets it shrink to its padding + border, as in Chrome.
    assert_eq!(b.width, 26.0, "{b:?}");
}
